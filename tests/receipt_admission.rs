//! Receipt admission: acceptance is recorded only for a matching, trusted
//! checker result, checked and written in one transaction against a real
//! SQLite ledger.

mod common;

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use checkspan::adapters::code::{self, Selection, Selector};
use checkspan::contracts::{
    Attempt, EvidenceRef, Exactly, GraphRun, GraphSpec, Ident, NodeId, NodeStatus, PolicyRef,
    Provenance, ReceiptRef, RecordKind, RunId, SchemaVersion, Timestamp, Validity, Verdict,
    VerifierReceipt, Version, parse_record,
};
use checkspan::digests::artifact_digest;
use checkspan::receipts::{AdmitError, IssueError, admit, issue};
use checkspan::state::{RunState, kinds};
use checkspan::store::Store;
use checkspan::verifier_host::{
    VerifierProfile, VerifierRequest, VerifierResponse, run as run_verifier,
};
use checkspan::verifiers::software::{self, CheckSpec, ValidationProfile, child_args};
use common::fixture;
use serde_json::json;

const NOW: &str = "2026-09-06T15:00:00Z";
const LATER: &str = "2026-10-02T00:00:00Z";

fn ts(s: &str) -> Timestamp {
    Timestamp::new(s).unwrap()
}

fn node(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}

