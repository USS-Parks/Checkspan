//! The software-check verifier run as a real protocol child of the real
//! `checkspan` binary, spawning real check processes against real
//! repositories.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use checkspan::adapters::code::{self, Selection, Selector};
use checkspan::contracts::{
    AttemptNumber, AttemptRef, Conclusion, EvidenceRef, Exactly, GraphId, Ident, NodeId, NodeRef,
    PolicyRef, Provenance, Record, Revision, RunId, SoftwareCheckOutcome, SoftwareCheckResult,
    Timestamp, Verdict, VerifierRef, Version, parse_record,
};
use checkspan::digests::{artifact_digest, input_manifest_digest};
use checkspan::evidence::code_selection;
use checkspan::validation::{self, ValidatedRecord};
use checkspan::verifier_host::{Completion, ProcessFailure, VerifierProfile, VerifierRequest, run};
use checkspan::verifiers::software::{
    self, CheckSpec, EXIT_CONTRACT, EXIT_PROFILE, EXIT_SUBJECT, ValidationProfile, child_args,
};
use common::{fixture, fixture_path, schema_errors};

const SLEEPER: &str = "CHECKSPAN_SLEEPER";
const MUTATE: &str = "CHECKSPAN_MUTATE";

/// A check program for the timeout case: sleeps only when its gate variable
/// is set by a validation profile.
#[test]
fn sleeper_child() {
    if std::env::var(SLEEPER).is_ok() {
        std::thread::sleep(Duration::from_secs(30));
    }
}

/// A check program for the changed-candidate case: appends to the file its
/// gate variable names, then exits successfully.
#[test]
fn mutator_child() {
    if let Ok(path) = std::env::var(MUTATE) {
        use std::io::Write;
        let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
        file.write_all(b"\nmutated while checking\n").unwrap();
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("checkspan-swcheck-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Checkspan Fixture",
            "-c",
            "user.email=fixture@checkspan.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.autocrlf=false",
        ])
        .args(args)
        .output()
        .expect("git is installed");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A candidate repository whose `contract.json` is a valid record document.
fn repo(name: &str) -> PathBuf {
    let root = temp_dir(&format!("{name}-repo"));
    git(&root, &["init", "-q", "-b", "main"]);
    fs::write(root.join("README.md"), b"# candidate\n").unwrap();
    fs::write(
        root.join("contract.json"),
        fixture("contracts/valid/graph-run-minimal.json"),
    )
    .unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "base"]);
    root
}

fn checkspan_bin() -> String {
    env!("CARGO_BIN_EXE_checkspan").to_owned()
}

/// The pinned validation profile for these tests. `schema` validates the
/// candidate-owned document, `fixture` validates a profile-owned document
/// outside the candidate, `hang` sleeps past its ceiling, and `mutate`
/// edits the candidate and exits cleanly.
fn write_profile(dir: &Path, repo: &Path) -> (PathBuf, VerifierRef) {
    let me = std::env::current_exe().unwrap().display().to_string();
    let profile = ValidationProfile {
        profile: Exactly,
        id: Ident::new(software::VERIFIER_ID).unwrap(),
        version: Version::new(software::VERIFIER_VERSION).unwrap(),
        checks: vec![
            CheckSpec {
                id: Ident::new("schema").unwrap(),
                program: checkspan_bin(),
                args: vec!["validate".into(), "contract.json".into()],
                env: vec![],
                timeout_ms: 20_000,
            },
            CheckSpec {
                id: Ident::new("fixture").unwrap(),
                program: checkspan_bin(),
                args: vec![
                    "validate".into(),
                    fixture_path("contracts/valid/graph-run-minimal.json")
                        .display()
                        .to_string(),
                ],
                env: vec![],
                timeout_ms: 20_000,
            },
            CheckSpec {
                id: Ident::new("hang").unwrap(),
                program: me.clone(),
                args: vec![
                    "sleeper_child".into(),
                    "--exact".into(),
                    "--nocapture".into(),
                ],
                env: vec![(SLEEPER.into(), "1".into())],
                timeout_ms: 500,
            },
            CheckSpec {
                id: Ident::new("mutate").unwrap(),
                program: me,
                args: vec![
                    "mutator_child".into(),
                    "--exact".into(),
                    "--nocapture".into(),
                ],
                env: vec![(
                    MUTATE.into(),
                    repo.join("contract.json").display().to_string(),
                )],
                timeout_ms: 20_000,
            },
        ],
    };
    let path = dir.join("validation-profile.json");
    let bytes = serde_json::to_vec_pretty(&profile).unwrap();
    fs::write(&path, &bytes).unwrap();
    let pinned = VerifierRef {
        id: profile.id.clone(),
        version: profile.version,
        digest: artifact_digest(&bytes),
    };
    (path, pinned)
}

