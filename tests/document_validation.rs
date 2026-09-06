//! Bounded offline document validation: oversized, deep, duplicate-key, and
//! traversal/remote-reference inputs reject before any interpretation or
//! network access, every chosen Draft 2020-12 keyword has positive and
//! negative coverage, and diagnostics carry field paths.

mod common;

use std::collections::HashSet;
use std::fs;

use checkspan::contracts::RecordKind;
use checkspan::contracts::registry;
use checkspan::validation::schema::{SUPPORTED_KEYWORDS, compile, unsupported_keywords};
use checkspan::validation::{Limits, Report, Stage, ValidatedRecord, validate};
use common::{fixture, fixture_names, fixture_path};
use serde_json::{Value, json};

fn check(text: &str) -> Report {
    validate(text.as_bytes(), &Limits::default())
}

fn stages(report: &Report) -> HashSet<Stage> {
    report.diagnostics.iter().map(|d| d.stage).collect()
}

#[test]
fn every_valid_top_level_fixture_validates_to_its_record() {
    let mut kinds = HashSet::new();
    for dir in ["contracts/valid", "outcomes/valid"] {
        for name in fixture_names(dir) {
            let report = check(&fixture(&format!("{dir}/{name}")));
            assert!(report.is_valid(), "{dir}/{name}: {:?}", report.diagnostics);
            let record = report.record.as_ref().unwrap();
            let header = report.header.as_ref().unwrap();
            assert_eq!(record.kind().as_str(), header.record, "{dir}/{name}");
            kinds.insert(record.kind());
        }
    }
    assert_eq!(
        kinds.len(),
        registry::SUPPORTED.len(),
        "every record kind exercised"
    );
}

#[test]
fn every_invalid_top_level_fixture_is_rejected_by_exactly_one_stage() {
    let mut seen = HashSet::new();
    for dir in ["contracts/invalid", "outcomes/invalid"] {
        for name in fixture_names(dir) {
            let report = check(&fixture(&format!("{dir}/{name}")));
            assert!(!report.is_valid(), "{dir}/{name} must be rejected");
            assert!(
                report.record.is_none(),
                "{dir}/{name} must not yield a record"
            );
            let stages = stages(&report);
            assert_eq!(stages.len(), 1, "{dir}/{name}: one stage, got {stages:?}");
            seen.extend(stages);
        }
    }
    assert!(seen.contains(&Stage::Header));
    assert!(seen.contains(&Stage::Schema));
    assert!(seen.contains(&Stage::Contract));
}

#[test]
fn oversized_input_rejects_before_parsing() {
    let text = fixture("contracts/valid/graph-spec-minimal.json");
    let limits = Limits {
        max_bytes: text.len() - 1,
        max_depth: 64,
    };
    let report = validate(text.as_bytes(), &limits);
    assert_eq!(report.failed_stage(), Some(Stage::Size));
    assert!(report.header.is_none(), "nothing was interpreted");
    let ok = validate(
        text.as_bytes(),
        &Limits {
            max_bytes: text.len(),
            max_depth: 64,
        },
    );
    assert!(ok.is_valid());
}

#[test]
fn deep_nesting_rejects_during_parsing_with_a_path() {
    let mut value: Value =
        serde_json::from_str(&fixture("contracts/valid/graph-spec-minimal.json")).unwrap();
    let mut deep = json!(1);
    for _ in 0..70 {
        deep = json!([deep]);
    }
    value["nodes"][0]["display_name"] = deep;
    let report = check(&value.to_string());
    assert_eq!(report.failed_stage(), Some(Stage::Depth));
    assert!(
        report.diagnostics[0]
            .path
            .starts_with("/nodes/0/display_name/0/0")
    );
    assert!(
        report.header.is_none(),
        "parsing stopped before the header was read"
    );
    let shallow = validate(
        value.to_string().as_bytes(),
        &Limits {
            max_bytes: 4 << 20,
            max_depth: 128,
        },
    );
    assert_eq!(
        shallow.failed_stage(),
        Some(Stage::Schema),
        "with a looser limit the same document reaches the schema stage"
    );
}

#[test]
fn duplicate_keys_reject_with_the_repeated_key_path() {
    let text = fixture("contracts/valid/graph-spec-minimal.json");
    let dup = text.replacen(
        "\"graph_id\": \"review_pilot\",",
        "\"graph_id\": \"review_pilot\",\n  \"graph_id\": \"other\",",
        1,
    );
    assert_ne!(dup, text);
    let report = check(&dup);
    assert_eq!(report.failed_stage(), Some(Stage::DuplicateKey));
    assert_eq!(report.diagnostics[0].path, "/graph_id");
    let nested = text.replacen(
        "\"id\": \"cs_packet\",",
        "\"id\": \"cs_packet\",\n      \"id\": \"cs_other\",",
        1,
    );
    let report = check(&nested);
    assert_eq!(report.failed_stage(), Some(Stage::DuplicateKey));
    assert_eq!(report.diagnostics[0].path, "/nodes/0/id");
}

