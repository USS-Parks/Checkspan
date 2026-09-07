//! Helpers shared by the contract test suites.
#![allow(dead_code)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use jsonschema::{Draft, Retrieve, Uri, Validator};
use serde_json::Value;

/// Serves `$ref` targets from the bundled registry only.
pub struct Bundled;

impl Retrieve for Bundled {
    fn retrieve(&self, uri: &Uri<String>) -> Result<Value, Box<dyn Error + Send + Sync>> {
        registry::bundled(uri.as_str())
            .map(|source| serde_json::from_str(source).expect("bundled schema is JSON"))
            .ok_or_else(|| format!("schema {uri} is not bundled").into())
    }
}

/// Compile a bundled schema with offline resolution and format assertions.
pub fn validator_for(schema_id: &str) -> Validator {
    let root: Value = serde_json::from_str(registry::bundled(schema_id).unwrap()).unwrap();
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .with_retriever(Bundled)
        .should_validate_formats(true)
        .build(&root)
        .expect("bundled schema compiles offline")
}

/// Path of a file under `tests/fixtures`.
pub fn fixture_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}

/// Contents of a file under `tests/fixtures`.
pub fn fixture(rel: &str) -> String {
    let path = fixture_path(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Sorted file names in a directory under `tests/fixtures`.
pub fn fixture_names(dir: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(fixture_path(dir))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Every schema violation of `text` against the bundled schema, with paths.
pub fn schema_errors(schema_id: &str, text: &str) -> Vec<String> {
    let instance: Value = serde_json::from_str(text).unwrap();
    validator_for(schema_id)
        .iter_errors(&instance)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect()
}

use checkspan::contracts::{
    AcceptanceSpec, Dependency, Digest, Exactly, FailureClass, GraphBudget, GraphId, GraphSpec,
    Ident, NodeId, NodeKind, NodeRef, NodeSpec, OnExhaustion, PatchResult, PolicyRef, PortKind,
    PortSpec, Record, RecordKind, RetryPolicy, Revision, SchemaVersion, SoftwareCheckResult,
    TypeRef, VerifierRef, Version, registry,
};
use checkspan::controller::REPO_SOURCE;
use checkspan::digests::artifact_digest;
use checkspan::verifiers::{patch, software};

/// The two-node pilot graph: capture the patch, then run the checks.
pub fn pilot_graph_for(profile_digest: Digest) -> GraphSpec {
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
        version: Exactly,
        max_attempts: 2,
        allowed_failure_classes: vec![FailureClass::Rejected, FailureClass::Infrastructure],
        on_exhaustion: OnExhaustion::Gate,
    };
    let node_ref = |id: &str| NodeRef {
        graph_id: GraphId::new("local_pilot").unwrap(),
        node_id: NodeId::new(id).unwrap(),
        revision: Revision::new(1).unwrap(),
    };
    GraphSpec {
        record: RecordKind::GraphSpec,
        schema_version: SchemaVersion(1),
        graph_id: GraphId::new("local_pilot").unwrap(),
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