fn ident(s: &str) -> Ident {
    Ident::new(s).unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("checkspan-receipts-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_record(run_id: &str, spec: &GraphSpec) -> GraphRun {
    GraphRun {
        record: RecordKind::GraphRun,
        schema_version: SchemaVersion(1),
        run_id: RunId::new(run_id).unwrap(),
        graph_ref: spec.graph_ref(),
        budget_lineage_ref: None,
        admitted_imports: vec![],
    }
}

/// Everything the admission tests need: the review graph with cs_patch
/// accepted (receipt valid until 2026-10-01) and cs_ci's attempt 1 sealed
/// as completed with a result and a frozen candidate subject.
struct Setup {
    store: Store,
    spec: GraphSpec,
    run: GraphRun,
    attempt: Attempt,
    subject: String,
}

fn seeded(name: &str) -> Setup {
    let dir = temp_dir(name);
    let mut store = Store::open(dir.join("ledger.sqlite")).unwrap();
    let spec: GraphSpec = parse_record(&fixture("contracts/valid/graph-spec-full.json")).unwrap();
    store.store_graph(&spec).unwrap();
    let run = run_record("run-0001", &spec);
    store.create_run(&run).unwrap();

    // cs_patch: dispatched, sealed, accepted.
    let patch = node("cs_patch");
    let patch_digest = artifact_digest(b"the exact patch bytes");
    store
        .transaction(|tx| {
            tx.append_event(
                &run.run_id,
                Some(&patch),
                kinds::ATTEMPT_DISPATCHED,
                &json!({"number": 1}),
            )?;
            Ok(())
        })
        .unwrap();
    let mut patch_attempt: Attempt = parse_record(&fixture(
        "outcomes/valid/attempt-completed-with-receipt.json",
    ))
    .unwrap();
    patch_attempt.node = spec.node_ref(&patch).unwrap();
    patch_attempt.verifier_receipt = None;
    patch_attempt.dependency_receipts.clear();
    let result = patch_attempt.result.as_mut().unwrap();
    result.result_type = spec.nodes[0].result_type.clone();
    result.digest = patch_digest.clone();
    store.seal_attempt(&patch_attempt).unwrap();
    let mut patch_receipt: VerifierReceipt =
        parse_record(&fixture("outcomes/valid/receipt-accept.json")).unwrap();
    patch_receipt.id = ident("rcpt-patch-1");
    patch_receipt.attempt = patch_attempt.attempt_ref();
    patch_receipt.result_digest = patch_digest;
    patch_receipt.verifier = spec.nodes[0].acceptance.verifier.clone();
    patch_receipt.policy_ref = spec.nodes[0].acceptance.policy_ref.clone();
    patch_receipt.validity.valid_until = Some(ts("2026-10-01T00:00:00Z"));
    store.admit_receipt(&patch_receipt).unwrap();

    // cs_ci: dispatched and sealed as completed, awaiting its receipt.
    let subject = format!("patch_subject:{}", artifact_digest(b"the candidate"));
    let attempt = seal_ci_attempt(&mut store, &spec, &run, 1, &subject);
    Setup {
        store,
        spec,
        run,
        attempt,
        subject,
    }
}

fn seal_ci_attempt(
    store: &mut Store,
    spec: &GraphSpec,
    run: &GraphRun,
    number: u32,
    subject: &str,
) -> Attempt {
    let ci = node("cs_ci");
    store
        .transaction(|tx| {
            tx.append_event(
                &run.run_id,
                Some(&ci),
                kinds::ATTEMPT_DISPATCHED,
                &json!({"number": number}),
            )?;
            Ok(())
        })
        .unwrap();
    let mut attempt: Attempt = parse_record(&fixture(
        "outcomes/valid/attempt-completed-with-receipt.json",
    ))
    .unwrap();
    attempt.node = spec.node_ref(&ci).unwrap();
    attempt.number = checkspan::contracts::AttemptNumber::new(number).unwrap();
    attempt.verifier_receipt = None;
    attempt.dependency_receipts = vec![ReceiptRef {
        id: ident("rcpt-patch-1"),
        run_id: run.run_id.clone(),
        node: spec.node_ref(&node("cs_patch")).unwrap(),
    }];
    attempt.input_manifest = vec![EvidenceRef {
        port_name: ident("candidate"),
        locator: "git:C:/pilot#base=aaaa&candidate=commit:bbbb".into(),
        subject: subject.to_owned(),
        version: "bbbb".into(),
        content_digest: artifact_digest(b"the canonical candidate record"),
        provenance: Provenance {
            producer: "checkspan/code-adapter".into(),
            attestation_ref: None,
        },
        policy_ref: PolicyRef {
            id: ident("local_evidence"),
            version: Version::new(1).unwrap(),
        },
        valid_until: None,
    }];
    let ci_node = spec.nodes.iter().find(|n| n.id == ci).unwrap();
    let result = attempt.result.as_mut().unwrap();
    result.result_type = ci_node.result_type.clone();
    result.digest = artifact_digest(format!("check result {number}").as_bytes());
    store.seal_attempt(&attempt).unwrap();
    attempt
}

fn state_of(setup: &Setup) -> RunState {
    RunState::replay(
        setup.run.run_id.clone(),
        &setup.spec,
        &setup.store.events(&setup.run.run_id).unwrap(),
    )
    .unwrap()
}

fn ci_status(setup: &Setup) -> NodeStatus {
    state_of(setup).node(&node("cs_ci")).unwrap().status
}

fn response_for(setup: &Setup, verdict: Verdict) -> VerifierResponse {
    let ci = &setup.spec.nodes[1];
    let rejected = verdict == Verdict::Reject;
    VerifierResponse {
        protocol: Exactly,
        verifier: ci.acceptance.verifier.clone(),
        attempt: setup.attempt.attempt_ref(),
        verdict,
        checks: vec![],
        reason_code: rejected.then(|| ident("check_failed")),
        reason_text: rejected.then(|| "a required check failed".to_owned()),
    }
}

fn good_receipt(setup: &Setup, id: &str, verdict: Verdict) -> VerifierReceipt {
    let mut response = response_for(setup, verdict);
    response.attempt = setup.attempt.attempt_ref();
    issue(
        &response,
        &setup.spec.nodes[1],
        &setup.attempt,
        ident(id),
        setup.subject.clone(),
        "checkspan-local-controller".into(),
        ts(NOW),
        Validity {
            valid_until: Some(ts("2026-12-31T00:00:00Z")),
            conditions: vec![],
        },
    )
    .unwrap()
}

fn counts(setup: &Setup) -> Vec<(String, u64)> {
    setup.store.row_counts().unwrap()
}

#[test]
fn a_matching_receipt_is_admitted_once_and_only_once() {
    let mut setup = seeded("admit-once");
    assert_eq!(ci_status(&setup), NodeStatus::Running);
    let receipt = good_receipt(&setup, "rcpt-ci-1", Verdict::Accept);
    admit(
        &mut setup.store,
        &setup.spec,
        &setup.run,
        &receipt,
        &ts(NOW),
    )
    .unwrap();
    assert_eq!(ci_status(&setup), NodeStatus::Accepted);
    let stored = setup
        .store
        .receipt(&setup.run.run_id, "rcpt-ci-1")
        .unwrap()
        .unwrap();
    assert_eq!(stored, receipt);
    let after_first = counts(&setup);

    // The same message again.
    let error = admit(
        &mut setup.store,
        &setup.spec,
        &setup.run,
        &receipt,
        &ts(NOW),
    )
    .unwrap_err();
    assert!(matches!(error, AdmitError::Duplicate), "{error}");
    // A re-issued receipt under a fresh id for the same attempt.
    let reissued = good_receipt(&setup, "rcpt-ci-2", Verdict::Accept);
    let error = admit(
        &mut setup.store,
        &setup.spec,
        &setup.run,
        &reissued,
        &ts(NOW),
    )
    .unwrap_err();
    assert!(matches!(error, AdmitError::Stale { .. }), "{error}");
    assert_eq!(ci_status(&setup), NodeStatus::Accepted);
    assert_eq!(counts(&setup), after_first, "nothing was written");
}

#[test]
fn a_rejecting_receipt_is_recorded_and_never_accepts() {
    let mut setup = seeded("reject");
    let receipt = good_receipt(&setup, "rcpt-ci-1", Verdict::Reject);
    admit(
        &mut setup.store,
        &setup.spec,
        &setup.run,
        &receipt,
        &ts(NOW),
    )
    .unwrap();
    assert_eq!(ci_status(&setup), NodeStatus::Rejected);
}

#[test]
fn a_wrong_verifier_wrong_policy_or_expired_receipt_is_refused() {
    let mut setup = seeded("wrong-verifier");
    let before = counts(&setup);

    // The response of a verifier the contract did not pin cannot even be
    // issued into a receipt.
    let mut response = response_for(&setup, Verdict::Accept);
    response.verifier.digest = artifact_digest(b"another profile");
    let error = issue(
        &response,
        &setup.spec.nodes[1],
        &setup.attempt,
        ident("rcpt-x"),
        setup.subject.clone(),
        "issuer".into(),
        ts(NOW),
        Validity {
            valid_until: None,
            conditions: vec![],
        },
    )
    .unwrap_err();
    assert!(matches!(error, IssueError::WrongVerifier), "{error}");

    let mut receipt = good_receipt(&setup, "rcpt-ci-1", Verdict::Accept);
    receipt.verifier.digest = artifact_digest(b"another profile");
    let error = admit(
        &mut setup.store,
        &setup.spec,
        &setup.run,
        &receipt,
        &ts(NOW),
    )
    .unwrap_err();
    assert!(matches!(error, AdmitError::WrongVerifier { .. }), "{error}");

    let mut receipt = good_receipt(&setup, "rcpt-ci-1", Verdict::Accept);
    receipt.policy_ref.version = Version::new(2).unwrap();
    let error = admit(
        &mut setup.store,
        &setup.spec,
        &setup.run,
        &receipt,
        &ts(NOW),
    )
    .unwrap_err();
    assert!(matches!(error, AdmitError::WrongPolicy), "{error}");

    let mut receipt = good_receipt(&setup, "rcpt-ci-1", Verdict::Accept);
    receipt.validity.valid_until = Some(ts("2026-09-01T00:00:00Z"));
    let error = admit(
        &mut setup.store,
        &setup.spec,
        &setup.run,
        &receipt,
        &ts(NOW),
    )
    .unwrap_err();
    assert!(matches!(error, AdmitError::Expired), "{error}");

    assert_eq!(ci_status(&setup), NodeStatus::Running);
    assert_eq!(counts(&setup), before);
}

#[test]
fn forged_bindings_cannot_change_acceptance() {
    let mut setup = seeded("forged");
    let before = counts(&setup);

    // A verdict for a different result.
    let mut receipt = good_receipt(&setup, "rcpt-ci-1", Verdict::Accept);
    receipt.result_digest = artifact_digest(b"a result that was never sealed");
    let error = admit(
        &mut setup.store,
        &setup.spec,
        &setup.run,
        &receipt,
        &ts(NOW),
    )
    .unwrap_err();
    assert!(matches!(error, AdmitError::WrongResult), "{error}");

    // A verdict about a different subject.
    let mut receipt = good_receipt(&setup, "rcpt-ci-1", Verdict::Accept);
    receipt.subject = format!("patch_subject:{}", artifact_digest(b"another candidate"));
    let error = admit(
        &mut setup.store,
        &setup.spec,
        &setup.run,
        &receipt,
        &ts(NOW),
    )
    .unwrap_err();
    assert!(matches!(error, AdmitError::WrongSubject { .. }), "{error}");

    // A context digest over substituted evidence.
    let mut receipt = good_receipt(&setup, "rcpt-ci-1", Verdict::Accept);
    receipt.context_digest = artifact_digest(b"a context that was never frozen");
    let error = admit(
        &mut setup.store,
        &setup.spec,
        &setup.run,
        &receipt,
        &ts(NOW),
    )
    .unwrap_err();
    assert!(matches!(error, AdmitError::ContextMismatch), "{error}");

    // A receipt for an attempt that was never sealed.
    let mut receipt = good_receipt(&setup, "rcpt-ci-1", Verdict::Accept);
    receipt.attempt.number = checkspan::contracts::AttemptNumber::new(7).unwrap();
    receipt.context_digest = artifact_digest(b"whatever");
    let error = admit(
        &mut setup.store,
        &setup.spec,
        &setup.run,
        &receipt,
        &ts(NOW),
    )
    .unwrap_err();
    assert!(matches!(error, AdmitError::UnknownAttempt), "{error}");

    assert_eq!(ci_status(&setup), NodeStatus::Running);
    assert_eq!(counts(&setup), before);
}

#[test]
fn a_stale_attempts_receipt_cannot_accept_after_a_retry() {
    let mut setup = seeded("stale");
    let first = good_receipt(&setup, "rcpt-ci-1", Verdict::Reject);
    admit(&mut setup.store, &setup.spec, &setup.run, &first, &ts(NOW)).unwrap();
    assert_eq!(ci_status(&setup), NodeStatus::Rejected);

    // Policy allows a retry; attempt 2 runs and is sealed.
    setup
        .store
        .transaction(|tx| {
            tx.append_event(
                &setup.run.run_id,
                Some(&node("cs_ci")),
                kinds::RETRY_ALLOWED,
                &json!({}),
            )?;
            Ok(())
        })
        .unwrap();
    let old_attempt = setup.attempt.clone();
    setup.attempt = seal_ci_attempt(&mut setup.store, &setup.spec, &setup.run, 2, &setup.subject);
    assert_eq!(ci_status(&setup), NodeStatus::Running);

    // A late acceptance for attempt 1 arrives after attempt 2 exists.
    let attempt2 = std::mem::replace(&mut setup.attempt, old_attempt);
    let stale = good_receipt(&setup, "rcpt-ci-late", Verdict::Accept);
    setup.attempt = attempt2;
    let error = admit(&mut setup.store, &setup.spec, &setup.run, &stale, &ts(NOW)).unwrap_err();
    assert!(matches!(error, AdmitError::Stale { .. }), "{error}");
    assert_eq!(ci_status(&setup), NodeStatus::Running);

    // The current attempt's receipt is admissible.
    let fresh = good_receipt(&setup, "rcpt-ci-2", Verdict::Accept);
    admit(&mut setup.store, &setup.spec, &setup.run, &fresh, &ts(NOW)).unwrap();
    assert_eq!(ci_status(&setup), NodeStatus::Accepted);
}

#[test]
fn a_failed_attempt_cannot_carry_a_verdict() {
    let mut setup = seeded("failed");
    let mut attempt: Attempt =
        parse_record(&fixture("outcomes/valid/attempt-failed.json")).unwrap();
    attempt.node = setup.spec.node_ref(&node("cs_packet")).unwrap();
    attempt.dependency_receipts.clear();
    setup
        .store
        .transaction(|tx| {
            tx.append_event(
                &setup.run.run_id,
                Some(&node("cs_packet")),
                kinds::ATTEMPT_DISPATCHED,
                &json!({"number": attempt.number}),
            )?;
            Ok(())
        })
        .unwrap();
    setup.store.seal_attempt(&attempt).unwrap();

    let error = issue(
        &checkspan::verifier_host::VerifierResponse {
            protocol: Exactly,
            verifier: setup.spec.nodes[2].acceptance.verifier.clone(),
            attempt: attempt.attempt_ref(),
            verdict: Verdict::Accept,
            checks: vec![],
            reason_code: None,
            reason_text: None,
        },
        &setup.spec.nodes[2],
        &attempt,
        ident("rcpt-packet-1"),
        "subject".into(),
        "issuer".into(),
        ts(NOW),
        Validity {
            valid_until: None,
            conditions: vec![],
        },
    )
    .unwrap_err();
    assert!(matches!(error, IssueError::NotCompleted), "{error}");

    // Even a hand-built receipt for it is refused at admission.
    let mut receipt = good_receipt(&setup, "rcpt-packet-1", Verdict::Accept);
    receipt.attempt = attempt.attempt_ref();
    receipt.verifier = setup.spec.nodes[2].acceptance.verifier.clone();
    receipt.policy_ref = setup.spec.nodes[2].acceptance.policy_ref.clone();
    receipt.subject = "subject".into();
    let error = admit(
        &mut setup.store,
        &setup.spec,
        &setup.run,
        &receipt,
        &ts(NOW),
    )
    .unwrap_err();
    assert!(matches!(error, AdmitError::NotCompleted), "{error}");
}

#[test]
fn an_inadmissible_dependency_blocks_acceptance_but_not_rejection() {
    let mut setup = seeded("expired-dep");
    // rcpt-patch-1 expires 2026-10-01; at LATER it no longer admits new work.
    let accept = good_receipt(&setup, "rcpt-ci-1", Verdict::Accept);
    let error = admit(
        &mut setup.store,
        &setup.spec,
        &setup.run,
        &accept,
        &ts(LATER),
    )
    .unwrap_err();
    assert!(matches!(error, AdmitError::DependencyBlocked(_)), "{error}");
    assert_eq!(ci_status(&setup), NodeStatus::Running);
    // cs_patch's own historical acceptance is untouched.
    assert_eq!(
        state_of(&setup).node(&node("cs_patch")).unwrap().status,
        NodeStatus::Accepted
    );

    // Recording a rejection needs no admissible dependencies.
    let reject = good_receipt(&setup, "rcpt-ci-2", Verdict::Reject);
    admit(
        &mut setup.store,
        &setup.spec,
        &setup.run,
        &reject,
        &ts(LATER),
    )
    .unwrap();
    assert_eq!(ci_status(&setup), NodeStatus::Rejected);
}

#[test]
fn a_real_verifier_response_becomes_an_admitted_receipt() {
    let scratch = temp_dir("real");
    // A real candidate repository.
    let repo = scratch.join("repo");
    fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args([
                "-c",
                "user.name=Checkspan Fixture",
                "-c",
                "user.email=fixture@checkspan.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}");
    };
    git(&["init", "-q", "-b", "main"]);
    fs::write(repo.join("README.md"), b"# candidate\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base"]);

    // A pinned profile with one profile-owned check.
    let profile = ValidationProfile {
        profile: Exactly,
        id: ident(software::VERIFIER_ID),
        version: Version::new(software::VERIFIER_VERSION).unwrap(),
        checks: vec![CheckSpec {
            id: ident("fixture"),
            program: env!("CARGO_BIN_EXE_checkspan").to_owned(),
            args: vec![
                "validate".into(),
                common::fixture_path("contracts/valid/graph-run-minimal.json")
                    .display()
                    .to_string(),
            ],
            env: vec![],
            timeout_ms: 20_000,
        }],
    };
    let profile_path = scratch.join("profile.json");
    let profile_bytes = serde_json::to_vec_pretty(&profile).unwrap();
    fs::write(&profile_path, &profile_bytes).unwrap();
    let pinned = checkspan::contracts::VerifierRef {
        id: profile.id.clone(),
        version: profile.version,
        digest: artifact_digest(&profile_bytes),
    };

    // The graph pins that profile as cs_ci's verifier.
    let mut spec: GraphSpec = parse_record(&fixture("evidence/graph-with-ports.json")).unwrap();
    {
        let ci = spec
            .nodes
            .iter_mut()
            .find(|n| n.id.as_str() == "cs_ci")
            .unwrap();
        ci.acceptance.verifier = pinned.clone();
        ci.acceptance.required_checks = vec![ident("fixture")];
    }

    // Freeze the candidate.
    let captured = code::capture(
        &repo,
        &Selection {
            candidate: Selector::WorkingTree,
            base: None,
        },
        &code::Limits::default(),
        &ts(NOW),
    )
    .unwrap();
    let subject = format!("patch_subject:{}", captured.subject_digest);
    let evidence = EvidenceRef {
        port_name: ident("checkout"),
        locator: format!(
            "git:{}#base={}&candidate={}:{}",
            captured.root.display(),
            captured.result.base_commit,
            captured.result.candidate.kind.as_str(),
            captured.result.candidate.commit
        ),
        subject: subject.clone(),
        version: captured.result.candidate.commit.to_string(),
        content_digest: artifact_digest(&serde_json::to_vec(&captured.result).unwrap()),
        provenance: Provenance {
            producer: "checkspan/code-adapter".into(),
            attestation_ref: None,
        },
        policy_ref: PolicyRef {
            id: ident("local_evidence"),
            version: Version::new(1).unwrap(),
        },
        valid_until: None,
    };

    // Seed the ledger: cs_patch accepted, cs_ci attempt dispatched.
    let mut store = Store::open(scratch.join("ledger.sqlite")).unwrap();
    store.store_graph(&spec).unwrap();
    let run = run_record("run-0001", &spec);
    store.create_run(&run).unwrap();
    let patch_digest = artifact_digest(b"the exact patch bytes");
    store
        .transaction(|tx| {
            tx.append_event(
                &run.run_id,
                Some(&node("cs_patch")),
                kinds::ATTEMPT_DISPATCHED,
                &json!({"number": 1}),
            )?;
            Ok(())
        })
        .unwrap();
    let mut patch_attempt: Attempt = parse_record(&fixture(
        "outcomes/valid/attempt-completed-with-receipt.json",
    ))
    .unwrap();
    patch_attempt.node = spec.node_ref(&node("cs_patch")).unwrap();
    patch_attempt.verifier_receipt = None;
    patch_attempt.dependency_receipts.clear();
    let result = patch_attempt.result.as_mut().unwrap();
    result.result_type = spec.nodes[0].result_type.clone();
    result.digest = patch_digest.clone();
    store.seal_attempt(&patch_attempt).unwrap();
    let mut patch_receipt: VerifierReceipt =
        parse_record(&fixture("outcomes/valid/receipt-accept.json")).unwrap();
    patch_receipt.id = ident("rcpt-patch-1");
    patch_receipt.attempt = patch_attempt.attempt_ref();
    patch_receipt.result_digest = patch_digest;
    patch_receipt.verifier = spec.nodes[0].acceptance.verifier.clone();
    patch_receipt.policy_ref = spec.nodes[0].acceptance.policy_ref.clone();
    patch_receipt.validity.valid_until = None;
    store.admit_receipt(&patch_receipt).unwrap();
    store
        .transaction(|tx| {
            tx.append_event(
                &run.run_id,
                Some(&node("cs_ci")),
                kinds::ATTEMPT_DISPATCHED,
                &json!({"number": 1}),
            )?;
            Ok(())
        })
        .unwrap();

    // The real verifier runs as a real child.
    let ci = spec
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "cs_ci")
        .unwrap();
    let request = VerifierRequest {
        protocol: Exactly,
        verifier: pinned.clone(),
        attempt: checkspan::contracts::AttemptRef {
            run_id: run.run_id.clone(),
            node: spec.node_ref(&node("cs_ci")).unwrap(),
            number: checkspan::contracts::AttemptNumber::new(1).unwrap(),
        },
        subject: subject.clone(),
        claim: ci.acceptance.claim.clone(),
        required_checks: ci.acceptance.required_checks.clone(),
        policy_ref: ci.acceptance.policy_ref.clone(),
        input_manifest_digest: checkspan::digests::input_manifest_digest(
            std::slice::from_ref(&evidence),
            &[],
        )
        .unwrap(),
        evidence: vec![evidence.clone()],
    };
    let out = scratch.join("result.json");
    let host = VerifierProfile {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_checkspan")),
        executable_digest: None,
        args: child_args(&profile_path, &out),
        cwd: scratch.clone(),
        env: vec![("PATH".into(), std::env::var("PATH").unwrap())],
        timeout: Duration::from_secs(120),
        max_stdout_bytes: 1 << 20,
        max_stderr_bytes: 1 << 20,
    };
    let completion = run_verifier(&host, &request, &Arc::new(AtomicBool::new(false)));
    let response = completion
        .outcome
        .as_ref()
        .unwrap_or_else(|e| panic!("{e}; stderr: {}", completion.stderr));
    assert_eq!(response.verdict, Verdict::Accept);

    // Seal the attempt with the real result artifact.
    let result_bytes = fs::read(&out).unwrap();
    let mut attempt: Attempt = parse_record(&fixture(
        "outcomes/valid/attempt-completed-with-receipt.json",
    ))
    .unwrap();
    attempt.node = spec.node_ref(&node("cs_ci")).unwrap();
    attempt.verifier_receipt = None;
    attempt.dependency_receipts = vec![ReceiptRef {
        id: ident("rcpt-patch-1"),
        run_id: run.run_id.clone(),
        node: spec.node_ref(&node("cs_patch")).unwrap(),
    }];
    attempt.input_manifest = vec![evidence];
    let sealed = attempt.result.as_mut().unwrap();
    sealed.result_type = ci.result_type.clone();
    sealed.digest = artifact_digest(&result_bytes);
    sealed.artifact_ref = format!("file:{}", out.display());
    store.seal_attempt(&attempt).unwrap();

    // Issue and admit.
    let receipt = issue(
        response,
        ci,
        &attempt,
        ident("rcpt-ci-1"),
        subject,
        "checkspan-local-controller".into(),
        ts(NOW),
        Validity {
            valid_until: None,
            conditions: vec![],
        },
    )
    .unwrap();
    admit(&mut store, &spec, &run, &receipt, &ts(NOW)).unwrap();
    let state = RunState::replay(
        run.run_id.clone(),
        &spec,
        &store.events(&run.run_id).unwrap(),
    )
    .unwrap();
    let view = state.node(&node("cs_ci")).unwrap();
    assert_eq!(view.status, NodeStatus::Accepted);
    let stored = store.receipt(&run.run_id, "rcpt-ci-1").unwrap().unwrap();
    assert_eq!(stored.result_digest, artifact_digest(&result_bytes));
    assert_eq!(stored.verifier, pinned);
}