#[test]
fn syntax_trailing_content_and_encoding_reject() {
    for text in [
        "",
        "not json",
        "{\"record\": }",
        "[1, 2",
        "{} {}",
        "{}\n{\"record\":\"graph_spec\"}",
    ] {
        let report = check(text);
        assert_eq!(report.failed_stage(), Some(Stage::Syntax), "{text:?}");
    }
    let report = validate(&[0xff, 0xfe, b'{', b'}'], &Limits::default());
    assert_eq!(report.failed_stage(), Some(Stage::Syntax));
    assert!(report.diagnostics[0].message.contains("UTF-8"));
}

#[test]
fn header_stage_rejects_non_objects_and_unsupported_records() {
    for text in ["[]", "1", "\"graph_spec\"", "null"] {
        let report = check(text);
        assert_eq!(report.failed_stage(), Some(Stage::Header), "{text:?}");
        assert!(report.diagnostics[0].message.contains("not a JSON object"));
    }
    let report = check("{\"nodes\": []}");
    assert_eq!(report.failed_stage(), Some(Stage::Header));
    assert!(report.header.is_none());

    let report = check(&fixture("contracts/invalid/schema-unknown-version.json"));
    assert_eq!(report.failed_stage(), Some(Stage::Header));
    let header = report.header.as_ref().unwrap();
    assert_eq!(header.record, "graph_spec");
    assert_eq!(header.schema_version.0, 2);
    let message = &report.diagnostics[0].message;
    assert!(message.contains("not supported"), "{message}");
    for (kind, version, _) in registry::SUPPORTED {
        assert!(message.contains(&format!("{kind}@{version}")), "{message}");
    }
    let report = check(&fixture("contracts/invalid/schema-unknown-record.json"));
    assert_eq!(report.failed_stage(), Some(Stage::Header));
    assert_eq!(report.header.as_ref().unwrap().record, "graph");
}

#[test]
fn schema_diagnostics_carry_field_paths() {
    let report = check(&fixture("contracts/invalid/schema-bad-identifier.json"));
    assert_eq!(report.failed_stage(), Some(Stage::Schema));
    assert_eq!(report.diagnostics[0].path, "/graph_id");

    let report = check(&fixture(
        "contracts/invalid/schema-node-missing-revision.json",
    ));
    assert_eq!(report.failed_stage(), Some(Stage::Schema));
    assert_eq!(report.diagnostics[0].path, "/nodes/0");
    assert!(report.diagnostics[0].message.contains("revision"));

    let report = check(&fixture("outcomes/invalid/receipt-verdict-timed-out.json"));
    assert_eq!(report.failed_stage(), Some(Stage::Schema));
    assert!(
        report.diagnostics.iter().any(|d| d.path == "/verdict"),
        "{:?}",
        report.diagnostics
    );

    let report = check(&fixture(
        "outcomes/invalid/view-accepted-without-receipt.json",
    ));
    assert_eq!(report.failed_stage(), Some(Stage::Schema));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("receipt"))
    );
}

#[test]
fn contract_diagnostics_name_the_containing_field() {
    let report = check(&fixture("contracts/invalid/identity-target-unknown.json"));
    assert_eq!(report.failed_stage(), Some(Stage::Contract));
    assert_eq!(report.diagnostics[0].path, "/targets");
    assert!(report.diagnostics[0].message.contains("cs_missing"));

    let report = check(&fixture("contracts/invalid/identity-duplicate-node.json"));
    assert_eq!(report.diagnostics[0].path, "/nodes");

    let report = check(&fixture("contracts/invalid/identity-run-self-lineage.json"));
    assert_eq!(report.diagnostics[0].path, "/budget_lineage_ref");

    let report = check(&fixture("outcomes/invalid/attempt-receipt-other-node.json"));
    assert_eq!(report.failed_stage(), Some(Stage::Contract));
    assert!(report.diagnostics[0].message.contains("another node"));

    let report = check(&fixture(
        "outcomes/invalid/view-accepted-receipt-other-node.json",
    ));
    assert_eq!(report.diagnostics[0].path, "/status");

    // A node-level contract rule inside a graph is reported at that node.
    let mut value: Value =
        serde_json::from_str(&fixture("contracts/valid/graph-spec-full.json")).unwrap();
    value["nodes"][1]["deps"] = json!([]);
    let report = check(&value.to_string());
    assert_eq!(
        report.failed_stage(),
        Some(Stage::Schema),
        "the schema catches it first"
    );
    value["nodes"][1]["deps"] = json!([
        value["nodes"][2]["deps"][0].clone(),
        value["nodes"][2]["deps"][0].clone()
    ]);
    let report = check(&value.to_string());
    assert_eq!(report.failed_stage(), Some(Stage::Contract));
    assert_eq!(report.diagnostics[0].path, "/nodes/1");
    assert!(report.diagnostics[0].message.contains("more than once"));
}