/// Freeze the candidate as evidence exactly the way the resolver writes it.
fn freeze(repo: &Path) -> (EvidenceRef, String) {
    let now = Timestamp::new("2026-09-06T12:00:00Z").unwrap();
    let captured = code::capture(
        repo,
        &Selection {
            candidate: Selector::WorkingTree,
            base: None,
        },
        &code::Limits::default(),
        &now,
    )
    .unwrap();
    let subject = format!("patch_subject:{}", captured.subject_digest);
    let locator = format!(
        "git:{}#base={}&candidate={}:{}",
        captured.root.display(),
        captured.result.base_commit,
        captured.result.candidate.kind.as_str(),
        captured.result.candidate.commit
    );
    assert!(code_selection(&locator).is_some(), "{locator}");
    let reference = EvidenceRef {
        port_name: Ident::new("candidate").unwrap(),
        locator,
        subject: subject.clone(),
        version: captured.result.candidate.commit.to_string(),
        content_digest: artifact_digest(&serde_json::to_vec(&captured.result).unwrap()),
        provenance: Provenance {
            producer: "checkspan/code-adapter".into(),
            attestation_ref: None,
        },
        policy_ref: PolicyRef {
            id: Ident::new("local_evidence").unwrap(),
            version: Version::new(1).unwrap(),
        },
        valid_until: None,
    };
    (reference, subject)
}

fn request(
    pinned: &VerifierRef,
    evidence: EvidenceRef,
    subject: String,
    checks: &[&str],
) -> VerifierRequest {
    VerifierRequest {
        protocol: Exactly,
        verifier: pinned.clone(),
        attempt: AttemptRef {
            run_id: RunId::new("run-0001").unwrap(),
            node: NodeRef {
                graph_id: GraphId::new("evidence_pilot").unwrap(),
                node_id: NodeId::new("cs_ci").unwrap(),
                revision: Revision::new(1).unwrap(),
            },
            number: AttemptNumber::new(1).unwrap(),
        },
        subject,
        claim: "The required checks passed for this exact candidate.".into(),
        required_checks: checks.iter().map(|c| Ident::new(*c).unwrap()).collect(),
        policy_ref: PolicyRef {
            id: Ident::new("trusted_local").unwrap(),
            version: Version::new(1).unwrap(),
        },
        input_manifest_digest: input_manifest_digest(std::slice::from_ref(&evidence), &[]).unwrap(),
        evidence: vec![evidence],
    }
}

/// Run the real `checkspan software-verifier` child under the CS-17 host.
/// `PATH` is granted so the verifier can find `git` and the platform's
/// process utilities; nothing else of the host environment is.
fn run_verifier(
    scratch: &Path,
    profile_path: &Path,
    request: &VerifierRequest,
) -> (Completion, PathBuf) {
    let out = scratch.join("software-check-result.json");
    let host = VerifierProfile {
        executable: PathBuf::from(checkspan_bin()),
        executable_digest: None,
        args: child_args(profile_path, &out),
        cwd: scratch.to_path_buf(),
        env: vec![("PATH".into(), std::env::var("PATH").unwrap())],
        timeout: Duration::from_secs(120),
        max_stdout_bytes: 1 << 20,
        max_stderr_bytes: 1 << 20,
    };
    let completion = run(&host, request, &Arc::new(AtomicBool::new(false)));
    (completion, out)
}

