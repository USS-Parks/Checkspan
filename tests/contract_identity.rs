//! Identity and envelope contracts: the same local slug is a different node
//! across graphs and revisions, runs are distinct from graphs, unknown schema
//! versions and missing targets reject, and every bundled schema resolves
//! offline.

mod common;

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use checkspan::contracts::registry::{self, SCHEMAS, SUPPORTED};
use checkspan::contracts::{
    ContractError, GraphId, GraphRun, GraphSpec, IdentityError, NodeId, NodeRef, Record,
    RecordKind, Revision, RunId, parse_record,
};
use common::{Bundled, fixture, schema_errors, validator_for};
use jsonschema::Draft;
use serde_json::Value;

fn schema_id_of(text: &str) -> &'static str {
    let value: Value = serde_json::from_str(text).unwrap();
    match value["record"].as_str() {
        Some("graph_spec") => GraphSpec::SCHEMA_ID,
        Some("graph_run") => GraphRun::SCHEMA_ID,
        other => panic!("fixture has no known record kind: {other:?}"),
    }
}

fn node_ref(graph: &str, node: &str, revision: u32) -> NodeRef {
    NodeRef {
        graph_id: GraphId::new(graph).unwrap(),
        node_id: NodeId::new(node).unwrap(),
        revision: Revision::new(revision).unwrap(),
    }
}

#[test]
fn bundled_schemas_are_valid_draft_2020_12_and_self_consistent() {
    assert!(!SCHEMAS.is_empty());
    for schema in SCHEMAS {
        let value: Value = serde_json::from_str(schema.source).unwrap();
        assert_eq!(value["$id"], schema.id, "bundled id must match $id");
        assert_eq!(
            value["$schema"], "https://json-schema.org/draft/2020-12/schema",
            "{}",
            schema.id
        );
        assert!(schema.id.starts_with(registry::BASE_URI));
        jsonschema::meta::validate(&value).unwrap_or_else(|e| panic!("{}: {e}", schema.id));
    }
    let ids: HashSet<_> = SCHEMAS.iter().map(|s| s.id).collect();
    assert_eq!(ids.len(), SCHEMAS.len(), "schema ids are unique");
}

#[test]
fn every_supported_record_maps_to_a_bundled_schema() {
    for (kind, version, id) in SUPPORTED {
        assert!(registry::bundled(id).is_some(), "{id}");
        assert_eq!(registry::schema_for(*kind, *version), Some(*id));
        validator_for(id);
    }
    assert_eq!(
        registry::schema_for(GraphSpec::KIND, GraphSpec::VERSION),
        Some(GraphSpec::SCHEMA_ID)
    );
    assert_eq!(
        registry::schema_for(GraphRun::KIND, GraphRun::VERSION),
        Some(GraphRun::SCHEMA_ID)
    );
    assert_eq!(
        registry::schema_for(
            RecordKind::GraphSpec,
            checkspan::contracts::SchemaVersion(2)
        ),
        None
    );
}

#[test]
fn unbundled_schema_references_are_never_fetched() {
    for missing in [
        "https://checkspan.invalid/schemas/v1/missing.schema.json",
        "https://example.com/anything.json",
        "file:///etc/passwd",
    ] {
        let root: Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://checkspan.invalid/schemas/v1/test-root.schema.json",
            "$ref": missing
        });
        let built = jsonschema::options()
            .with_draft(Draft::Draft202012)
            .with_retriever(Bundled)
            .build(&root);
        assert!(built.is_err(), "{missing} must not resolve");
    }
}

#[test]
fn valid_fixtures_pass_schema_parse_identity_and_round_trip() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/contracts/valid");
    let mut count = 0;
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let text = fs::read_to_string(&path).unwrap();
        let schema_id = schema_id_of(&text);
        assert_eq!(
            schema_errors(schema_id, &text),
            Vec::<String>::new(),
            "{name}"
        );
        if schema_id == GraphSpec::SCHEMA_ID {
            let spec: GraphSpec = parse_record(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(spec.check_identity(), vec![], "{name}");
            let again: GraphSpec = parse_record(&serde_json::to_string(&spec).unwrap()).unwrap();
            assert_eq!(spec, again, "{name} round trip");
        } else {
            let run: GraphRun = parse_record(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(run.check_identity(), vec![], "{name}");
            let again: GraphRun = parse_record(&serde_json::to_string(&run).unwrap()).unwrap();
            assert_eq!(run, again, "{name} round trip");
        }
        count += 1;
    }
    assert_eq!(count, 5, "every valid fixture was exercised");
}

