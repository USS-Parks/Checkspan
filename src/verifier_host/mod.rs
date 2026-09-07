//! The bounded verifier process protocol.
//!
//! A verifier is a separate native process. The host pins its executable,
//! argument vector, working directory, and environment, hands it one typed
//! request on standard input, and reads exactly one typed response from
//! standard output. Everything else the process can do — exit nonzero, print
//! something that is not the protocol, print too much, hang, or leave
//! descendants — is a **process failure**, reported as such and never as a
//! verdict.
//!
//! The environment is scrubbed: the child sees only what the profile grants,
//! never the controller's environment. Standard output and standard error
//! are bounded; a timeout or cancellation kills the child and its process
//! tree. This is trusted-local execution under an explicit profile. It is
//! not a security sandbox, and nothing here claims isolation from a hostile
//! verifier.

use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::contracts::{
    AttemptRef, Digest, EvidenceRef, Exactly, Ident, PolicyRef, Verdict, VerifierRef,
};
use crate::digests::artifact_digest;

/// The wire protocol version this build speaks.
pub const PROTOCOL_VERSION: u32 = 1;

/// Everything a verifier is told. It receives evidence references and frozen
/// context; it is not handed the controller's environment, store, or
/// authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierRequest {
    /// Always `1` for this build.
    pub protocol: Exactly<1>,
    /// The verifier the contract pinned; the response must echo it.
    pub verifier: VerifierRef,
    /// The exact attempt under verification; the response must echo it.
    pub attempt: AttemptRef,
    /// The exact subject under verification.
    pub subject: String,
    /// The pinned acceptance claim.
    pub claim: String,
    /// Checks the verifier must run.
    pub required_checks: Vec<Ident>,
    /// Verification policy in force.
    pub policy_ref: PolicyRef,
    /// Digest of the frozen inputs.
    pub input_manifest_digest: Digest,
    /// The frozen evidence references.
    pub evidence: Vec<EvidenceRef>,
}

/// How one required check ended inside a completed verifier run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckOutcome {
    /// The check ran and passed.
    Passed,
    /// The check ran and failed.
    Failed,
    /// The check did not run. A skipped required check never passes.
    Skipped,
}

impl CheckOutcome {
    /// The wire text.
    pub fn as_str(self) -> &'static str {
        match self {
            CheckOutcome::Passed => "passed",
            CheckOutcome::Failed => "failed",
            CheckOutcome::Skipped => "skipped",
        }
    }
}

/// One check's outcome as the verifier reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckReport {
    /// The check.
    pub id: Ident,
    /// How it ended.
    pub outcome: CheckOutcome,
    /// Bounded human-readable detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The one document a verifier writes to standard output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierResponse {
    /// Always `1` for this build.
    pub protocol: Exactly<1>,
    /// Must equal the request's verifier.
    pub verifier: VerifierRef,
    /// Must equal the request's attempt.
    pub attempt: AttemptRef,
    /// What the verifier concluded about its exact subject.
    pub verdict: Verdict,
    /// Per-check outcomes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<CheckReport>,
    /// Machine-readable reason class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<Ident>,
    /// Bounded human-readable reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_text: Option<String>,
}

/// The pinned execution profile of one registered verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierProfile {
    /// The executable, by explicit path. Never resolved through a shell.
    pub executable: PathBuf,
    /// Digest the executable bytes must have, when pinned.
    pub executable_digest: Option<Digest>,
    /// The exact argument vector. No interpolation of any kind.
    pub args: Vec<String>,
    /// The working directory.
    pub cwd: PathBuf,
    /// The only environment the child sees, beyond the platform minimum
    /// (`SystemRoot` on Windows) and scratch variables pointed at `cwd`.
    pub env: Vec<(String, String)>,
    /// Wall-clock ceiling for the whole run.
    pub timeout: Duration,
    /// Most standard-output bytes accepted.
    pub max_stdout_bytes: usize,
    /// Most standard-error bytes retained; anything beyond is discarded.
    pub max_stderr_bytes: usize,
}