fn exit_code(completion: &Completion) -> i32 {
    match &completion.outcome {
        Err(ProcessFailure::NonZeroExit { code: Some(code) }) => *code,
        other => panic!(
            "expected a nonzero exit, got {other:?}: {}",
            completion.stderr
        ),
    }
}

#[test]
fn a_passing_candidate_yields_an_accepted_typed_result() {
    let scratch = temp_dir("pass");
    let repo = repo("pass");
    let (profile_path, pinned) = write_profile(&scratch, &repo);
    let (evidence, subject) = freeze(&repo);
    let request = request(&pinned, evidence, subject.clone(), &["schema", "fixture"]);

    let (completion, out) = run_verifier(&scratch, &profile_path, &request);
    let response = completion
        .outcome
        .as_ref()
        .unwrap_or_else(|e| panic!("expected a verdict: {e}; stderr: {}", completion.stderr));
    assert_eq!(response.verdict, Verdict::Accept);
    assert_eq!(response.verifier, request.verifier);
    assert_eq!(response.attempt, request.attempt);

    let text = fs::read_to_string(&out).unwrap();
    assert_eq!(
        schema_errors(SoftwareCheckResult::SCHEMA_ID, &text),
        Vec::<String>::new()
    );
    let result: SoftwareCheckResult = parse_record(&text).unwrap();
    assert_eq!(result.check_bindings(), vec![]);
    assert_eq!(result.conclusion, Conclusion::Accept);
    assert_eq!(result.subject, subject);
    assert_eq!(result.attempt, request.attempt);
    assert_eq!(result.profile, request.verifier);
    assert_eq!(result.environment.os, std::env::consts::OS);
    assert_eq!(result.environment.arch, std::env::consts::ARCH);
    assert_eq!(result.checks.len(), 2);
    for check in &result.checks {
        assert_eq!(
            check.outcome,
            SoftwareCheckOutcome::Passed,
            "{:?}",
            check.id
        );
        assert_eq!(check.exit_code, Some(0), "{:?}", check.id);
    }
    let report = validation::validate(text.as_bytes(), &validation::Limits::default());
    assert!(report.is_valid(), "{:?}", report.diagnostics);
    assert!(matches!(
        report.record,
        Some(ValidatedRecord::SoftwareCheckResult(ref r)) if *r == result
    ));
}