#[test]
fn a_valid_document_yields_the_typed_record() {
    let report = check(&fixture("contracts/valid/graph-spec-full.json"));
    match report.record {
        Some(ValidatedRecord::GraphSpec(spec)) => {
            assert_eq!(spec.nodes.len(), 3);
            assert_eq!(spec.revision.get(), 2);
        }
        other => panic!("expected a graph spec, got {other:?}"),
    }
    let report = check(&fixture("outcomes/valid/receipt-accept.json"));
    assert_eq!(report.record.unwrap().kind(), RecordKind::VerifierReceipt);
}

#[test]
fn remote_file_and_traversal_references_never_resolve() {
    for target in [
        "https://example.com/schema.json",
        "http://checkspan.invalid/schemas/v1/common.schema.json",
        "file:///etc/passwd",
        "file:///C:/Windows/win.ini",
        "../../../etc/passwd",
        "../v0/common.schema.json",
        "missing.schema.json",
        "common.schema.json#/$defs/no_such_def",
    ] {
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://checkspan.invalid/schemas/v1/probe.schema.json",
            "$ref": target
        });
        assert!(compile(&schema).is_err(), "{target} must not resolve");
    }
    let ok = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://checkspan.invalid/schemas/v1/probe.schema.json",
        "$ref": "common.schema.json#/$defs/identifier"
    });
    let validator = compile(&ok).unwrap();
    assert!(validator.is_valid(&json!("cs_ok")));
    assert!(!validator.is_valid(&json!("Not Ok")));
}

#[test]
fn no_network_or_filesystem_resolver_is_compiled_in() {
    let lock = fs::read_to_string(fixture_path("../../Cargo.lock")).unwrap();
    for crate_name in [
        "reqwest",
        "hyper",
        "rustls",
        "aws-lc-rs",
        "tokio",
        "ureq",
        "url",
    ] {
        assert!(
            !lock.contains(&format!("name = \"{crate_name}\"")),
            "{crate_name} must not be in the dependency graph"
        );
    }
}

#[test]
fn bundled_schemas_use_only_the_frozen_keyword_set() {
    let mut used = HashSet::new();
    for schema in registry::SCHEMAS {
        let value: Value = serde_json::from_str(schema.source).unwrap();
        assert_eq!(
            unsupported_keywords(&value),
            Vec::<String>::new(),
            "{}",
            schema.id
        );
        collect_keywords(&value, &mut used);
    }
    let frozen: HashSet<&str> = SUPPORTED_KEYWORDS.iter().copied().collect();
    let unused: Vec<&&str> = frozen.iter().filter(|k| !used.contains(**k)).collect();
    assert!(
        unused.is_empty(),
        "frozen keywords not exercised by the corpus: {unused:?}"
    );
}

fn collect_keywords(node: &Value, used: &mut HashSet<String>) {
    let Value::Object(map) = node else {
        return;
    };
    for (key, value) in map {
        match key.as_str() {
            "properties" | "$defs" => {
                used.insert(key.clone());
                if let Value::Object(children) = value {
                    children.values().for_each(|c| collect_keywords(c, used));
                }
            }
            "allOf" => {
                used.insert(key.clone());
                if let Value::Array(items) = value {
                    items.iter().for_each(|c| collect_keywords(c, used));
                }
            }
            "items" | "contains" | "not" | "if" | "then" | "else" | "additionalProperties" => {
                used.insert(key.clone());
                collect_keywords(value, used);
            }
            other => {
                used.insert(other.to_string());
            }
        }
    }
}

#[test]
fn unsupported_schema_features_are_refused_before_compilation() {
    for (keyword, body) in [
        ("$dynamicRef", json!("#meta")),
        ("unevaluatedProperties", json!(false)),
        ("dependentSchemas", json!({})),
        ("patternProperties", json!({})),
        ("$anchor", json!("x")),
        ("anyOf", json!([{"type": "string"}])),
        ("oneOf", json!([{"type": "string"}])),
        ("uniqueItems", json!(true)),
        ("$comment", json!("hidden")),
    ] {
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://checkspan.invalid/schemas/v1/probe.schema.json",
            "type": "object",
            "properties": { "x": { keyword: body } }
        });
        let found = unsupported_keywords(&schema);
        assert_eq!(found, vec![format!("/properties/x/{keyword}:{keyword}")]);
        assert!(compile(&schema).is_err(), "{keyword}");
    }
    // Property names that look like keywords are data, not keywords.
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://checkspan.invalid/schemas/v1/probe.schema.json",
        "type": "object",
        "properties": { "anyOf": { "type": "string" }, "$dynamicRef": { "type": "string" } },
        "$defs": { "oneOf": { "type": "integer" } }
    });
    assert_eq!(unsupported_keywords(&schema), Vec::<String>::new());
    assert!(compile(&schema).is_ok());
}

