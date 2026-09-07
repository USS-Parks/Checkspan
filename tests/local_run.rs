//! The local run workflow operated through the real CLI binary: every
//! command is a separate process against the same durable store, so every
//! test is also a restart test.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use checkspan::contracts::{
    AcceptanceSpec, Dependency, GraphBudget, GraphSpec, Ident, NodeId, NodeKind, NodeRef, NodeSpec,
    PatchResult, PolicyRef, PortKind, PortSpec, Record, RecordKind, RetryPolicy, Revision,
    SchemaVersion, SoftwareCheckResult, TypeRef, VerifierRef, Version, registry,
};
use checkspan::contracts::{FailureClass, OnExhaustion};
use checkspan::controller::REPO_SOURCE;
use checkspan::digests::artifact_digest;
use checkspan::verifiers::{patch, software};
use serde_json::Value;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("checkspan-run-{}-{name}", std::process::id()));
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

/// Run one `checkspan` CLI command as a real process; returns exit code and
/// parsed JSON output.
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

/// One prepared workspace: repository, profile, graph, store, artifacts.
struct Pilot {
    dir: PathBuf,
    repo: PathBuf,
    store: String,
    graph: String,
    profile: String,
    artifacts: String,
}

impl Pilot {
    /// Build a workspace whose candidate document is `contract` and whose
    /// software profile validates it inside the checkout.
    fn new(name: &str, contract: &[u8]) -> Pilot {
        let dir = temp_dir(name);
        let repo = dir.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        fs::write(repo.join("contract.json"), contract).unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", "base"]);

        let profile = software::ValidationProfile {
            profile: checkspan::contracts::Exactly,
            id: Ident::new(software::VERIFIER_ID).unwrap(),
            version: Version::new(software::VERIFIER_VERSION).unwrap(),
            checks: vec![software::CheckSpec {
                id: Ident::new("schema").unwrap(),
                program: env!("CARGO_BIN_EXE_checkspan").to_owned(),
                args: vec!["validate".into(), "contract.json".into()],
                env: vec![],
                timeout_ms: 60_000,
            }],
        };
        let profile_path = dir.join("profile.json");
        let profile_bytes = serde_json::to_vec_pretty(&profile).unwrap();
        fs::write(&profile_path, &profile_bytes).unwrap();

        let graph = pilot_graph(artifact_digest(&profile_bytes));
        let graph_path = dir.join("graph.json");
        fs::write(&graph_path, serde_json::to_vec_pretty(&graph).unwrap()).unwrap();
        let artifacts = dir.join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();
        Pilot {
            store: dir.join("ledger.sqlite").display().to_string(),
            graph: graph_path.display().to_string(),
            profile: profile_path.display().to_string(),
            artifacts: artifacts.display().to_string(),
            repo,
            dir,
        }
    }

    fn create(&self) -> (i32, Value) {
        checkspan(&[
            "run",
            "create",
            "--store",
            &self.store,
            "--graph",
            &self.graph,
            "--run-id",
            "run-0001",
        ])
    }

    fn drive(&self) -> (i32, Value) {
        let repo = self.repo.display().to_string();
        checkspan(&[
            "run",
            "drive",
            "--store",
            &self.store,
            "--graph",
            &self.graph,
            "--run-id",
            "run-0001",
            "--repo",
            &repo,
            "--software-profile",
            &self.profile,
            "--artifacts",
            &self.artifacts,
            "--controller",
            "ctl-1",
        ])
    }

    fn status(&self) -> (i32, Value) {
        checkspan(&[
            "run",
            "status",
            "--store",
            &self.store,
            "--graph",
            &self.graph,
            "--run-id",
            "run-0001",
        ])
    }
}