#[test]
fn a_failing_candidate_is_rejected_with_the_failed_check() {
    let scratch = temp_dir("fail");
    let repo = repo("fail");
    // The candidate weakens its own checked document; the pinned check
    // still runs and now fails. Commit so the tree is clean.
    fs::write(repo.join("contract.json"), b"{\"record\": \"graph_run\"}\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "weaken"]);
    let (profile_path, pinned) = write_profile(&scratch, &repo);
    let (evidence, subject) = freeze(&repo);
    let request = request(&pinned, evidence, subject, &["schema", "fixture"]);

    let (completion, out) = run_verifier(&scratch, &profile_path, &request);
    let response = completion
        .outcome
        .as_ref()
        .unwrap_or_else(|e| panic!("expected a verdict: {e}; stderr: {}", completion.stderr));
    assert_eq!(response.verdict, Verdict::Reject);
    assert!(
        response.reason_text.as_ref().unwrap().contains("schema"),
        "{:?}",
        response.reason_text
    );
    let result: SoftwareCheckResult = parse_record(&fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(result.conclusion, Conclusion::Reject);
    assert_eq!(result.check_bindings(), vec![]);
    let schema = result
        .checks
        .iter()
        .find(|c| c.id.as_str() == "schema")
        .unwrap();
    assert_eq!(schema.outcome, SoftwareCheckOutcome::Failed);
    assert_eq!(schema.exit_code, Some(1));
    let fixture_check = result
        .checks
        .iter()
        .find(|c| c.id.as_str() == "fixture")
        .unwrap();
    assert_eq!(fixture_check.outcome, SoftwareCheckOutcome::Passed);
}

#[test]
fn a_tampered_profile_cannot_run_a_single_check() {
    let scratch = temp_dir("tamper");
    let repo = repo("tamper");
    let (profile_path, pinned) = write_profile(&scratch, &repo);
    let (evidence, subject) = freeze(&repo);
    let request = request(&pinned, evidence, subject, &["schema"]);

    // Weaken the profile after its digest was pinned.
    let weakened = fs::read_to_string(&profile_path)
        .unwrap()
        .replace("contract.json", "README.md");
    fs::write(&profile_path, weakened).unwrap();

    let (completion, out) = run_verifier(&scratch, &profile_path, &request);
    assert_eq!(
        exit_code(&completion),
        EXIT_PROFILE,
        "{}",
        completion.stderr
    );
    assert!(
        completion.stderr.contains("the contract pins"),
        "{}",
        completion.stderr
    );
    assert!(!out.exists(), "no result may be written");
}

#[test]
fn a_required_check_without_a_definition_cannot_pass() {
    let scratch = temp_dir("missing");
    let repo = repo("missing");
    let (profile_path, pinned) = write_profile(&scratch, &repo);
    let (evidence, subject) = freeze(&repo);
    let request = request(&pinned, evidence, subject, &["schema", "coverage"]);

    let (completion, out) = run_verifier(&scratch, &profile_path, &request);
    assert_eq!(
        exit_code(&completion),
        EXIT_CONTRACT,
        "{}",
        completion.stderr
    );
    assert!(
        completion.stderr.contains("coverage"),
        "{}",
        completion.stderr
    );
    assert!(!out.exists());
}

#[test]
fn a_hanging_check_times_out_and_rejects() {
    let scratch = temp_dir("hang");
    let repo = repo("hang");
    let (profile_path, pinned) = write_profile(&scratch, &repo);
    let (evidence, subject) = freeze(&repo);
    let request = request(&pinned, evidence, subject, &["fixture", "hang"]);

    let (completion, out) = run_verifier(&scratch, &profile_path, &request);
    let response = completion
        .outcome
        .as_ref()
        .unwrap_or_else(|e| panic!("expected a verdict: {e}; stderr: {}", completion.stderr));
    assert_eq!(response.verdict, Verdict::Reject);
    let result: SoftwareCheckResult = parse_record(&fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(result.check_bindings(), vec![]);
    let hang = result
        .checks
        .iter()
        .find(|c| c.id.as_str() == "hang")
        .unwrap();
    assert_eq!(hang.outcome, SoftwareCheckOutcome::TimedOut);
    assert_eq!(hang.exit_code, None);
    assert!(hang.duration_ms >= 500, "{}", hang.duration_ms);
    assert!(
        hang.detail.as_ref().unwrap().contains("timed out"),
        "{:?}",
        hang.detail
    );
}

#[test]
fn a_candidate_changed_during_checking_is_refused_without_a_verdict() {
    let scratch = temp_dir("mutate");
    let repo = repo("mutate");
    let (profile_path, pinned) = write_profile(&scratch, &repo);
    let (evidence, subject) = freeze(&repo);
    let request = request(&pinned, evidence, subject, &["mutate", "schema"]);

    let (completion, out) = run_verifier(&scratch, &profile_path, &request);
    assert_eq!(
        exit_code(&completion),
        EXIT_SUBJECT,
        "{}",
        completion.stderr
    );
    assert!(
        completion.stderr.contains("changed while checking"),
        "{}",
        completion.stderr
    );
    assert!(!out.exists(), "an invalidated run leaves no result");
}

#[test]
fn a_stale_candidate_is_refused_before_any_check_runs() {
    let scratch = temp_dir("stale");
    let repo = repo("stale");
    let (profile_path, pinned) = write_profile(&scratch, &repo);
    let (evidence, subject) = freeze(&repo);
    // The checkout moves after the evidence was frozen.
    fs::write(repo.join("README.md"), b"# drifted\n").unwrap();
    let request = request(&pinned, evidence, subject, &["schema"]);

    let (completion, out) = run_verifier(&scratch, &profile_path, &request);
    assert_eq!(
        exit_code(&completion),
        EXIT_SUBJECT,
        "{}",
        completion.stderr
    );
    assert!(
        completion.stderr.contains("the request froze"),
        "{}",
        completion.stderr
    );
    assert!(!out.exists());
}
