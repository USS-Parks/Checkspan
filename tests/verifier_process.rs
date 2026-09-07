//! The bounded verifier process protocol, exercised with real native
//! processes.
//!
//! This test runs without the libtest harness: invoked normally it executes
//! its cases; invoked with `CHECKSPAN_VERIFIER_MODE` set it *is* the
//! verifier child, reading one request on standard input and behaving as the
//! mode dictates, so its standard output stays clean protocol.

mod common;

use std::fs;
use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use checkspan::contracts::{
    AttemptNumber, AttemptRef, Exactly, GraphId, Ident, NodeId, NodeRef, PolicyRef, Revision,
    RunId, Verdict, VerifierRef, Version,
};
use checkspan::digests::artifact_digest;
use checkspan::verifier_host::{
    CheckOutcome, CheckReport, Completion, ProcessFailure, VerifierProfile, VerifierRequest,
    VerifierResponse, run,
};
use common::fixture;

const MODE: &str = "CHECKSPAN_VERIFIER_MODE";
const HEARTBEAT: &str = "CHECKSPAN_HEARTBEAT_FILE";
const GRANTED: &str = "CHECKSPAN_GRANTED";
const SENTINEL: &str = "the-secret-sentinel-bytes";

fn main() {
    if let Ok(mode) = std::env::var(MODE) {
        child(&mode);
        return;
    }
    let cases: &[(&str, fn())] = &[
        (
            "fixtures_parse_and_round_trip",
            fixtures_parse_and_round_trip,
        ),
        (
            "an_accepting_verifier_round_trips",
            an_accepting_verifier_round_trips,
        ),
        (
            "a_rejecting_verifier_round_trips",
            a_rejecting_verifier_round_trips,
        ),
        (
            "a_nonzero_exit_is_never_a_verdict",
            a_nonzero_exit_is_never_a_verdict,
        ),
        (
            "malformed_and_extra_output_are_failures",
            malformed_and_extra_output_are_failures,
        ),
        (
            "oversized_output_is_a_failure",
            oversized_output_is_a_failure,
        ),
        ("the_environment_is_scrubbed", the_environment_is_scrubbed),
        (
            "the_working_directory_is_the_profile_cwd",
            the_working_directory_is_the_profile_cwd,
        ),
        ("a_wrong_echo_is_a_failure", a_wrong_echo_is_a_failure),
        (
            "a_timeout_kills_the_process_tree",
            a_timeout_kills_the_process_tree,
        ),
        (
            "cancellation_kills_the_process",
            cancellation_kills_the_process,
        ),
        (
            "a_mismatched_executable_never_spawns",
            a_mismatched_executable_never_spawns,
        ),
    ];
    let mut failed = 0;
    for (name, case) in cases {
        match catch_unwind(AssertUnwindSafe(case)) {
            Ok(()) => println!("case {name} ... ok"),
            Err(_) => {
                failed += 1;
                println!("case {name} ... FAILED");
            }
        }
    }
    println!(
        "verifier process suite: {} passed; {failed} failed",
        cases.len() - failed
    );
    if failed > 0 {
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------- child --

/// The verifier child. Modes that speak the protocol read the request
/// first; misbehaving modes misbehave exactly as named.
fn child(mode: &str) {
    if mode == "heartbeat" {
        let path = std::env::var(HEARTBEAT).expect("heartbeat file");
        loop {
            use std::io::Write;
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .expect("heartbeat file opens");
            file.write_all(b"beat\n").unwrap();
            drop(file);
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .expect("request arrives on stdin");
    let request: VerifierRequest = match serde_json::from_str(&text) {
        Ok(request) => request,
        Err(_) => std::process::exit(7),
    };
    let respond = |response: &VerifierResponse| {
        print!("{}", serde_json::to_string(response).unwrap());
    };
    match mode {
        "ok" => respond(&accept_response(&request)),
        "reject" => {
            let mut response = accept_response(&request);
            response.verdict = Verdict::Reject;
            response.checks[1].outcome = CheckOutcome::Failed;
            response.checks[1].detail = Some("2 warnings".to_owned());
            response.reason_code = Some(Ident::new("check_failed").unwrap());
            response.reason_text = Some("clippy failed".to_owned());
            eprintln!("clippy: 2 warnings");
            respond(&response);
        }
        "exit3" => {
            respond(&accept_response(&request));
            eprintln!("but the run itself broke");
            std::process::exit(3);
        }
        "malformed" => print!("this is not the protocol"),
        "extra" => {
            respond(&accept_response(&request));
            print!("\nsomething extra");
        }
        "huge" => {
            let line = [b'x'; 8192];
            for _ in 0..40 {
                use std::io::Write;
                std::io::stdout().write_all(&line).unwrap();
            }
        }
        "env" => {
            let mut response = accept_response(&request);
            let show = |key: &str| std::env::var(key).unwrap_or_else(|_| "ABSENT".to_owned());
            response.reason_text = Some(format!(
                "PATH={} CARGO_MANIFEST_DIR={} GRANTED={} TMP={}",
                show("PATH"),
                show("CARGO_MANIFEST_DIR"),
                show(GRANTED),
                show("TMP"),
            ));
            respond(&response);
        }
        "cwd" => {
            let mut response = accept_response(&request);
            response.reason_text = Some(std::env::current_dir().unwrap().display().to_string());
            respond(&response);
        }
        "wrong-attempt" => {
            let mut response = accept_response(&request);
            response.attempt.number = AttemptNumber::new(9).unwrap();
            respond(&response);
        }
        "hang" => std::thread::sleep(Duration::from_secs(60)),
        "tree" => {
            let heartbeat = std::env::var(HEARTBEAT).expect("heartbeat file");
            // The grandchild is deliberately left running: the host's tree
            // kill, not this child, must end it.
            #[allow(clippy::zombie_processes)]
            let _grandchild = Command::new(std::env::current_exe().unwrap())
                .env(MODE, "heartbeat")
                .env(HEARTBEAT, heartbeat)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("grandchild spawns");
            std::thread::sleep(Duration::from_secs(60));
        }
        other => panic!("unknown mode {other}"),
    }
}

fn accept_response(request: &VerifierRequest) -> VerifierResponse {
    VerifierResponse {
        protocol: Exactly,
        verifier: request.verifier.clone(),
        attempt: request.attempt.clone(),
        verdict: Verdict::Accept,
        checks: request
            .required_checks
            .iter()
            .map(|id| CheckReport {
                id: id.clone(),
                outcome: CheckOutcome::Passed,
                detail: None,
            })
            .collect(),
        reason_code: None,
        reason_text: None,
    }
}

// ---------------------------------------------------------------- host --

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("checkspan-verifier-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn request() -> VerifierRequest {
    VerifierRequest {
        protocol: Exactly,
        verifier: VerifierRef {
            id: Ident::new("software_check").unwrap(),
            version: Version::new(1).unwrap(),
            digest: artifact_digest(b"the pinned verifier profile"),
        },
        attempt: AttemptRef {
            run_id: RunId::new("run-0001").unwrap(),
            node: NodeRef {
                graph_id: GraphId::new("evidence_pilot").unwrap(),
                node_id: NodeId::new("cs_ci").unwrap(),
                revision: Revision::new(1).unwrap(),
            },
            number: AttemptNumber::new(1).unwrap(),
        },
        subject: format!("patch_subject:{}", artifact_digest(b"the candidate")),
        claim: "The required checks passed for this exact candidate.".to_owned(),
        required_checks: vec![
            Ident::new("fmt").unwrap(),
            Ident::new("clippy").unwrap(),
            Ident::new("test").unwrap(),
        ],
        policy_ref: PolicyRef {
            id: Ident::new("trusted_local").unwrap(),
            version: Version::new(1).unwrap(),
        },
        input_manifest_digest: artifact_digest(b"the frozen manifest"),
        evidence: vec![],
    }
}

fn profile(mode: &str, cwd: &Path) -> VerifierProfile {
    VerifierProfile {
        executable: std::env::current_exe().unwrap(),
        executable_digest: None,
        args: vec![],
        cwd: cwd.to_path_buf(),
        env: vec![(MODE.to_owned(), mode.to_owned())],
        timeout: Duration::from_secs(20),
        max_stdout_bytes: 64 * 1024,
        max_stderr_bytes: 16 * 1024,
    }
}

fn run_mode(mode: &str) -> Completion {
    let cwd = temp_dir(mode);
    run(
        &profile(mode, &cwd),
        &request(),
        &Arc::new(AtomicBool::new(false)),
    )
}

fn failure(completion: &Completion) -> &ProcessFailure {
    match &completion.outcome {
        Err(failure) => failure,
        Ok(response) => panic!("expected a process failure, got a verdict: {response:?}"),
    }
}

fn fixtures_parse_and_round_trip() {
    let request: VerifierRequest = serde_json::from_str(&fixture("verifier/request.json")).unwrap();
    assert_eq!(request.required_checks.len(), 3);
    let again: VerifierRequest =
        serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
    assert_eq!(again, request);

    for name in [
        "verifier/response-accept.json",
        "verifier/response-reject.json",
    ] {
        let response: VerifierResponse = serde_json::from_str(&fixture(name)).unwrap();
        let again: VerifierResponse =
            serde_json::from_str(&serde_json::to_string(&response).unwrap()).unwrap();
        assert_eq!(again, response, "{name}");
        assert_eq!(response.verifier, request.verifier, "{name}");
        assert_eq!(response.attempt, request.attempt, "{name}");
    }
    let accept: VerifierResponse =
        serde_json::from_str(&fixture("verifier/response-accept.json")).unwrap();
    assert_eq!(accept.verdict, Verdict::Accept);
    let reject: VerifierResponse =
        serde_json::from_str(&fixture("verifier/response-reject.json")).unwrap();
    assert_eq!(reject.verdict, Verdict::Reject);
    assert!(
        reject
            .checks
            .iter()
            .any(|c| c.outcome == CheckOutcome::Failed)
    );

    let wrong_protocol = fixture("verifier/response-wrong-protocol.json");
    let error = serde_json::from_str::<VerifierResponse>(&wrong_protocol).unwrap_err();
    assert!(error.to_string().contains("not supported"), "{error}");
    let unknown_field = fixture("verifier/response-unknown-field.json");
    let error = serde_json::from_str::<VerifierResponse>(&unknown_field).unwrap_err();
    assert!(error.to_string().contains("unknown field"), "{error}");
}

fn an_accepting_verifier_round_trips() {
    let completion = run_mode("ok");
    let response = completion.outcome.as_ref().expect("a verdict");
    assert_eq!(response.verdict, Verdict::Accept);
    assert_eq!(response.checks.len(), 3);
    assert!(
        response
            .checks
            .iter()
            .all(|c| c.outcome == CheckOutcome::Passed)
    );
    assert_eq!(response.verifier, request().verifier);
    assert_eq!(response.attempt, request().attempt);
    assert_eq!(completion.stderr, "");
    assert!(completion.duration < Duration::from_secs(20));
}

fn a_rejecting_verifier_round_trips() {
    let completion = run_mode("reject");
    let response = completion.outcome.as_ref().expect("a verdict");
    assert_eq!(response.verdict, Verdict::Reject);
    assert_eq!(response.checks[1].outcome, CheckOutcome::Failed);
    assert_eq!(
        response.reason_code.as_ref().unwrap().as_str(),
        "check_failed"
    );
    assert!(
        completion.stderr.contains("clippy: 2 warnings"),
        "{}",
        completion.stderr
    );
}

fn a_nonzero_exit_is_never_a_verdict() {
    let completion = run_mode("exit3");
    assert!(
        matches!(
            failure(&completion),
            ProcessFailure::NonZeroExit { code: Some(3) }
        ),
        "{}",
        failure(&completion)
    );
    assert!(completion.stderr.contains("but the run itself broke"));
}

fn malformed_and_extra_output_are_failures() {
    let completion = run_mode("malformed");
    assert!(
        matches!(failure(&completion), ProcessFailure::MalformedOutput { .. }),
        "{}",
        failure(&completion)
    );
    let completion = run_mode("extra");
    assert!(
        matches!(
            failure(&completion),
            ProcessFailure::ExtraOutput { trailing_bytes } if *trailing_bytes > 0
        ),
        "{}",
        failure(&completion)
    );
}

fn oversized_output_is_a_failure() {
    let completion = run_mode("huge");
    assert!(
        matches!(
            failure(&completion),
            ProcessFailure::OutputTooLarge { limit: 65536 }
        ),
        "{}",
        failure(&completion)
    );
}

fn the_environment_is_scrubbed() {
    // The host process has PATH and cargo's variables; the child must not.
    assert!(std::env::var("PATH").is_ok(), "the host itself has PATH");
    let cwd = temp_dir("env");
    let mut profile = profile("env", &cwd);
    profile.env.push((GRANTED.to_owned(), SENTINEL.to_owned()));
    let completion = run(&profile, &request(), &Arc::new(AtomicBool::new(false)));
    let response = completion.outcome.as_ref().expect("a verdict");
    let report = response.reason_text.as_ref().unwrap();
    assert!(report.contains("PATH=ABSENT"), "{report}");
    assert!(report.contains("CARGO_MANIFEST_DIR=ABSENT"), "{report}");
    assert!(report.contains(&format!("GRANTED={SENTINEL}")), "{report}");
    assert!(
        report.contains(&format!("TMP={}", cwd.display())),
        "{report}"
    );

    // What was not granted cannot appear anywhere the run retains.
    let completion = run_mode("ok");
    assert!(!completion.stderr.contains(SENTINEL));
    let response = completion.outcome.as_ref().expect("a verdict");
    let retained = serde_json::to_string(response).unwrap();
    assert!(!retained.contains(SENTINEL));
}

fn the_working_directory_is_the_profile_cwd() {
    let cwd = temp_dir("cwd");
    let completion = run(
        &profile("cwd", &cwd),
        &request(),
        &Arc::new(AtomicBool::new(false)),
    );
    let response = completion.outcome.as_ref().expect("a verdict");
    let reported = PathBuf::from(response.reason_text.as_ref().unwrap());
    assert_eq!(
        fs::canonicalize(&reported).unwrap(),
        fs::canonicalize(&cwd).unwrap()
    );
}

fn a_wrong_echo_is_a_failure() {
    let completion = run_mode("wrong-attempt");
    assert!(
        matches!(
            failure(&completion),
            ProcessFailure::EchoMismatch { field: "attempt" }
        ),
        "{}",
        failure(&completion)
    );
}

fn a_timeout_kills_the_process_tree() {
    let cwd = temp_dir("tree");
    let heartbeat = cwd.join("heartbeat.txt");
    let mut profile = profile("tree", &cwd);
    profile.timeout = Duration::from_millis(600);
    profile
        .env
        .push((HEARTBEAT.to_owned(), heartbeat.display().to_string()));
    let completion = run(&profile, &request(), &Arc::new(AtomicBool::new(false)));
    assert!(
        matches!(failure(&completion), ProcessFailure::TimedOut { .. }),
        "{}",
        failure(&completion)
    );
    assert!(completion.duration < Duration::from_secs(10));

    // The grandchild was writing heartbeats; after the tree is killed the
    // file stops growing.
    assert!(heartbeat.exists(), "the grandchild ran");
    std::thread::sleep(Duration::from_millis(400));
    let size_after_kill = fs::metadata(&heartbeat).unwrap().len();
    std::thread::sleep(Duration::from_millis(500));
    let size_later = fs::metadata(&heartbeat).unwrap().len();
    assert_eq!(
        size_later, size_after_kill,
        "no descendant survived the kill"
    );
}

fn cancellation_kills_the_process() {
    let cwd = temp_dir("cancel");
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        flag.store(true, Ordering::SeqCst);
    });
    let completion = run(&profile("hang", &cwd), &request(), &cancel);
    canceller.join().unwrap();
    assert!(
        matches!(failure(&completion), ProcessFailure::Cancelled),
        "{}",
        failure(&completion)
    );
    assert!(completion.duration < Duration::from_secs(10));
}

fn a_mismatched_executable_never_spawns() {
    let cwd = temp_dir("digest");
    let mut profile = profile("ok", &cwd);
    profile.executable_digest = Some(artifact_digest(b"a different program"));
    let completion = run(&profile, &request(), &Arc::new(AtomicBool::new(false)));
    assert!(
        matches!(
            failure(&completion),
            ProcessFailure::ExecutableMismatch {
                actual: Some(_),
                ..
            }
        ),
        "{}",
        failure(&completion)
    );
}
