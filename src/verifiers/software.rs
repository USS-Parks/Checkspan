//! The first software-check verifier: run the required checks on one exact
//! candidate and report what actually happened.
//!
//! The verifier is a protocol child of the verifier host: it reads one
//! request on
//! standard input and writes one response on standard output. What to run
//! comes from a **pinned validation profile** — a document whose digest the
//! contract pinned as the verifier digest — never from the candidate. The
//! candidate cannot change which programs run; it can only make them pass
//! or fail. The candidate is re-captured before and after the checks: a
//! subject that does not match the request, or that changes while checking,
//! ends the run without a verdict.

use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::adapters::code;
use crate::contracts::{
    CheckExecution, Conclusion, EnvironmentInfo, Exactly, Ident, RecordKind, SchemaVersion,
    SoftwareCheckOutcome, SoftwareCheckResult, Timestamp, Verdict, Version,
};
use crate::digests::artifact_digest;
use crate::evidence::code_selection;
use crate::verifier_host::{
    CheckOutcome, CheckReport, VerifierRequest, VerifierResponse, bounded_reader, kill_tree,
};

/// The verifier's own identity; the profile digest completes it.
pub const VERIFIER_ID: &str = "software_check";
/// The verifier's version.
pub const VERIFIER_VERSION: u32 = 1;

/// Exit code when the loaded profile does not match the pinned digest or
/// identity.
pub const EXIT_PROFILE: i32 = 4;
/// Exit code when the candidate does not match the request, or changed
/// while checking.
pub const EXIT_SUBJECT: i32 = 5;
/// Exit code when the request and profile cannot be reconciled: a required
/// check with no definition, or not exactly one code evidence entry.
pub const EXIT_CONTRACT: i32 = 6;
/// Exit code when standard input is not one well-formed request.
pub const EXIT_REQUEST: i32 = 7;

/// Most bytes of one check's standard output or error that are read.
const CHECK_OUTPUT_CAP: usize = 1 << 20;

/// One check definition inside the validation profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckSpec {
    /// The check this definition implements.
    pub id: Ident,
    /// The program, by explicit path. Never resolved through a shell, and
    /// never taken from the candidate.
    pub program: String,
    /// The exact argument vector.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Environment granted to this check, beyond the platform minimum.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<(String, String)>,
    /// Wall-clock ceiling in milliseconds.
    pub timeout_ms: u64,
}

/// The pinned validation profile: which checks exist and how each runs.
///
/// The profile is an approved document outside the candidate. Its exact
/// bytes are digested, and that digest is the verifier digest the contract
/// pins, so weakening a check changes the verifier identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationProfile {
    /// Always `1` for this build.
    pub profile: Exactly<1>,
    /// Must equal the pinned verifier id.
    pub id: Ident,
    /// Must equal the pinned verifier version.
    pub version: Version,
    /// The check definitions.
    pub checks: Vec<CheckSpec>,
}