#[test]
fn same_local_slug_is_a_different_node_across_graphs_and_revisions() {
    let a1 = node_ref("graph_a", "cs_patch", 1);
    let a2 = node_ref("graph_a", "cs_patch", 2);
    let b1 = node_ref("graph_b", "cs_patch", 1);
    assert_eq!(a1.node_id, a2.node_id);
    assert_eq!(a1.node_id, b1.node_id);
    assert_ne!(a1, a2);
    assert_ne!(a1, b1);
    assert_ne!(a2, b1);
    let distinct: HashSet<_> = [a1.clone(), a2.clone(), b1.clone(), a1.clone()]
        .into_iter()
        .collect();
    assert_eq!(distinct.len(), 3);
    assert_eq!(a1.to_string(), "graph_a/cs_patch@1");
    assert_eq!(b1.to_string(), "graph_b/cs_patch@1");
}

#[test]
fn node_ref_resolves_through_the_owning_graph_revision() {
    let spec: GraphSpec = parse_record(&fixture("contracts/valid/graph-spec-full.json")).unwrap();
    let ci = NodeId::new("cs_ci").unwrap();
    assert_eq!(
        spec.node_ref(&ci),
        Some(node_ref("review_pilot", "cs_ci", 2))
    );
    assert_eq!(spec.node_ref(&NodeId::new("cs_missing").unwrap()), None);
    assert_eq!(spec.graph_ref().revision.get(), 2);
    assert_eq!(spec.supersedes.as_ref().unwrap().revision.get(), 1);
}

#[test]
fn two_runs_of_one_graph_revision_are_distinct_runs() {
    let first: GraphRun = parse_record(&fixture("contracts/valid/graph-run-minimal.json")).unwrap();
    let mut second = first.clone();
    second.run_id = RunId::new("run-0009").unwrap();
    assert_eq!(first.graph_ref, second.graph_ref);
    assert_ne!(first.run_id, second.run_id);
    assert_ne!(first, second);
    let lineage: GraphRun =
        parse_record(&fixture("contracts/valid/graph-run-lineage.json")).unwrap();
    assert_eq!(lineage.budget_lineage_ref, Some(first.run_id.clone()));
    assert_ne!(
        lineage.graph_ref, first.graph_ref,
        "lineage may cross revisions"
    );
}

#[test]
fn display_name_carries_no_identity() {
    let spec: GraphSpec = parse_record(&fixture("contracts/valid/graph-spec-full.json")).unwrap();
    let mut renamed = spec.clone();
    renamed.display_name = Some("a completely different label".into());
    renamed.nodes[0].display_name = None;
    assert_ne!(spec, renamed);
    assert_eq!(spec.graph_ref(), renamed.graph_ref());
    let patch = NodeId::new("cs_patch").unwrap();
    assert_eq!(spec.node_ref(&patch), renamed.node_ref(&patch));
}