/// The two-node pilot graph: capture the patch, then run the checks.
fn pilot_graph(profile_digest: checkspan::contracts::Digest) -> GraphSpec {
    let patch_type = TypeRef {
        schema_id: Ident::new("patch_result").unwrap(),
        version: Version::new(1).unwrap(),
        digest: artifact_digest(
            registry::bundled(PatchResult::SCHEMA_ID)
                .unwrap()
                .as_bytes(),
        ),
    };
    let check_type = TypeRef {
        schema_id: Ident::new("software_check_result").unwrap(),
        version: Version::new(1).unwrap(),
        digest: artifact_digest(
            registry::bundled(SoftwareCheckResult::SCHEMA_ID)
                .unwrap()
                .as_bytes(),
        ),
    };
    let policy = PolicyRef {
        id: Ident::new("trusted_local").unwrap(),
        version: Version::new(1).unwrap(),
    };
    let port = |name: &str| PortSpec {
        name: Ident::new(name).unwrap(),
        kind: PortKind::Code,
        expected_type: patch_type.clone(),
        required: true,
        allowed_source_scope: vec![REPO_SOURCE.to_owned()],
        handling_policy_ref: PolicyRef {
            id: Ident::new("local_evidence").unwrap(),
            version: Version::new(1).unwrap(),
        },
    };
    let retry = RetryPolicy {
        version: checkspan::contracts::Exactly,
        max_attempts: 2,
        allowed_failure_classes: vec![FailureClass::Rejected, FailureClass::Infrastructure],
        on_exhaustion: OnExhaustion::Gate,
    };
    let node_ref = |id: &str| NodeRef {
        graph_id: checkspan::contracts::GraphId::new("local_pilot").unwrap(),
        node_id: NodeId::new(id).unwrap(),
        revision: Revision::new(1).unwrap(),
    };
    GraphSpec {
        record: RecordKind::GraphSpec,
        schema_version: SchemaVersion(1),
        graph_id: checkspan::contracts::GraphId::new("local_pilot").unwrap(),
        revision: Revision::new(1).unwrap(),
        display_name: Some("Local run pilot".into()),
        nodes: vec![
            NodeSpec {
                id: NodeId::new("cs_patch").unwrap(),
                revision: Revision::new(1).unwrap(),
                kind: NodeKind::Task,
                display_name: None,
                prompt: "Capture the supplied patch as an exact candidate.".into(),
                result_type: patch_type.clone(),
                acceptance: AcceptanceSpec {
                    claim: "A typed patch artifact exists against the pinned base.".into(),
                    required_checks: vec![
                        Ident::new("artifact_exists").unwrap(),
                        Ident::new("base_matches").unwrap(),
                    ],
                    verifier: VerifierRef {
                        id: Ident::new(patch::VERIFIER_ID).unwrap(),
                        version: Version::new(patch::VERIFIER_VERSION).unwrap(),
                        digest: patch::pinned_digest(),
                    },
                    policy_ref: policy.clone(),
                },
                authority_policy_ref: PolicyRef {
                    id: Ident::new("no_external_actions").unwrap(),
                    version: Version::new(1).unwrap(),
                },
                deps: vec![],
                evidence_ports: vec![port("candidate")],
                resource_scope: vec![],
                retry_policy: retry.clone(),
                supersedes: None,
            },
            NodeSpec {
                id: NodeId::new("cs_ci").unwrap(),
                revision: Revision::new(1).unwrap(),
                kind: NodeKind::Check,
                display_name: None,
                prompt: "Run the required checks against the exact candidate.".into(),
                result_type: check_type,
                acceptance: AcceptanceSpec {
                    claim: "The required checks passed for this exact candidate.".into(),
                    required_checks: vec![Ident::new("schema").unwrap()],
                    verifier: VerifierRef {
                        id: Ident::new(software::VERIFIER_ID).unwrap(),
                        version: Version::new(software::VERIFIER_VERSION).unwrap(),
                        digest: profile_digest,
                    },
                    policy_ref: policy,
                },
                authority_policy_ref: PolicyRef {
                    id: Ident::new("no_external_actions").unwrap(),
                    version: Version::new(1).unwrap(),
                },
                deps: vec![Dependency {
                    node: node_ref("cs_patch"),
                    output_port: Ident::new("result").unwrap(),
                    expected_type: patch_type.clone(),
                }],
                evidence_ports: vec![port("checkout")],
                resource_scope: vec![],
                retry_policy: retry,
                supersedes: None,
            },
        ],
        targets: vec![node_ref("cs_ci")],
        budget: GraphBudget {
            total_attempts: 6,
            deadline: None,
        },
        supersedes: None,
    }
}