/// Run the software-check verifier as a protocol child: request on standard
/// input, response on standard output, the typed result written to `out`.
/// Returns the process exit code.
pub fn run_child(profile_path: &Path, out: &Path) -> i32 {
    let mut text = String::new();
    if std::io::stdin().read_to_string(&mut text).is_err() {
        eprintln!("cannot read the request");
        return EXIT_REQUEST;
    }
    let request: VerifierRequest = match serde_json::from_str(&text) {
        Ok(request) => request,
        Err(e) => {
            eprintln!("standard input is not a request: {e}");
            return EXIT_REQUEST;
        }
    };

    // The profile's exact bytes must be the pinned verifier digest.
    let bytes = match fs::read(profile_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("cannot read the validation profile: {e}");
            return EXIT_PROFILE;
        }
    };
    let digest = artifact_digest(&bytes);
    if digest != request.verifier.digest {
        eprintln!(
            "validation profile digest is {digest}, the contract pins {}",
            request.verifier.digest
        );
        return EXIT_PROFILE;
    }
    let profile: ValidationProfile = match serde_json::from_slice(&bytes) {
        Ok(profile) => profile,
        Err(e) => {
            eprintln!("the validation profile is not readable: {e}");
            return EXIT_PROFILE;
        }
    };
    if profile.id != request.verifier.id || profile.version != request.verifier.version {
        eprintln!(
            "profile is {}@{}, the contract pins {}@{}",
            profile.id, profile.version, request.verifier.id, request.verifier.version
        );
        return EXIT_PROFILE;
    }

    // Exactly one piece of code evidence names the candidate.
    let mut candidates = request
        .evidence
        .iter()
        .filter_map(|reference| code_selection(&reference.locator).map(|c| (reference, c)));
    let Some((reference, (root, frozen))) = candidates.next() else {
        eprintln!("no code evidence names a candidate");
        return EXIT_CONTRACT;
    };
    if candidates.next().is_some() {
        eprintln!("more than one code evidence entry names a candidate");
        return EXIT_CONTRACT;
    }
    // The checks run against the checkout as it is on disk, so the capture
    // that must match the frozen subject is always the working tree against
    // the frozen base: a commit candidate requires a clean checkout at that
    // commit, and any edit to the checkout changes the captured subject even
    // when the frozen candidate was a commit.
    let selection = code::Selection {
        candidate: code::Selector::WorkingTree,
        base: frozen.base.clone(),
    };

    // Every required check must have a pinned definition.
    let mut specs = Vec::new();
    for id in &request.required_checks {
        match profile.checks.iter().find(|c| c.id == *id) {
            Some(spec) => specs.push(spec),
            None => {
                eprintln!("required check {id} has no definition in the pinned profile");
                return EXIT_CONTRACT;
            }
        }
    }

    // The candidate on disk must be the one the request froze.
    let capture_time = Timestamp::new("2026-01-01T00:00:00Z").expect("a fixed instant parses");
    let before = match code::capture(&root, &selection, &code::Limits::default(), &capture_time) {
        Ok(captured) => captured,
        Err(e) => {
            eprintln!("cannot capture the candidate: {e}");
            return EXIT_SUBJECT;
        }
    };
    let subject = format!("patch_subject:{}", before.subject_digest);
    if subject != request.subject || subject != reference.subject {
        eprintln!(
            "the candidate on disk is {subject}; the request froze {} and the evidence {}",
            request.subject, reference.subject
        );
        return EXIT_SUBJECT;
    }

    let checks: Vec<CheckExecution> = specs
        .iter()
        .map(|spec| run_check(spec, &before.root))
        .collect();

    // The candidate must not have moved while the checks ran.
    match code::capture(&root, &selection, &code::Limits::default(), &capture_time) {
        Ok(after) if after.subject_digest == before.subject_digest => {}
        Ok(after) => {
            eprintln!(
                "the candidate changed while checking: {} is now {}",
                before.subject_digest, after.subject_digest
            );
            return EXIT_SUBJECT;
        }
        Err(e) => {
            eprintln!("cannot re-capture the candidate: {e}");
            return EXIT_SUBJECT;
        }
    }

    let conclusion = if checks
        .iter()
        .all(|c| c.outcome == SoftwareCheckOutcome::Passed)
    {
        Conclusion::Accept
    } else {
        Conclusion::Reject
    };
    let result = SoftwareCheckResult {
        record: RecordKind::SoftwareCheckResult,
        schema_version: SchemaVersion(1),
        attempt: request.attempt.clone(),
        subject,
        candidate: before.result.candidate.clone(),
        base_commit: before.result.base_commit.clone(),
        repository: before.result.repository.clone(),
        profile: request.verifier.clone(),
        environment: EnvironmentInfo {
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
        },
        required_checks: request.required_checks.clone(),
        checks,
        conclusion,
    };
    debug_assert!(result.check_bindings().is_empty());
    let mut serialized = serde_json::to_vec_pretty(&result).expect("a result serializes");
    serialized.push(b'\n');
    if let Err(e) = fs::write(out, &serialized) {
        eprintln!("cannot write the result to {}: {e}", out.display());
        return EXIT_CONTRACT;
    }

    let failed: Vec<&str> = result
        .checks
        .iter()
        .filter(|c| c.outcome != SoftwareCheckOutcome::Passed)
        .map(|c| c.id.as_str())
        .collect();
    let response = VerifierResponse {
        protocol: Exactly,
        verifier: request.verifier.clone(),
        attempt: request.attempt.clone(),
        verdict: match conclusion {
            Conclusion::Accept => Verdict::Accept,
            Conclusion::Reject => Verdict::Reject,
        },
        checks: result
            .checks
            .iter()
            .map(|c| CheckReport {
                id: c.id.clone(),
                outcome: match c.outcome {
                    SoftwareCheckOutcome::Passed => CheckOutcome::Passed,
                    SoftwareCheckOutcome::Failed | SoftwareCheckOutcome::TimedOut => {
                        CheckOutcome::Failed
                    }
                },
                detail: c.detail.clone(),
            })
            .collect(),
        reason_code: (!failed.is_empty()).then(|| Ident::new("check_failed").expect("an ident")),
        reason_text: (!failed.is_empty()).then(|| format!("not passed: {}", failed.join(", "))),
    };
    print!(
        "{}",
        serde_json::to_string(&response).expect("a response serializes")
    );
    0
}

