//! Bounded human gate packets: exhausted and undecidable work stops with a
//! complete decision packet, and stays gated until a real decision exists.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use checkspan::contracts::{
    Attempt, ContextKind, EvidenceRef, Exactly, GatePacket, GatePurpose, GraphRun, GraphSpec,
    Ident, NodeId, NodeStatus, PolicyRef, Provenance, ReceiptRef, Record, RecordKind, RunId,
    SchemaVersion, Timestamp, Validity, Verdict, VerifierReceipt, Version, parse_record,
};
use checkspan::digests::artifact_digest;
use checkspan::gates::{GateBuildError, open_resolve_work};
use checkspan::receipts::{admit, issue};
use checkspan::state::{RunState, kinds};
use checkspan::store::Store;
use checkspan::verifier_host::VerifierResponse;
use common::{fixture, schema_errors};
use serde_json::{Value, json};

const NOW: &str = "2026-09-06T15:00:00Z";
const EXPIRES: &str = "2026-09-13T00:00:00Z";

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
    let dir = std::env::temp_dir().join(format!("checkspan-gates-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// The review graph with cs_patch accepted and cs_ci's attempt sealed as
/// completed; the same seeding the receipt tests use.
fn seeded(name: &str) -> (Store, GraphSpec, GraphRun, Attempt, String) {
    let dir = temp_dir(name);
    let mut store = Store::open(dir.join("ledger.sqlite")).unwrap();
    let spec: GraphSpec = parse_record(&fixture("contracts/valid/graph-spec-full.json")).unwrap();
    store.store_graph(&spec).unwrap();
    let run = GraphRun {
        record: RecordKind::GraphRun,
        schema_version: SchemaVersion(1),
        run_id: RunId::new("run-0001").unwrap(),
        graph_ref: spec.graph_ref(),
        budget_lineage_ref: None,
        admitted_imports: vec![],
    };
    store.create_run(&run).unwrap();

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
    patch_receipt.validity.valid_until = None;
    store.admit_receipt(&patch_receipt).unwrap();

    let ci = node("cs_ci");
    let subject = format!("patch_subject:{}", artifact_digest(b"the candidate"));
    store
        .transaction(|tx| {
            tx.append_event(
                &run.run_id,
                Some(&ci),
                kinds::ATTEMPT_DISPATCHED,
                &json!({"number": 1}),
            )?;
            Ok(())
        })
        .unwrap();
    let mut attempt: Attempt = parse_record(&fixture(
        "outcomes/valid/attempt-completed-with-receipt.json",
    ))
    .unwrap();
    attempt.node = spec.node_ref(&ci).unwrap();
    attempt.verifier_receipt = None;
    attempt.dependency_receipts = vec![ReceiptRef {
        id: ident("rcpt-patch-1"),
        run_id: run.run_id.clone(),
        node: spec.node_ref(&patch).unwrap(),
    }];
    attempt.input_manifest = vec![EvidenceRef {
        port_name: ident("candidate"),
        locator: "git:C:/pilot#base=aaaa&candidate=commit:bbbb".into(),
        subject: subject.clone(),
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
    let ci_node = &spec.nodes[1];
    let result = attempt.result.as_mut().unwrap();
    result.result_type = ci_node.result_type.clone();
    result.digest = artifact_digest(b"check result 1");
    store.seal_attempt(&attempt).unwrap();
    (store, spec, run, attempt, subject)
}

fn receipt_with(
    spec: &GraphSpec,
    attempt: &Attempt,
    subject: &str,
    verdict: Verdict,
) -> VerifierReceipt {
    let ci = &spec.nodes[1];
    let decided = verdict != Verdict::Accept;
    let response = VerifierResponse {
        protocol: Exactly,
        verifier: ci.acceptance.verifier.clone(),
        attempt: attempt.attempt_ref(),
        verdict,
        checks: vec![],
        reason_code: decided.then(|| ident("cannot_determine")),
        reason_text: decided.then(|| "the checker could not decide".to_owned()),
    };
    issue(
        &response,
        ci,
        attempt,
        ident("rcpt-ci-1"),
        subject.to_owned(),
        "checkspan-local-controller".into(),
        ts(NOW),
        Validity {
            valid_until: None,
            conditions: vec![],
        },
    )
    .unwrap()
}

fn ci_view(store: &Store, spec: &GraphSpec, run: &GraphRun) -> (NodeStatus, Option<String>) {
    let state = RunState::replay(
        run.run_id.clone(),
        spec,
        &store.events(&run.run_id).unwrap(),
    )
    .unwrap();
    let s = state.node(&node("cs_ci")).unwrap();
    (
        s.status,
        s.waiting_on_gate.as_ref().map(ToString::to_string),
    )
}

fn assert_packet_complete(packet: &GatePacket, spec: &GraphSpec) {
    let text = serde_json::to_string_pretty(packet).unwrap();
    assert_eq!(
        schema_errors(GatePacket::SCHEMA_ID, &text),
        Vec::<String>::new()
    );
    assert_eq!(packet.check_bindings(), vec![]);
    assert_eq!(packet.purpose, GatePurpose::ResolveWork);
    assert_eq!(packet.node, spec.node_ref(&node("cs_ci")).unwrap());
    let kinds: Vec<ContextKind> = packet.context_refs.iter().map(|c| c.kind).collect();
    assert!(kinds.contains(&ContextKind::Attempt), "{kinds:?}");
    assert!(kinds.contains(&ContextKind::Receipt), "{kinds:?}");
    assert!(kinds.contains(&ContextKind::Artifact), "{kinds:?}");
    let option_names: Vec<&str> = packet
        .requested_decision
        .options
        .iter()
        .map(|o| o.as_str())
        .collect();
    assert_eq!(option_names, ["retry", "revise", "cancel"]);
    assert!(packet.requested_decision.action.is_none());
    assert_eq!(packet.authority_policy_ref.id.as_str(), "operator_decides");
    assert_eq!(packet.expires_at, ts(EXPIRES));
    assert!(packet.created_at.is_before(&packet.expires_at));
}

#[test]
fn an_undecidable_verdict_gates_the_node_with_a_complete_packet() {
    let (mut store, spec, run, attempt, subject) = seeded("undecidable");
    let receipt = receipt_with(&spec, &attempt, &subject, Verdict::Undecidable);
    admit(&mut store, &spec, &run, &receipt, &ts(NOW)).unwrap();
    assert_eq!(ci_view(&store, &spec, &run), (NodeStatus::Gated, None));

    let packet = open_resolve_work(
        &mut store,
        &spec,
        &run,
        &node("cs_ci"),
        &ts(NOW),
        &ts(EXPIRES),
    )
    .unwrap();
    assert_packet_complete(&packet, &spec);
    let receipt_context = packet
        .context_refs
        .iter()
        .find(|c| c.kind == ContextKind::Receipt)
        .unwrap();
    assert_eq!(receipt_context.id, "rcpt-ci-1");

    // The node waits on exactly this packet; nothing in the packet waits on
    // the node's own acceptance, and a second packet cannot stack.
    assert_eq!(
        ci_view(&store, &spec, &run),
        (NodeStatus::Gated, Some(packet.id.to_string()))
    );
    let error = open_resolve_work(
        &mut store,
        &spec,
        &run,
        &node("cs_ci"),
        &ts(NOW),
        &ts(EXPIRES),
    )
    .unwrap_err();
    assert!(
        matches!(error, GateBuildError::NotGateable { .. }),
        "{error}"
    );
    let stored = store.gate_packets(&run.run_id).unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0], packet);
}

#[test]
fn a_rejected_node_gates_with_the_rejection_as_evidence() {
    let (mut store, spec, run, attempt, subject) = seeded("rejected");
    let receipt = receipt_with(&spec, &attempt, &subject, Verdict::Reject);
    admit(&mut store, &spec, &run, &receipt, &ts(NOW)).unwrap();
    assert_eq!(ci_view(&store, &spec, &run).0, NodeStatus::Rejected);

    let packet = open_resolve_work(
        &mut store,
        &spec,
        &run,
        &node("cs_ci"),
        &ts(NOW),
        &ts(EXPIRES),
    )
    .unwrap();
    assert_packet_complete(&packet, &spec);
    assert_eq!(
        ci_view(&store, &spec, &run),
        (NodeStatus::Gated, Some(packet.id.to_string()))
    );
}

#[test]
fn an_accepted_or_running_node_cannot_be_gated_and_expiry_must_follow_creation() {
    let (mut store, spec, run, attempt, subject) = seeded("guards");

    // Still checking: no packet.
    let error = open_resolve_work(
        &mut store,
        &spec,
        &run,
        &node("cs_ci"),
        &ts(NOW),
        &ts(EXPIRES),
    )
    .unwrap_err();
    assert!(
        matches!(error, GateBuildError::NotGateable { .. }),
        "{error}"
    );

    // Accepted: no packet.
    let receipt = receipt_with(&spec, &attempt, &subject, Verdict::Accept);
    admit(&mut store, &spec, &run, &receipt, &ts(NOW)).unwrap();
    let error = open_resolve_work(
        &mut store,
        &spec,
        &run,
        &node("cs_ci"),
        &ts(NOW),
        &ts(EXPIRES),
    )
    .unwrap_err();
    assert!(
        matches!(error, GateBuildError::NotGateable { .. }),
        "{error}"
    );

    // A packet cannot expire before it exists.
    let error =
        open_resolve_work(&mut store, &spec, &run, &node("cs_ci"), &ts(NOW), &ts(NOW)).unwrap_err();
    assert!(
        matches!(error, GateBuildError::ExpiryNotAfterCreation),
        "{error}"
    );
    // cs_patch's accepted state never gained a packet.
    let stored = store.gate_packets(&run.run_id).unwrap();
    assert!(stored.is_empty());
}

// ------------------------------------------------------- CLI, real run --

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
        ])
        .args(args)
        .output()
        .expect("git is installed");
    assert!(output.status.success(), "git {args:?}");
}