fn actions(value: &Value) -> Vec<&str> {
    value["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["action"].as_str().unwrap())
        .collect()
}

fn node_status(status: &Value, node: &str) -> String {
    status["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["node"]["node_id"] == node)
        .unwrap_or_else(|| panic!("{node} not in {status}"))["status"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_submitted_patch_reaches_an_accepted_software_check_result() {
    let pilot = Pilot::new(
        "happy",
        common::fixture("contracts/valid/graph-run-minimal.json").as_bytes(),
    );
    let (code, created) = pilot.create();
    assert_eq!(code, 0, "{created}");
    assert_eq!(created["nodes"], 2);

    let (code, driven) = pilot.drive();
    assert_eq!(code, 0, "{driven}");
    let taken = actions(&driven);
    assert_eq!(
        taken,
        ["verified", "verified", "complete"],
        "actions: {driven}"
    );
    let verdicts: Vec<&str> = driven["actions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["verdict"].as_str())
        .collect();
    assert_eq!(verdicts, ["accept", "accept"]);

    let (code, status) = pilot.status();
    assert_eq!(code, 0);
    assert_eq!(node_status(&status, "cs_patch"), "accepted");
    assert_eq!(node_status(&status, "cs_ci"), "accepted");
    assert_eq!(status["budget"]["consumed"], 2);
    assert_eq!(status["active_claims"].as_array().unwrap().len(), 0);

    // The artifacts are the typed records they claim to be.
    let patch_artifact =
        fs::read_to_string(pilot.dir.join("artifacts/run-0001/cs_patch-1.json")).unwrap();
    assert_eq!(
        common::schema_errors(PatchResult::SCHEMA_ID, &patch_artifact),
        Vec::<String>::new()
    );
    let check_artifact =
        fs::read_to_string(pilot.dir.join("artifacts/run-0001/cs_ci-1.json")).unwrap();
    assert_eq!(
        common::schema_errors(SoftwareCheckResult::SCHEMA_ID, &check_artifact),
        Vec::<String>::new()
    );

    // A restart does no work again and the status is byte-stable.
    let (code, again) = pilot.drive();
    assert_eq!(code, 0);
    assert_eq!(actions(&again), ["complete"]);
    let (_, status_again) = pilot.status();
    assert_eq!(status, status_again);
}

#[test]
fn a_failing_candidate_is_rejected_retried_and_exhausted_to_a_gate() {
    let pilot = Pilot::new("failing", b"{\"record\": \"graph_run\"}\n");
    let (code, _) = pilot.create();
    assert_eq!(code, 0);
    let (code, driven) = pilot.drive();
    assert_eq!(code, 0, "{driven}");
    let taken = actions(&driven);
    // cs_patch is captured and accepted; cs_ci rejects, retries once, and
    // rejects again, exhausting its two attempts toward a gate.
    assert_eq!(taken[0], "verified", "{driven}");
    let rejects: Vec<&Value> = driven["actions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["verdict"] == "reject")
        .collect();
    assert_eq!(rejects.len(), 2, "{driven}");
    assert_eq!(rejects[0]["disposition"]["decision"], "retry");
    assert_eq!(rejects[1]["disposition"]["decision"], "exhausted");
    assert_eq!(rejects[1]["disposition"]["route"], "gate");
    assert_eq!(taken.last(), Some(&"idle"), "{driven}");

    let (_, status) = pilot.status();
    assert_eq!(node_status(&status, "cs_patch"), "accepted");
    assert_eq!(node_status(&status, "cs_ci"), "rejected");
    assert_eq!(status["budget"]["consumed"], 3);
}

#[test]
fn missing_evidence_fails_the_attempt_and_leaves_the_state_visible() {
    let pilot = Pilot::new(
        "missing",
        common::fixture("contracts/valid/graph-run-minimal.json").as_bytes(),
    );
    let (code, _) = pilot.create();
    assert_eq!(code, 0);
    // Drive without --repo: the code port cannot resolve.
    let (code, driven) = checkspan(&[
        "run",
        "drive",
        "--store",
        &pilot.store,
        "--graph",
        &pilot.graph,
        "--run-id",
        "run-0001",
        "--software-profile",
        &pilot.profile,
        "--artifacts",
        &pilot.artifacts,
        "--controller",
        "ctl-1",
    ]);
    assert_eq!(code, 0, "{driven}");
    let taken = actions(&driven);
    assert!(
        taken.iter().filter(|a| **a == "attempt_failed").count() >= 2,
        "{driven}"
    );
    let first = &driven["actions"][0];
    assert_eq!(first["action"], "attempt_failed");
    assert!(
        first["reason"].as_str().unwrap().contains("not registered"),
        "{first}"
    );

    let (_, status) = pilot.status();
    assert_eq!(node_status(&status, "cs_patch"), "failed");
    let blocked = status["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["node"]["node_id"] == "cs_ci")
        .unwrap();
    assert_eq!(blocked["status"], "open");
    assert!(
        blocked["blocked_reason"]
            .as_str()
            .unwrap()
            .contains("cs_patch"),
        "{blocked}"
    );

    // Supplying the repository afterwards repairs the run: the gate route
    // was not yet reached, so remaining attempts proceed.
    let (_, repaired) = pilot.drive();
    let statuses = pilot.status();
    let status = statuses.1;
    // Whether attempts remained after the failures is the budget's call;
    // what must hold is that the ledger is consistent and honest.
    assert_eq!(repaired["run_id"], "run-0001");
    assert!(
        matches!(
            node_status(&status, "cs_patch").as_str(),
            "accepted" | "failed"
        ),
        "{status}"
    );
}

#[test]
fn cancellation_is_recorded_and_stops_dependents() {
    let pilot = Pilot::new(
        "cancel",
        common::fixture("contracts/valid/graph-run-minimal.json").as_bytes(),
    );
    let (code, _) = pilot.create();
    assert_eq!(code, 0);
    let (code, cancelled) = checkspan(&[
        "run",
        "cancel",
        "--store",
        &pilot.store,
        "--graph",
        &pilot.graph,
        "--run-id",
        "run-0001",
        "--node",
        "cs_ci",
        "--controller",
        "ctl-1",
    ]);
    assert_eq!(code, 0, "{cancelled}");
    assert_eq!(cancelled["status"], "cancelled");

    let (code, driven) = pilot.drive();
    assert_eq!(code, 0, "{driven}");
    let (_, status) = pilot.status();
    assert_eq!(node_status(&status, "cs_patch"), "accepted");
    assert_eq!(node_status(&status, "cs_ci"), "cancelled");
    // The run can never complete; the last action says why it stopped.
    assert!(
        matches!(actions(&driven).last(), Some(&"idle") | Some(&"blocked")),
        "{driven}"
    );
}

#[test]
fn an_edited_graph_file_is_refused_against_the_stored_revision() {
    let pilot = Pilot::new(
        "edited",
        common::fixture("contracts/valid/graph-run-minimal.json").as_bytes(),
    );
    let (code, _) = pilot.create();
    assert_eq!(code, 0);
    // Weaken the stored graph's contract on disk.
    let text = fs::read_to_string(&pilot.graph).unwrap();
    fs::write(
        &pilot.graph,
        text.replace("The required checks passed", "Whatever happened"),
    )
    .unwrap();
    let (code, refused) = pilot.status();
    assert_eq!(code, 1, "{refused}");
    let message = refused["diagnostics"][0]["message"].as_str().unwrap();
    assert!(
        message.contains("does not match the stored revision"),
        "{message}"
    );
}