/// Run one pinned check in the candidate checkout and record what happened.
fn run_check(spec: &CheckSpec, checkout: &Path) -> CheckExecution {
    let started = Instant::now();
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(checkout)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
    let scratch = std::env::temp_dir();
    command
        .env("TMP", &scratch)
        .env("TEMP", &scratch)
        .env("TMPDIR", &scratch);
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);

    let failed = |detail: String, duration: Duration| CheckExecution {
        id: spec.id.clone(),
        outcome: SoftwareCheckOutcome::Failed,
        exit_code: Some(-1),
        duration_ms: duration.as_millis() as u64,
        stdout_digest: artifact_digest(b""),
        stderr_digest: artifact_digest(b""),
        detail: Some(detail),
    };
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            return failed(
                format!("cannot start {}: {e}", spec.program),
                started.elapsed(),
            );
        }
    };
    let stdout = bounded_reader(child.stdout.take().expect("piped"), CHECK_OUTPUT_CAP);
    let stderr = bounded_reader(child.stderr.take().expect("piped"), CHECK_OUTPUT_CAP);
    let deadline = started + Duration::from_millis(spec.timeout_ms);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => {
                kill_tree(&mut child);
                break None;
            }
        }
        if Instant::now() >= deadline {
            kill_tree(&mut child);
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let (stdout_bytes, _) = stdout.finish();
    let (stderr_bytes, _) = stderr.finish();
    let duration = started.elapsed();
    let (outcome, exit_code, detail) = match status {
        Some(status) if status.success() => (SoftwareCheckOutcome::Passed, status.code(), None),
        Some(status) => (
            SoftwareCheckOutcome::Failed,
            status.code().or(Some(-1)),
            Some(format!("exit status {:?}", status.code())),
        ),
        None => (
            SoftwareCheckOutcome::TimedOut,
            None,
            Some(format!("timed out after {}ms", spec.timeout_ms)),
        ),
    };
    CheckExecution {
        id: spec.id.clone(),
        outcome,
        exit_code,
        duration_ms: duration.as_millis() as u64,
        stdout_digest: artifact_digest(&stdout_bytes),
        stderr_digest: artifact_digest(&stderr_bytes),
        detail,
    }
}

/// The path the verifier subcommand uses, resolved for callers that need to
/// build a host profile for this binary.
pub fn child_args(profile: &Path, out: &Path) -> Vec<String> {
    vec![
        "software-verifier".to_owned(),
        "--profile".to_owned(),
        profile.display().to_string(),
        "--out".to_owned(),
        out.display().to_string(),
    ]
}