fn checkspan(args: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_checkspan"))
        .args(args)
        .output()
        .expect("checkspan runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "checkspan {args:?} did not print JSON: {e}\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.code().unwrap_or(-1), value)
}

/// A real failing pilot exhausts its attempts and lands at an open gate;
/// without a decision the run stays gated across restarts.
#[test]
fn an_exhausted_run_opens_a_gate_and_stays_gated_without_a_decision() {
    let dir = temp_dir("cli");
    let repo = dir.join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    fs::write(repo.join("contract.json"), b"{\"record\": \"graph_run\"}\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "base"]);

    // The same profile and graph construction the local-run suite uses.
    let profile = checkspan::verifiers::software::ValidationProfile {
        profile: Exactly,
        id: ident("software_check"),
        version: Version::new(1).unwrap(),
        checks: vec![checkspan::verifiers::software::CheckSpec {
            id: ident("schema"),
            program: env!("CARGO_BIN_EXE_checkspan").to_owned(),
            args: vec!["validate".into(), "contract.json".into()],
            env: vec![],
            timeout_ms: 60_000,
        }],
    };
    let profile_bytes = serde_json::to_vec_pretty(&profile).unwrap();
    let profile_path = dir.join("profile.json");
    fs::write(&profile_path, &profile_bytes).unwrap();
    let graph = common::pilot_graph_for(artifact_digest(&profile_bytes));
    let graph_path = dir.join("graph.json");
    fs::write(&graph_path, serde_json::to_vec_pretty(&graph).unwrap()).unwrap();
    let store = dir.join("ledger.sqlite").display().to_string();
    let graph_arg = graph_path.display().to_string();
    let artifacts = dir.join("artifacts").display().to_string();
    let repo_arg = repo.display().to_string();

    let (code, _) = checkspan(&[
        "run", "create", "--store", &store, "--graph", &graph_arg, "--run-id", "run-0001",
    ]);
    assert_eq!(code, 0);
    let (code, driven) = checkspan(&[
        "run",
        "drive",
        "--store",
        &store,
        "--graph",
        &graph_arg,
        "--run-id",
        "run-0001",
        "--repo",
        &repo_arg,
        "--software-profile",
        &profile_path.display().to_string(),
        "--artifacts",
        &artifacts,
        "--controller",
        "ctl-1",
        "--gate-expiry",
        EXPIRES,
    ]);
    assert_eq!(code, 0, "{driven}");
    let exhausted = driven["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["disposition"]["decision"] == "exhausted")
        .unwrap_or_else(|| panic!("{driven}"));
    assert_eq!(exhausted["disposition"]["route"], "gate");
    let packet_id = exhausted["disposition"]["packet"].as_str().unwrap();

    let (code, status) = checkspan(&[
        "run", "status", "--store", &store, "--graph", &graph_arg, "--run-id", "run-0001",
    ]);
    assert_eq!(code, 0);
    let ci = status["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["node"]["node_id"] == "cs_ci")
        .unwrap();
    assert_eq!(ci["status"], "gated", "{status}");
    assert_eq!(ci["waiting_on_gate"], packet_id, "{status}");

    // The packet the operator reads is schema-valid and complete.
    let (code, printed) = checkspan(&[
        "run", "packet", "--store", &store, "--graph", &graph_arg, "--run-id", "run-0001",
        "--node", "cs_ci",
    ]);
    assert_eq!(code, 0, "{printed}");
    let packet_text = serde_json::to_string(&printed["packet"]).unwrap();
    assert_eq!(
        schema_errors(GatePacket::SCHEMA_ID, &packet_text),
        Vec::<String>::new()
    );
    let packet: GatePacket = parse_record(&packet_text).unwrap();
    assert_eq!(packet.id.as_str(), packet_id);
    assert_eq!(packet.check_bindings(), vec![]);
    assert!(
        packet
            .context_refs
            .iter()
            .any(|c| c.kind == ContextKind::Receipt),
        "{packet_text}"
    );

    // Without an operator decision a fresh controller does nothing: the run
    // stays gated, no new attempt is dispatched, nothing changes.
    let (code, again) = checkspan(&[
        "run",
        "drive",
        "--store",
        &store,
        "--graph",
        &graph_arg,
        "--run-id",
        "run-0001",
        "--repo",
        &repo_arg,
        "--software-profile",
        &profile_path.display().to_string(),
        "--artifacts",
        &artifacts,
        "--controller",
        "ctl-2",
        "--gate-expiry",
        EXPIRES,
    ]);
    assert_eq!(code, 0);
    let taken: Vec<&str> = again["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["action"].as_str().unwrap())
        .collect();
    assert_eq!(taken, ["idle"], "{again}");
    let (_, status_again) = checkspan(&[
        "run", "status", "--store", &store, "--graph", &graph_arg, "--run-id", "run-0001",
    ]);
    assert_eq!(status, status_again);
}
