//! Node and evidence contracts: mandatory checks and inputs cannot disappear,
//! wrong port/output types and unsupported policy versions reject, and a
//! valid shape never implies a behaviour pass.

mod common;

use std::collections::HashSet;

use checkspan::contracts::registry;
use checkspan::contracts::{
    EvidenceRef, FailureClass, NodeContractError as E, NodeKind, NodeSpec, OnExhaustion,
    RetryPolicy,
};
use common::{fixture, fixture_names, schema_errors};
use serde_json::Value;

const NODE_SCHEMA: &str = "https://checkspan.invalid/schemas/v1/node-spec.schema.json";
const EVIDENCE_SCHEMA: &str = "https://checkspan.invalid/schemas/v1/evidence-ref.schema.json";
const OUTCOME_KEYS: [&str; 5] = ["status", "accepted", "verdict", "receipt", "passed"];

type Expect = fn(&E) -> bool;

fn parse_node(text: &str) -> Result<NodeSpec, serde_json::Error> {
    serde_json::from_str(text)
}

fn node_fixture(name: &str) -> String {
    fixture(&format!("nodes/invalid/{name}"))
}

/// Schema rejects, typed parse succeeds, and the contract check names the rule.
fn assert_schema_and_contract(name: &str, expected: Expect) {
    let text = node_fixture(name);
    assert!(
        !schema_errors(NODE_SCHEMA, &text).is_empty(),
        "{name}: schema must reject"
    );
    let node = parse_node(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
    let errors = node.check_contract();
    assert!(errors.iter().any(expected), "{name}: {errors:?}");
}

/// Schema rejects and typed parse fails with a message naming the field.
fn assert_schema_and_shape(name: &str, needle: &str) {
    let text = node_fixture(name);
    assert!(
        !schema_errors(NODE_SCHEMA, &text).is_empty(),
        "{name}: schema must reject"
    );
    let err = parse_node(&text).expect_err(name).to_string();
    assert!(err.contains(needle), "{name}: {err}");
}

fn mentions_outcome(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.keys().any(|k| OUTCOME_KEYS.contains(&k.as_str()))
                || map.values().any(mentions_outcome)
        }
        Value::Array(items) => items.iter().any(mentions_outcome),
        _ => false,
    }
}

