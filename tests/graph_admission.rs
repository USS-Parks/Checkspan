//! Graph admission: chains, diamonds, isolated targets, unresolved
//! references, cycles, incompatible ports and types, hidden proof
//! dependencies, external imports, and gate wait cycles produce
//! deterministic outcomes.

mod common;

use checkspan::contracts::{GraphSpec, IdentityError, NodeContractError, NodeId, parse_record};
use checkspan::graph::{AdmissionError as E, admission_diagnostics, admit};
use checkspan::validation::Stage;
use common::{fixture, fixture_names};

fn spec(name: &str) -> GraphSpec {
    parse_record(&fixture(&format!("graphs/{name}"))).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn ids(list: &[NodeId]) -> Vec<&str> {
    list.iter().map(NodeId::as_str).collect()
}

fn errors(name: &str) -> Vec<E> {
    admit(&spec(name)).expect_err(name)
}

#[test]
fn every_valid_graph_fixture_admits_and_is_deterministic() {
    let names = fixture_names("graphs/valid");
    assert_eq!(names.len(), 4);
    for name in names {
        let spec = spec(&format!("valid/{name}"));
        let first = admit(&spec).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let second = admit(&spec).unwrap();
        assert_eq!(first, second, "{name}: admission is deterministic");
        assert_eq!(first.order.len(), spec.nodes.len(), "{name}");
        assert_eq!(
            first.required.len() + first.optional.len(),
            spec.nodes.len(),
            "{name}"
        );
        for target in &spec.targets {
            assert!(first.required.contains(&target.node_id), "{name}");
        }
        assert!(admission_diagnostics(&spec).is_empty(), "{name}");
    }
}

#[test]
fn chain_with_gate_orders_producers_first_and_binds_the_gate() {
    let graph = admit(&spec("valid/chain-with-gate.json")).unwrap();
    assert_eq!(
        ids(&graph.order),
        ["cs_patch", "cs_ci", "cs_ci_gate", "cs_packet"]
    );
    assert_eq!(ids(&graph.required), ["cs_patch", "cs_ci", "cs_packet"]);
    assert_eq!(ids(&graph.optional), ["cs_ci_gate"]);
    assert_eq!(graph.gates.len(), 1);
    assert_eq!(graph.gates[0].gate.as_str(), "cs_ci_gate");
    assert_eq!(ids(&graph.gates[0].resolves), ["cs_ci"]);
    let ci = NodeId::new("cs_ci").unwrap();
    assert_eq!(ids(&graph.producers(&ci)), ["cs_patch"]);
    assert_eq!(ids(&graph.dependents(&ci)), ["cs_packet"]);
}

#[test]
fn diamond_orders_both_branches_before_the_join() {
    let graph = admit(&spec("valid/diamond.json")).unwrap();
    assert_eq!(
        ids(&graph.order),
        [
            "cs_requirements",
            "cs_implementation",
            "cs_synthesis",
            "cs_challenge",
            "cs_evidence_check",
            "cs_packet"
        ]
    );
    assert_eq!(graph.required.len(), 6);
    assert!(graph.optional.is_empty());
    assert!(graph.gates.is_empty());
    let synthesis = NodeId::new("cs_synthesis").unwrap();
    assert_eq!(
        ids(&graph.producers(&synthesis)),
        ["cs_requirements", "cs_implementation"]
    );
    assert_eq!(
        ids(&graph.dependents(&synthesis)),
        ["cs_challenge", "cs_evidence_check"]
    );
}

#[test]
fn isolated_target_admits_and_unrelated_nodes_are_optional() {
    let graph = admit(&spec("valid/isolated-target.json")).unwrap();
    assert_eq!(ids(&graph.order), ["cs_lone", "cs_side", "cs_down"]);
    assert_eq!(ids(&graph.required), ["cs_lone"]);
    assert_eq!(ids(&graph.optional), ["cs_side", "cs_down"]);
}

#[test]
fn node_revisions_are_independent_of_the_graph_revision() {
    let graph = admit(&spec("valid/mixed-revisions.json")).unwrap();
    assert_eq!(graph.spec.revision.get(), 4);
    assert_eq!(
        graph
            .node(&NodeId::new("cs_patch").unwrap())
            .unwrap()
            .revision
            .get(),
        3
    );
    assert_eq!(ids(&graph.order), ["cs_patch", "cs_ci", "cs_packet"]);
}

#[test]
fn unresolved_external_mismatched_and_self_dependencies_reject() {
    assert!(matches!(
        errors("invalid/unresolved-dependency.json").as_slice(),
        [E::UnresolvedDependency { node, dependency }]
            if node.as_str() == "cs_ci" && dependency.node_id.as_str() == "cs_missing"
    ));
    assert!(matches!(
        errors("invalid/external-dependency.json").as_slice(),
        [E::ExternalDependency { dependency, .. }] if dependency.graph_id.as_str() == "other_graph"
    ));
    assert!(matches!(
        errors("invalid/revision-mismatch.json").as_slice(),
        [E::DependencyRevisionMismatch { dependency, actual, .. }]
            if dependency.revision.get() == 2 && actual.get() == 1
    ));
    assert!(matches!(
        errors("invalid/self-dependency.json").as_slice(),
        [E::SelfDependency(node)] if node.as_str() == "cs_ci"
    ));
}

#[test]
fn incompatible_ports_and_types_reject() {
    assert!(matches!(
        errors("invalid/unknown-output-port.json").as_slice(),
        [E::UnknownOutputPort { port, producer, .. }]
            if port.as_str() == "artifact" && producer.as_str() == "cs_patch"
    ));
    let mismatch = errors("invalid/incompatible-type.json");
    assert!(matches!(
        mismatch.as_slice(),
        [E::IncompatibleType { expected, actual, .. }]
            if expected.schema_id.as_str() == "software_check_result"
                && actual.schema_id.as_str() == "patch_result"
    ));
    assert!(
        mismatch[0]
            .to_string()
            .contains("which produces patch_result@1")
    );
    let drift = errors("invalid/incompatible-digest.json");
    assert!(matches!(drift.as_slice(), [E::IncompatibleType { .. }]));
    assert!(drift[0].to_string().contains("digest"), "{}", drift[0]);
}

#[test]
fn cycles_are_reported_once_with_a_deterministic_path() {
    let two = errors("invalid/cycle-two.json");
    assert_eq!(two.len(), 1);
    match &two[0] {
        E::Cycle(path) => assert_eq!(ids(path), ["cs_patch", "cs_ci"]),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        two[0].to_string(),
        "dependency cycle: cs_patch -> cs_ci -> cs_patch"
    );
    let three = errors("invalid/cycle-three.json");
    assert_eq!(three.len(), 1);
    match &three[0] {
        E::Cycle(path) => assert_eq!(ids(path), ["cs_patch", "cs_packet", "cs_ci"]),
        other => panic!("{other:?}"),
    }
    assert_eq!(errors("invalid/cycle-three.json"), three, "deterministic");
}

#[test]
fn hidden_and_unsupported_proof_sources_reject() {
    assert!(matches!(
        errors("invalid/hidden-proof-dependency.json").as_slice(),
        [E::HiddenProofDependency { node, port, source }]
            if node.as_str() == "cs_packet" && port.as_str() == "prior" && source.as_str() == "cs_ci_gate"
    ));
    assert!(matches!(
        errors("invalid/unresolved-proof-source.json").as_slice(),
        [E::UnresolvedProofSource { source, .. }] if source == "cs_missing"
    ));
    assert!(matches!(
        errors("invalid/external-proof-import.json").as_slice(),
        [E::UnsupportedExternalImport { source, .. }] if source.starts_with("receipt:")
    ));
}

#[test]
fn gate_wait_cycles_reject() {
    let e = errors("invalid/gate-depends-on-resolved.json");
    assert!(
        matches!(
            e.as_slice(),
            [E::GateWaitCycle { gate, resolves, path }]
                if gate.as_str() == "cs_ci_gate" && resolves.as_str() == "cs_ci"
                    && ids(path) == ["cs_ci_gate", "cs_ci"]
        ),
        "{e:?}"
    );
    let e = errors("invalid/resolved-depends-on-gate.json");
    assert!(
        matches!(
            e.as_slice(),
            [E::GateWaitCycle { path, .. }] if ids(path) == ["cs_ci", "cs_ci_gate"]
        ),
        "{e:?}"
    );
    let e = errors("invalid/gate-resolves-itself.json");
    assert!(
        matches!(
            e.as_slice(),
            [E::GateWaitCycle { path, .. }] if ids(path) == ["cs_ci_gate"]
        ),
        "{e:?}"
    );
    assert!(matches!(
        errors("invalid/gate-resolves-unknown.json").as_slice(),
        [E::GateResolvesUnknownNode { target, .. }] if target == "cs_nowhere"
    ));
}

#[test]
fn admission_composes_identity_and_contract_rules_in_document_order() {
    let e = errors("invalid/identity-and-contract.json");
    assert_eq!(e.len(), 2, "{e:?}");
    assert!(matches!(
        &e[0],
        E::Identity(IdentityError::UnknownTarget(t)) if t.node_id.as_str() == "cs_missing"
    ));
    assert!(matches!(
        &e[1],
        E::Contract { node, error: NodeContractError::DuplicateRequiredCheck(c) }
            if node.as_str() == "cs_ci" && c.as_str() == "fmt"
    ));
    let diagnostics = admission_diagnostics(&spec("invalid/identity-and-contract.json"));
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().all(|d| d.stage == Stage::Admission));
    assert_eq!(diagnostics[0].path, "");
    assert_eq!(diagnostics[1].path, "/nodes/1");
}

#[test]
fn every_invalid_graph_fixture_rejects_with_at_least_one_reason() {
    let names = fixture_names("graphs/invalid");
    assert_eq!(names.len(), 17);
    for name in names {
        let e = errors(&format!("invalid/{name}"));
        assert!(!e.is_empty(), "{name}");
        for error in &e {
            assert!(!error.to_string().is_empty());
        }
    }
}