#[test]
fn unknown_schema_version_is_rejected_before_the_body_is_examined() {
    let text = fixture("contracts/invalid/schema-unknown-version.json");
    assert!(!schema_errors(GraphSpec::SCHEMA_ID, &text).is_empty());
    match parse_record::<GraphSpec>(&text) {
        Err(ContractError::UnsupportedVersion {
            record,
            found,
            supported,
        }) => {
            assert_eq!(record, RecordKind::GraphSpec);
            assert_eq!(found.0, 2);
            assert_eq!(supported, GraphSpec::VERSION);
        }
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
}

#[test]
fn wrong_unknown_and_missing_record_headers_are_rejected() {
    match parse_record::<GraphSpec>(&fixture("contracts/invalid/schema-wrong-record.json")) {
        Err(ContractError::WrongRecord { expected, found }) => {
            assert_eq!(expected, RecordKind::GraphSpec);
            assert_eq!(found, "graph_run");
        }
        other => panic!("expected WrongRecord, got {other:?}"),
    }
    match parse_record::<GraphSpec>(&fixture("contracts/invalid/schema-unknown-record.json")) {
        Err(ContractError::WrongRecord { found, .. }) => assert_eq!(found, "graph"),
        other => panic!("expected WrongRecord, got {other:?}"),
    }
    assert!(matches!(
        parse_record::<GraphSpec>(&fixture("contracts/invalid/schema-missing-header.json")),
        Err(ContractError::Header(_))
    ));
    assert!(matches!(
        parse_record::<GraphSpec>("not json"),
        Err(ContractError::Json(_))
    ));
    for name in [
        "contracts/invalid/schema-wrong-record.json",
        "contracts/invalid/schema-unknown-record.json",
        "contracts/invalid/schema-missing-header.json",
    ] {
        assert!(
            !schema_errors(GraphSpec::SCHEMA_ID, &fixture(name)).is_empty(),
            "{name}"
        );
    }
}

#[test]
fn shape_violations_fail_both_schema_and_typed_parse() {
    let cases = [
        ("contracts/invalid/schema-missing-targets.json", "targets"),
        ("contracts/invalid/schema-unknown-field.json", "owner"),
        ("contracts/invalid/schema-bad-identifier.json", "graph_id"),
        ("contracts/invalid/schema-revision-zero.json", "revision"),
        ("contracts/invalid/schema-bad-deadline.json", "timestamp"),
        (
            "contracts/invalid/schema-node-missing-revision.json",
            "revision",
        ),
    ];
    for (name, needle) in cases {
        let text = fixture(name);
        assert!(
            !schema_errors(GraphSpec::SCHEMA_ID, &text).is_empty(),
            "{name}"
        );
        match parse_record::<GraphSpec>(&text) {
            Err(ContractError::Shape(e)) => {
                assert!(e.to_string().contains(needle), "{name}: {e}");
            }
            other => panic!("{name}: expected Shape error, got {other:?}"),
        }
    }
}

#[test]
fn missing_or_empty_targets_and_zero_budget_reject() {
    let text = fixture("contracts/invalid/schema-empty-targets.json");
    assert!(!schema_errors(GraphSpec::SCHEMA_ID, &text).is_empty());
    let spec: GraphSpec = parse_record(&text).unwrap();
    assert_eq!(spec.check_identity(), vec![IdentityError::NoTargets]);

    let text = fixture("contracts/invalid/schema-zero-attempts.json");
    assert!(!schema_errors(GraphSpec::SCHEMA_ID, &text).is_empty());
    let spec: GraphSpec = parse_record(&text).unwrap();
    assert_eq!(
        spec.check_identity(),
        vec![IdentityError::ZeroAttemptBudget]
    );
}

#[test]
fn identity_fixtures_pass_the_schema_but_fail_identity_rules() {
    type Expect = fn(&IdentityError) -> bool;
    let cases: [(&str, Expect); 7] = [
        (
            "contracts/invalid/identity-duplicate-node.json",
            |e| matches!(e, IdentityError::DuplicateNodeId(id) if id.as_str() == "cs_packet"),
        ),
        (
            "contracts/invalid/identity-target-unknown.json",
            |e| matches!(e, IdentityError::UnknownTarget(t) if t.node_id.as_str() == "cs_missing"),
        ),
        (
            "contracts/invalid/identity-target-foreign-graph.json",
            |e| matches!(e, IdentityError::ForeignTarget(t) if t.graph_id.as_str() == "other_graph"),
        ),
        (
            "contracts/invalid/identity-target-revision-mismatch.json",
            |e| {
                matches!(
                    e,
                    IdentityError::TargetRevisionMismatch { target, actual }
                        if target.revision.get() == 2 && actual.get() == 1
                )
            },
        ),
        ("contracts/invalid/identity-duplicate-target.json", |e| {
            matches!(e, IdentityError::DuplicateTarget(_))
        }),
        (
            "contracts/invalid/identity-supersedes-not-earlier.json",
            |e| matches!(e, IdentityError::SupersedesNotEarlier(g) if g.revision.get() == 2),
        ),
        ("contracts/invalid/identity-supersedes-foreign.json", |e| {
            matches!(e, IdentityError::SupersedesForeignGraph(_))
        }),
    ];
    for (name, expected) in cases {
        let text = fixture(name);
        assert_eq!(
            schema_errors(GraphSpec::SCHEMA_ID, &text),
            Vec::<String>::new(),
            "{name} must be schema-valid"
        );
        let spec: GraphSpec = parse_record(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        let errors = spec.check_identity();
        assert!(
            errors.iter().any(expected),
            "{name}: expected identity error not found in {errors:?}"
        );
        for error in &errors {
            assert!(!error.to_string().is_empty());
        }
    }
}

#[test]
fn imports_must_be_unique_per_node_and_never_from_this_run() {
    for (name, expect) in [
        (
            "contracts/invalid/identity-import-duplicate.json",
            "more than one admitted import",
        ),
        (
            "contracts/invalid/identity-import-self.json",
            "names a receipt from this run",
        ),
    ] {
        let text = fixture(name);
        assert_eq!(
            schema_errors(GraphRun::SCHEMA_ID, &text),
            Vec::<String>::new(),
            "{name}"
        );
        let run: GraphRun = parse_record(&text).unwrap();
        let errors = run.check_identity();
        assert_eq!(errors.len(), 1, "{name}: {errors:?}");
        assert!(
            errors[0].to_string().contains(expect),
            "{name}: {}",
            errors[0]
        );
    }
    let run: GraphRun =
        parse_record(&fixture("contracts/valid/graph-run-with-import.json")).unwrap();
    assert_eq!(run.admitted_imports.len(), 1);
    assert_eq!(run.admitted_imports[0].receipt.run_id.as_str(), "run-0001");
}

#[test]
fn a_run_cannot_be_its_own_budget_lineage() {
    let text = fixture("contracts/invalid/identity-run-self-lineage.json");
    assert_eq!(
        schema_errors(GraphRun::SCHEMA_ID, &text),
        Vec::<String>::new()
    );
    let run: GraphRun = parse_record(&text).unwrap();
    assert_eq!(
        run.check_identity(),
        vec![IdentityError::SelfLineage(run.run_id.clone())]
    );
}