#[test]
fn every_frozen_keyword_has_positive_and_negative_coverage() {
    let base = "https://checkspan.invalid/schemas/v1/";
    let cases: Vec<(&str, Value, Value, Value)> = vec![
        ("type", json!({"type": "integer"}), json!(1), json!("1")),
        (
            "properties",
            json!({"properties": {"a": {"type": "integer"}}}),
            json!({"a": 1}),
            json!({"a": "x"}),
        ),
        (
            "required",
            json!({"required": ["a"]}),
            json!({"a": 1}),
            json!({}),
        ),
        (
            "additionalProperties",
            json!({"properties": {"a": {}}, "additionalProperties": false}),
            json!({"a": 1}),
            json!({"b": 1}),
        ),
        ("enum", json!({"enum": ["x", "y"]}), json!("x"), json!("z")),
        ("const", json!({"const": 1}), json!(1), json!(2)),
        (
            "allOf",
            json!({"allOf": [{"type": "integer"}, {"minimum": 2}]}),
            json!(2),
            json!(1),
        ),
        (
            "if/then/else",
            json!({"if": {"const": 1}, "then": {"type": "integer"}, "else": {"type": "string"}}),
            json!("s"),
            json!(2.5),
        ),
        (
            "not",
            json!({"not": {"type": "string"}}),
            json!(1),
            json!("s"),
        ),
        (
            "items",
            json!({"items": {"type": "integer"}}),
            json!([1]),
            json!(["x"]),
        ),
        ("minItems", json!({"minItems": 1}), json!([1]), json!([])),
        (
            "contains/minContains/maxContains",
            json!({"contains": {"const": 1}, "minContains": 1, "maxContains": 1}),
            json!([0, 1]),
            json!([1, 1]),
        ),
        ("minimum", json!({"minimum": 1}), json!(1), json!(0)),
        ("maximum", json!({"maximum": 1}), json!(1), json!(2)),
        ("minLength", json!({"minLength": 1}), json!("a"), json!("")),
        (
            "maxLength",
            json!({"maxLength": 1}),
            json!("a"),
            json!("ab"),
        ),
        (
            "pattern",
            json!({"pattern": "^[a-z]+$"}),
            json!("abc"),
            json!("ABC"),
        ),
        (
            "format",
            json!({"type": "string", "format": "date-time"}),
            json!("2026-09-06T12:00:00Z"),
            json!("2026-09-06"),
        ),
        (
            "$ref/$defs",
            json!({"$defs": {"n": {"type": "integer"}}, "$ref": "#/$defs/n"}),
            json!(1),
            json!("x"),
        ),
    ];
    let mut covered: HashSet<&str> = ["$schema", "$id", "title", "description"]
        .into_iter()
        .collect();
    for (label, mut schema, valid, invalid) in cases {
        let object = schema.as_object_mut().unwrap();
        object.insert(
            "$schema".into(),
            json!("https://json-schema.org/draft/2020-12/schema"),
        );
        object.insert(
            "$id".into(),
            json!(format!(
                "{base}probe-{}.schema.json",
                label.replace('/', "-").replace('$', "")
            )),
        );
        object.insert("title".into(), json!(label));
        object.insert("description".into(), json!("keyword probe"));
        for key in label.split('/') {
            if key.starts_with("if") || key == "then" || key == "else" {
                covered.extend(["if", "then", "else"]);
            } else if key == "$ref" || key == "$defs" {
                covered.extend(["$ref", "$defs"]);
            } else {
                let owned: &str = SUPPORTED_KEYWORDS
                    .iter()
                    .copied()
                    .find(|k| *k == key)
                    .unwrap_or_else(|| panic!("{key} is not a frozen keyword"));
                covered.insert(owned);
            }
        }
        let validator = compile(&schema).unwrap_or_else(|e| panic!("{label}: {e}"));
        assert!(validator.is_valid(&valid), "{label}: positive case");
        assert!(!validator.is_valid(&invalid), "{label}: negative case");
    }
    let frozen: HashSet<&str> = SUPPORTED_KEYWORDS.iter().copied().collect();
    let missing: Vec<&&str> = frozen.iter().filter(|k| !covered.contains(**k)).collect();
    assert!(
        missing.is_empty(),
        "frozen keywords without coverage: {missing:?}"
    );
}