#[test]
fn valid_node_fixtures_pass_schema_parse_contract_and_round_trip() {
    let names = fixture_names("nodes/valid");
    assert_eq!(names.len(), 5);
    let mut kinds = HashSet::new();
    for name in names {
        let text = fixture(&format!("nodes/valid/{name}"));
        assert_eq!(
            schema_errors(NODE_SCHEMA, &text),
            Vec::<String>::new(),
            "{name}"
        );
        let node = parse_node(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(node.check_contract(), vec![], "{name}");
        let again: NodeSpec = serde_json::from_str(&serde_json::to_string(&node).unwrap()).unwrap();
        assert_eq!(node, again, "{name} round trip");
        kinds.insert(node.kind);
    }
    assert_eq!(kinds.len(), 4, "every node kind has a valid example");
}

#[test]
fn a_well_formed_contract_carries_no_acceptance() {
    for name in fixture_names("nodes/valid") {
        let value: Value = serde_json::from_str(&fixture(&format!("nodes/valid/{name}"))).unwrap();
        assert!(!mentions_outcome(&value), "{name}");
    }
    let schema: Value = serde_json::from_str(registry::bundled(NODE_SCHEMA).unwrap()).unwrap();
    for key in OUTCOME_KEYS {
        assert!(
            schema["properties"].get(key).is_none(),
            "schema must not offer {key}"
        );
    }
    assert_schema_and_shape("status-field.json", "status");
}

#[test]
fn kind_conditional_rules_are_enforced_by_schema_and_contract() {
    let cases: [(&str, Expect); 5] = [
        ("check-without-deps.json", |e| {
            matches!(e, E::KindRequiresDependencies(NodeKind::Check))
        }),
        ("sink-without-deps.json", |e| {
            matches!(e, E::KindRequiresDependencies(NodeKind::Sink))
        }),
        ("human-gate-without-human-port.json", |e| {
            matches!(e, E::HumanGateNeedsOneHumanPort { found: 0 })
        }),
        ("human-gate-two-human-ports.json", |e| {
            matches!(e, E::HumanGateNeedsOneHumanPort { found: 2 })
        }),
        ("task-with-human-port.json", |e| {
            matches!(
                e,
                E::HumanPortOnNonGate {
                    kind: NodeKind::Task,
                    ..
                }
            )
        }),
    ];
    for (name, expected) in cases {
        assert_schema_and_contract(name, expected);
    }
}

#[test]
fn mandatory_checks_and_inputs_cannot_disappear() {
    assert_schema_and_contract("acceptance-no-checks.json", |e| {
        matches!(e, E::NoRequiredChecks)
    });
    assert_schema_and_contract(
        "port-empty-scope.json",
        |e| matches!(e, E::EmptySourceScope(p) if p.as_str() == "x"),
    );
    assert_schema_and_contract("prompt-empty.json", |e| matches!(e, E::EmptyPrompt));
    for (name, needle) in [
        ("acceptance-missing-checks.json", "required_checks"),
        ("acceptance-missing-verifier.json", "verifier"),
        ("acceptance-weaker-verifier-field.json", "skip_checks"),
        ("result-type-missing.json", "result_type"),
        ("port-missing-required-flag.json", "required"),
        ("port-missing-scope.json", "allowed_source_scope"),
        ("authority-policy-missing.json", "authority_policy_ref"),
    ] {
        assert_schema_and_shape(name, needle);
    }
}

#[test]
fn wrong_port_and_output_types_reject() {
    for (name, needle) in [
        ("kind-unknown.json", "agent"),
        ("dep-bad-port.json", "identifier"),
        ("dep-missing-expected-type.json", "expected_type"),
        ("port-unknown-kind.json", "web"),
        ("verifier-bad-digest.json", "digest"),
        ("result-type-bad-digest.json", "digest"),
        ("result-type-version-zero.json", "version"),
        ("resource-unknown-access.json", "owner"),
    ] {
        assert_schema_and_shape(name, needle);
    }
}

#[test]
fn unsupported_policy_versions_reject() {
    let text = node_fixture("retry-policy-version-2.json");
    assert!(!schema_errors(NODE_SCHEMA, &text).is_empty());
    let err = parse_node(&text).unwrap_err().to_string();
    assert!(err.contains("version 2 is not supported"), "{err}");
    assert!(err.contains("supports 1"), "{err}");
    for (name, needle) in [
        ("policy-ref-version-zero.json", "version"),
        ("retry-unknown-failure-class.json", "undecidable"),
        ("retry-unknown-exhaustion.json", "retry_forever"),
    ] {
        assert_schema_and_shape(name, needle);
    }
    assert_schema_and_contract("retry-zero-attempts.json", |e| {
        matches!(e, E::ZeroMaxAttempts)
    });
}

#[test]
fn contract_rules_the_schema_cannot_express_still_reject() {
    let cases: [(&str, Expect); 6] = [
        (
            "dep-duplicate.json",
            |e| matches!(e, E::DuplicateDependency { output_port, .. } if output_port.as_str() == "result"),
        ),
        (
            "port-duplicate-name.json",
            |e| matches!(e, E::DuplicatePortName(p) if p.as_str() == "x"),
        ),
        (
            "resource-duplicate.json",
            |e| matches!(e, E::DuplicateResource(r) if r.as_str() == "checkout"),
        ),
        (
            "supersedes-other-node.json",
            |e| matches!(e, E::SupersedesDifferentNode(n) if n.node_id.as_str() == "cs_other"),
        ),
        (
            "supersedes-not-earlier.json",
            |e| matches!(e, E::SupersedesNotEarlier(n) if n.revision.get() == 2),
        ),
        ("prompt-whitespace.json", |e| matches!(e, E::EmptyPrompt)),
    ];
    for (name, expected) in cases {
        let text = node_fixture(name);
        assert_eq!(
            schema_errors(NODE_SCHEMA, &text),
            Vec::<String>::new(),
            "{name} is schema-valid by construction"
        );
        let node = parse_node(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        let errors = node.check_contract();
        assert!(errors.iter().any(expected), "{name}: {errors:?}");
        for error in &errors {
            assert!(!error.to_string().is_empty());
        }
    }
}

#[test]
fn retry_policy_defaults_are_explicit_after_parsing() {
    let node = parse_node(&fixture("nodes/valid/task-patch.json")).unwrap();
    assert_eq!(node.retry_policy, RetryPolicy::default());
    assert_eq!(node.retry_policy.max_attempts, 3);
    assert_eq!(
        node.retry_policy.allowed_failure_classes,
        vec![
            FailureClass::Rejected,
            FailureClass::Infrastructure,
            FailureClass::Timeout
        ]
    );
    assert_eq!(node.retry_policy.on_exhaustion, OnExhaustion::Gate);
    let json = serde_json::to_value(&node).unwrap();
    assert_eq!(json["retry_policy"]["version"], 1);
    assert_eq!(json["retry_policy"]["max_attempts"], 3);

    let research = parse_node(&fixture("nodes/valid/task-research.json")).unwrap();
    assert_eq!(research.retry_policy.max_attempts, 2);
    assert_eq!(research.retry_policy.on_exhaustion, OnExhaustion::Cancel);
    assert_eq!(research.resource_scope.len(), 2);
    assert_eq!(research.supersedes.as_ref().unwrap().revision.get(), 1);
}

#[test]
fn evidence_refs_bind_digest_subject_provenance_policy_and_validity() {
    let names = fixture_names("evidence/valid");
    assert_eq!(names.len(), 2);
    for name in names {
        let text = fixture(&format!("evidence/valid/{name}"));
        assert_eq!(
            schema_errors(EVIDENCE_SCHEMA, &text),
            Vec::<String>::new(),
            "{name}"
        );
        let evidence: EvidenceRef =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        let again: EvidenceRef =
            serde_json::from_str(&serde_json::to_string(&evidence).unwrap()).unwrap();
        assert_eq!(evidence, again, "{name} round trip");
        assert!(evidence.content_digest.as_str().starts_with("sha256:"));
    }
    for (name, needle) in [
        ("missing-digest.json", "content_digest"),
        ("bad-digest.json", "digest"),
        ("bad-valid-until.json", "timestamp"),
        ("unknown-field.json", "trusted"),
        ("missing-provenance.json", "provenance"),
    ] {
        let text = fixture(&format!("evidence/invalid/{name}"));
        assert!(!schema_errors(EVIDENCE_SCHEMA, &text).is_empty(), "{name}");
        let err = serde_json::from_str::<EvidenceRef>(&text)
            .expect_err(name)
            .to_string();
        assert!(err.contains(needle), "{name}: {err}");
    }
    let text = fixture("evidence/invalid/empty-subject.json");
    assert!(!schema_errors(EVIDENCE_SCHEMA, &text).is_empty());
}