/// Why a verifier run is not a verdict.
#[derive(Debug)]
pub enum ProcessFailure {
    /// The pinned executable's bytes do not match the profile.
    ExecutableMismatch {
        /// The digest pinned.
        expected: Digest,
        /// The digest found, when the file could be read.
        actual: Option<Digest>,
    },
    /// The process could not be started.
    Spawn(std::io::Error),
    /// The wall-clock ceiling passed; the process tree was killed.
    TimedOut {
        /// The ceiling.
        after: Duration,
    },
    /// The caller cancelled; the process tree was killed.
    Cancelled,
    /// The process exited with a failure status. Anything it printed is not
    /// a verdict.
    NonZeroExit {
        /// The exit code, when the platform reports one.
        code: Option<i32>,
    },
    /// Standard output exceeded the ceiling; the process tree was killed.
    OutputTooLarge {
        /// The ceiling.
        limit: usize,
    },
    /// Standard output is not exactly one well-formed response document.
    MalformedOutput {
        /// What the parser said.
        error: String,
    },
    /// A well-formed response was followed by more output.
    ExtraOutput {
        /// Bytes beyond the first document.
        trailing_bytes: usize,
    },
    /// The response does not echo the request.
    EchoMismatch {
        /// Which field differs.
        field: &'static str,
    },
}

impl fmt::Display for ProcessFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcessFailure::ExecutableMismatch { expected, actual } => match actual {
                Some(actual) => write!(f, "executable digest is {actual}, profile pins {expected}"),
                None => write!(f, "executable cannot be read; profile pins {expected}"),
            },
            ProcessFailure::Spawn(e) => write!(f, "cannot start the verifier: {e}"),
            ProcessFailure::TimedOut { after } => {
                write!(
                    f,
                    "timed out after {}ms; process tree killed",
                    after.as_millis()
                )
            }
            ProcessFailure::Cancelled => f.write_str("cancelled; process tree killed"),
            ProcessFailure::NonZeroExit { code } => match code {
                Some(code) => write!(f, "exited with status {code}"),
                None => f.write_str("terminated without an exit code"),
            },
            ProcessFailure::OutputTooLarge { limit } => {
                write!(
                    f,
                    "wrote more than {limit} bytes of output; process tree killed"
                )
            }
            ProcessFailure::MalformedOutput { error } => {
                write!(f, "output is not a response document: {error}")
            }
            ProcessFailure::ExtraOutput { trailing_bytes } => write!(
                f,
                "{trailing_bytes} bytes of output follow the response document"
            ),
            ProcessFailure::EchoMismatch { field } => {
                write!(f, "response does not echo the request's {field}")
            }
        }
    }
}

impl std::error::Error for ProcessFailure {}

/// One finished verifier run: a verdict, or the failure that is not one,
/// plus bounded diagnostics.
#[derive(Debug)]
pub struct Completion {
    /// The verdict, or the process failure.
    pub outcome: Result<VerifierResponse, ProcessFailure>,
    /// Bounded standard error, lossily decoded.
    pub stderr: String,
    /// How long the run took.
    pub duration: Duration,
}

/// How often the wait loop polls the child and the cancellation flag.
const POLL: Duration = Duration::from_millis(10);

