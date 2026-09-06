//! `checkspan validate` and `checkspan inspect`: stable JSON output and exit
//! codes, and no side effects.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn checkspan(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_checkspan"))
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}

fn run(args: &[&str]) -> (i32, Value) {
    let out = checkspan(args, &root());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("{args:?}: stdout is not JSON ({e}): {stdout}"));
    (out.status.code().unwrap(), json)
}

fn stages(json: &Value) -> Vec<String> {
    json["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["stage"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn validate_valid_examples_exit_zero() {
    for example in [
        "graphs/review-pilot.json",
        "graphs/research-pilot.json",
        "records/graph-run.json",
    ] {
        let (code, json) = run(&["validate", &format!("examples/{example}")]);
        assert_eq!(code, 0, "{example}: {json}");
        assert_eq!(json["command"], "validate");
        assert_eq!(json["valid"], true);
        assert_eq!(json["path"], format!("examples/{example}"));
        assert_eq!(json["schema_version"], 1);
        assert!(
            json["schema_id"]
                .as_str()
                .unwrap()
                .starts_with("https://checkspan.invalid/")
        );
        assert_eq!(json["diagnostics"], Value::Array(vec![]));
    }
    let (_, json) = run(&["validate", "examples/graphs/review-pilot.json"]);
    assert_eq!(json["record"], "graph_spec");
    let (_, json) = run(&["validate", "examples/records/graph-run.json"]);
    assert_eq!(json["record"], "graph_run");
}

#[test]
fn validate_invalid_examples_exit_one_with_the_rejecting_stage() {
    let expected = [
        ("cycle", "admission", "dependency cycle"),
        ("unresolved-dependency", "admission", "not in this graph"),
        ("incompatible-type", "admission", "which produces"),
        (
            "hidden-proof-dependency",
            "admission",
            "without declaring a dependency",
        ),
        (
            "gate-wait-cycle",
            "admission",
            "cannot wait on the node it resolves",
        ),
        (
            "external-import",
            "admission",
            "cross-graph imports are not supported",
        ),
        ("duplicate-key", "duplicate_key", "more than once"),
        ("unknown-version", "header", "not supported"),
        ("bad-identifier", "schema", "does not match"),
    ];
    let mut names: Vec<String> = fs::read_dir(root().join("examples/invalid"))
        .unwrap()
        .map(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .trim_end_matches(".json")
                .to_string()
        })
        .collect();
    names.sort();
    let mut listed: Vec<&str> = expected.iter().map(|(n, _, _)| *n).collect();
    listed.sort_unstable();
    assert_eq!(names, listed, "every invalid example has an expectation");
    for (name, stage, needle) in expected {
        let (code, json) = run(&["validate", &format!("examples/invalid/{name}.json")]);
        assert_eq!(code, 1, "{name}: {json}");
        assert_eq!(json["valid"], false, "{name}");
        let stages = stages(&json);
        assert!(!stages.is_empty(), "{name}");
        assert!(stages.iter().all(|s| s == stage), "{name}: {stages:?}");
        let messages: Vec<&str> = json["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["message"].as_str().unwrap())
            .collect();
        assert!(
            messages.iter().any(|m| m.contains(needle)),
            "{name}: {messages:?}"
        );
        for d in json["diagnostics"].as_array().unwrap() {
            assert!(d["path"].is_string(), "{name}: every diagnostic has a path");
        }
    }
}

#[test]
fn unreadable_file_exits_three() {
    let (code, json) = run(&["validate", "examples/does-not-exist.json"]);
    assert_eq!(code, 3);
    assert_eq!(json["valid"], false);
    assert_eq!(stages(&json), ["io"]);
    let (code, _) = run(&["inspect", "examples/does-not-exist.json"]);
    assert_eq!(code, 3);
}

#[test]
fn inspect_describes_order_closure_and_gates() {
    let (code, json) = run(&["inspect", "examples/graphs/review-pilot.json"]);
    assert_eq!(code, 0, "{json}");
    assert_eq!(json["command"], "inspect");
    assert_eq!(json["valid"], true);
    let graph = &json["graph"];
    assert_eq!(graph["graph_id"], "review_pilot");
    assert_eq!(
        graph["order"],
        serde_json::json!(["cs_patch", "cs_ci", "cs_ci_gate", "cs_packet"])
    );
    assert_eq!(
        graph["required"],
        serde_json::json!(["cs_patch", "cs_ci", "cs_packet"])
    );
    assert_eq!(graph["optional"], serde_json::json!(["cs_ci_gate"]));
    assert_eq!(graph["targets"], serde_json::json!(["cs_packet"]));
    assert_eq!(graph["gates"][0]["gate"], "cs_ci_gate");
    assert_eq!(graph["gates"][0]["resolves"], serde_json::json!(["cs_ci"]));
    let nodes = graph["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 4);
    assert_eq!(nodes[1]["id"], "cs_ci");
    assert_eq!(nodes[1]["kind"], "check");
    assert_eq!(nodes[1]["deps"][0]["node_id"], "cs_patch");
    assert_eq!(nodes[1]["deps"][0]["expected_type"], "patch_result@1");
    assert_eq!(
        nodes[1]["required_checks"],
        serde_json::json!(["fmt", "clippy", "test"])
    );
    assert_eq!(nodes[1]["max_attempts"], 3);

    let (code, json) = run(&["inspect", "examples/graphs/research-pilot.json"]);
    assert_eq!(code, 0);
    assert_eq!(json["graph"]["order"].as_array().unwrap().len(), 6);
    assert_eq!(json["graph"]["optional"], serde_json::json!([]));
}

#[test]
fn inspect_on_a_non_graph_record_reports_its_identity() {
    let (code, json) = run(&["inspect", "examples/records/graph-run.json"]);
    assert_eq!(code, 0);
    assert_eq!(json["record"], "graph_run");
    assert_eq!(json["identity"]["run_id"], "run-0001");
    assert!(json.get("graph").is_none());
    let (code, json) = run(&[
        "inspect",
        "tests/fixtures/outcomes/valid/receipt-accept.json",
    ]);
    assert_eq!(code, 0);
    assert_eq!(json["identity"]["verdict"], "accept");
    assert_eq!(json["identity"]["provenance_level"], "local_controller");
}

#[test]
fn inspect_on_an_invalid_document_exits_one_with_diagnostics() {
    let (code, json) = run(&["inspect", "examples/invalid/cycle.json"]);
    assert_eq!(code, 1);
    assert_eq!(json["command"], "inspect");
    assert_eq!(json["valid"], false);
    assert!(json.get("graph").is_none());
    assert_eq!(stages(&json), ["admission"]);
}

#[test]
fn commands_create_no_files_and_leave_the_input_untouched() {
    let dir = std::env::temp_dir().join(format!("checkspan-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let source = root().join("examples/graphs/review-pilot.json");
    let copy = dir.join("graph.json");
    fs::copy(&source, &copy).unwrap();
    let before = fs::read(&copy).unwrap();
    for args in [["validate", "graph.json"], ["inspect", "graph.json"]] {
        let out = checkspan(&args, &dir);
        assert!(out.status.success(), "{args:?}");
    }
    let entries: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        entries,
        ["graph.json"],
        "no run store or scratch file was created"
    );
    assert_eq!(fs::read(&copy).unwrap(), before, "the input is unchanged");
    assert!(!root().join(".checkspan").exists());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn output_is_stable_across_runs_and_carries_no_stderr_on_success() {
    let first = checkspan(&["inspect", "examples/graphs/research-pilot.json"], &root());
    let second = checkspan(&["inspect", "examples/graphs/research-pilot.json"], &root());
    assert_eq!(first.stdout, second.stdout);
    assert!(first.stderr.is_empty());
    assert!(first.stdout.ends_with(b"}\n"));
}

#[test]
fn no_subcommand_prints_help_and_exits_two() {
    let out = checkspan(&[], &root());
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8(out.stderr).unwrap();
    assert!(text.contains("validate"), "{text}");
    assert!(text.contains("inspect"), "{text}");
    let out = checkspan(&["validate"], &root());
    assert_eq!(out.status.code(), Some(2));
}