/// Run one verifier under its pinned profile and hand it `request`.
///
/// `cancel` is polled while waiting; setting it kills the process tree and
/// reports [`ProcessFailure::Cancelled`]. The child's environment is the
/// profile's alone. This does not sandbox the verifier; it bounds and
/// classifies what a trusted-local one does.
pub fn run(
    profile: &VerifierProfile,
    request: &VerifierRequest,
    cancel: &Arc<AtomicBool>,
) -> Completion {
    let started = Instant::now();
    let fail = |outcome: ProcessFailure, stderr: String| Completion {
        outcome: Err(outcome),
        stderr,
        duration: started.elapsed(),
    };

    if let Some(expected) = &profile.executable_digest {
        let actual = fs::read(&profile.executable)
            .ok()
            .map(|bytes| artifact_digest(&bytes));
        if actual.as_ref() != Some(expected) {
            return fail(
                ProcessFailure::ExecutableMismatch {
                    expected: expected.clone(),
                    actual,
                },
                String::new(),
            );
        }
    }

    let request_bytes = serde_json::to_vec(request).expect("a request serializes");
    let mut command = Command::new(&profile.executable);
    command
        .args(&profile.args)
        .current_dir(&profile.cwd)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
    let scratch = profile.cwd.as_os_str();
    command
        .env("TMP", scratch)
        .env("TEMP", scratch)
        .env("TMPDIR", scratch);
    for (key, value) in &profile.env {
        command.env(key, value);
    }
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => return fail(ProcessFailure::Spawn(e), String::new()),
    };

    // The request is written from its own thread so a child that never reads
    // cannot block the host; the handle is dropped to close the pipe.
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&request_bytes);
    });
    let stdout = child.stdout.take().expect("stdout is piped");
    let stdout_reader = bounded_reader(stdout, profile.max_stdout_bytes);
    let stderr = child.stderr.take().expect("stderr is piped");
    let stderr_reader = bounded_reader(stderr, profile.max_stderr_bytes);

    let deadline = started + profile.timeout;
    let (status, failure) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (Some(status), None),
            Ok(None) => {}
            Err(e) => {
                kill_tree(&mut child);
                break (None, Some(ProcessFailure::Spawn(e)));
            }
        }
        if stdout_reader.overflowed() {
            kill_tree(&mut child);
            break (
                None,
                Some(ProcessFailure::OutputTooLarge {
                    limit: profile.max_stdout_bytes,
                }),
            );
        }
        if cancel.load(Ordering::SeqCst) {
            kill_tree(&mut child);
            break (None, Some(ProcessFailure::Cancelled));
        }
        if Instant::now() >= deadline {
            kill_tree(&mut child);
            break (
                None,
                Some(ProcessFailure::TimedOut {
                    after: profile.timeout,
                }),
            );
        }
        std::thread::sleep(POLL);
    };
    let _ = writer.join();
    let (stdout_bytes, stdout_overflow) = stdout_reader.finish();
    let (stderr_bytes, _) = stderr_reader.finish();
    let stderr_text = String::from_utf8_lossy(&stderr_bytes).into_owned();

    if let Some(failure) = failure {
        return fail(failure, stderr_text);
    }
    let status = status.expect("either a status or a failure");
    if !status.success() {
        return fail(
            ProcessFailure::NonZeroExit {
                code: status.code(),
            },
            stderr_text,
        );
    }
    if stdout_overflow {
        return fail(
            ProcessFailure::OutputTooLarge {
                limit: profile.max_stdout_bytes,
            },
            stderr_text,
        );
    }
    let outcome = parse_response(&stdout_bytes).and_then(|response| {
        if response.verifier != request.verifier {
            Err(ProcessFailure::EchoMismatch { field: "verifier" })
        } else if response.attempt != request.attempt {
            Err(ProcessFailure::EchoMismatch { field: "attempt" })
        } else {
            Ok(response)
        }
    });
    Completion {
        outcome,
        stderr: stderr_text,
        duration: started.elapsed(),
    }
}

/// Exactly one response document, with nothing but whitespace after it.
fn parse_response(bytes: &[u8]) -> Result<VerifierResponse, ProcessFailure> {
    let mut stream = serde_json::Deserializer::from_slice(bytes).into_iter::<VerifierResponse>();
    let response = match stream.next() {
        Some(Ok(response)) => response,
        Some(Err(e)) => {
            return Err(ProcessFailure::MalformedOutput {
                error: e.to_string(),
            });
        }
        None => {
            return Err(ProcessFailure::MalformedOutput {
                error: "standard output is empty".to_owned(),
            });
        }
    };
    let trailing = &bytes[stream.byte_offset()..];
    let trailing_bytes = trailing.iter().filter(|b| !b.is_ascii_whitespace()).count();
    if trailing_bytes > 0 {
        return Err(ProcessFailure::ExtraOutput { trailing_bytes });
    }
    Ok(response)
}

/// A pipe drained on its own thread: bytes are kept up to the cap and
/// discarded past it, so a talkative child never blocks on a full pipe.
pub(crate) struct BoundedReader {
    handle: std::thread::JoinHandle<(Vec<u8>, bool)>,
    overflow: mpsc::Receiver<()>,
    seen_overflow: std::cell::Cell<bool>,
}

pub(crate) fn bounded_reader<R: Read + Send + 'static>(mut source: R, cap: usize) -> BoundedReader {
    let (sender, overflow) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut chunk = [0u8; 8192];
        let mut warned = false;
        loop {
            match source.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let room = cap.saturating_sub(kept.len());
                    kept.extend_from_slice(&chunk[..n.min(room)]);
                    if n > room && !warned {
                        warned = true;
                        let _ = sender.send(());
                    }
                }
            }
        }
        (kept, warned)
    });
    BoundedReader {
        handle,
        overflow,
        seen_overflow: std::cell::Cell::new(false),
    }
}

impl BoundedReader {
    pub(crate) fn overflowed(&self) -> bool {
        if self.overflow.try_recv().is_ok() {
            self.seen_overflow.set(true);
        }
        self.seen_overflow.get()
    }

    pub(crate) fn finish(self) -> (Vec<u8>, bool) {
        self.handle.join().unwrap_or_default()
    }
}

/// Kill the child and every descendant it started.
///
/// On Windows the tree is terminated through `taskkill /T /F` with an
/// explicit argument vector. On Unix the child is spawned as its own process
/// group and the group is signalled through the `kill` utility. Both are
/// best-effort cleanup of a trusted-local process, not containment.
pub(crate) fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}
