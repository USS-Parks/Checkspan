//! Transactional run storage against real SQLite files: revisions are
//! immutable, runs need their graph, acceptance cannot exist without its
//! receipt row, a failing transaction leaves the prior state intact, and a
//! newer schema is refused without destructive repair.

mod common;

use std::fs;
use std::path::PathBuf;

use checkspan::contracts::{
    Attempt, GateDecision, GatePacket, GraphRun, GraphSpec, NodeId, RunId, VerifierReceipt,
    parse_record,
};
use checkspan::store::{ArtifactRef, MAX_LOCATOR_LEN, SCHEMA_VERSION, Store, StoreError};
use common::fixture;
use serde_json::json;

fn temp_store(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("checkspan-store-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.join("ledger.sqlite")
}

fn graph() -> GraphSpec {
    parse_record(&fixture("contracts/valid/graph-spec-full.json")).unwrap()
}

fn run_for(graph: &GraphSpec, run_id: &str) -> GraphRun {
    GraphRun {
        record: checkspan::contracts::RecordKind::GraphRun,
        schema_version: checkspan::contracts::SchemaVersion(1),
        run_id: RunId::new(run_id).unwrap(),
        graph_ref: graph.graph_ref(),
        budget_lineage_ref: None,
        admitted_imports: vec![],
    }
}

fn attempt(name: &str) -> Attempt {
    parse_record(&fixture(&format!("outcomes/valid/{name}"))).unwrap()
}

fn receipt(name: &str) -> VerifierReceipt {
    parse_record(&fixture(&format!("outcomes/valid/{name}"))).unwrap()
}

/// A store holding the full graph at revision 2 and run `run-0001` of it.
fn seeded(name: &str) -> (Store, GraphSpec) {
    let mut store = Store::open(temp_store(name)).unwrap();
    let graph = graph();
    store.store_graph(&graph).unwrap();
    store.create_run(&run_for(&graph, "run-0001")).unwrap();
    (store, graph)
}

#[test]
fn open_creates_a_versioned_wal_store_and_reopens_it() {
    let path = temp_store("open");
    let store = Store::open(&path).unwrap();
    assert!(path.exists());
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    assert_eq!(store.journal_mode().unwrap(), "wal");
    assert!(store.row_counts().unwrap().iter().all(|(_, n)| *n == 0));
    drop(store);
    let again = Store::open(&path).unwrap();
    assert_eq!(again.schema_version().unwrap(), SCHEMA_VERSION);
    assert_eq!(again.path(), path);
}

#[test]
fn graph_revisions_are_bound_to_their_digest_and_immutable() {
    let mut store = Store::open(temp_store("graphs")).unwrap();
    let graph = graph();
    let first = store.store_graph(&graph).unwrap();
    assert!(first.inserted);
    assert!(first.digest.starts_with("sha256:"));
    let second = store.store_graph(&graph).unwrap();
    assert!(!second.inserted, "identical content is a no-op");
    assert_eq!(second.digest, first.digest);

    let mut edited = graph.clone();
    edited.nodes[0].prompt.push_str(" changed");
    match store.store_graph(&edited) {
        Err(StoreError::RevisionConflict {
            graph_ref,
            stored,
            offered,
        }) => {
            assert_eq!(graph_ref, graph.graph_ref());
            assert_eq!(stored, first.digest);
            assert_ne!(offered, stored);
        }
        other => panic!("{other:?}"),
    }
    let loaded = store.load_graph(&graph.graph_ref()).unwrap().unwrap();
    assert_eq!(loaded, graph, "the original content is what is stored");

    let mut next = edited.clone();
    next.revision = checkspan::contracts::Revision::new(3).unwrap();
    next.supersedes = Some(graph.graph_ref());
    assert!(
        store.store_graph(&next).unwrap().inserted,
        "a new revision is fine"
    );
    assert_eq!(store.row_counts().unwrap()[0], ("graphs".to_string(), 2));
}

#[test]
fn runs_need_their_graph_and_are_created_with_their_event() {
    let mut store = Store::open(temp_store("runs")).unwrap();
    let graph = graph();
    let run = run_for(&graph, "run-0001");
    assert!(matches!(
        store.create_run(&run),
        Err(StoreError::MissingReference(what)) if what.contains("review_pilot@2")
    ));
    store.store_graph(&graph).unwrap();
    store.create_run(&run).unwrap();
    assert!(matches!(
        store.create_run(&run),
        Err(StoreError::Duplicate(_))
    ));
    assert_eq!(store.load_run(&run.run_id).unwrap().unwrap(), run);
    let events = store.events(&run.run_id).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "run_created");
    assert_eq!(events[0].seq, 1);
    assert!(events[0].node_id.is_none());
    assert_eq!(events[0].payload["revision"], 2);
    assert!(events[0].recorded_at.ends_with('Z'));
}

#[test]
fn sealing_and_admission_write_record_and_event_together() {
    let (mut store, _) = seeded("admit");
    let run_id = RunId::new("run-0001").unwrap();
    let ci = NodeId::new("cs_ci").unwrap();
    let sealed = attempt("attempt-completed-with-receipt.json");
    store.seal_attempt(&sealed).unwrap();
    assert!(matches!(
        store.seal_attempt(&sealed),
        Err(StoreError::Duplicate(what)) if what.contains("cs_ci")
    ));
    let accept = receipt("receipt-accept.json");
    store.admit_receipt(&accept).unwrap();
    assert!(matches!(
        store.admit_receipt(&accept),
        Err(StoreError::Duplicate(what)) if what.contains("rcpt-ci-1")
    ));

    let events = store.events(&run_id).unwrap();
    assert_eq!(
        events.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(),
        ["run_created", "attempt_sealed", "receipt_admitted"]
    );
    assert_eq!(events[2].payload["verdict"], "accept");
    assert_eq!(events[2].payload["receipt_id"], "rcpt-ci-1");
    assert_eq!(events[2].node_id.as_ref().unwrap(), &ci);
    assert_eq!(store.attempts(&run_id, &ci).unwrap(), vec![sealed]);
    assert_eq!(
        store.receipt(&run_id, "rcpt-ci-1").unwrap().unwrap(),
        accept
    );
    let summary = store.summary(&run_id).unwrap();
    assert_eq!(summary.len(), 1);
    assert_eq!(summary[0].node_id, ci);
    assert_eq!((summary[0].attempts, summary[0].receipts), (1, 1));
    assert_eq!(summary[0].last_verdict.as_deref(), Some("accept"));
}

#[test]
fn a_receipt_needs_its_sealed_attempt_and_an_attempt_needs_its_run() {
    let (mut store, _) = seeded("refs");
    let accept = receipt("receipt-accept.json");
    assert!(matches!(
        store.admit_receipt(&accept),
        Err(StoreError::MissingReference(what)) if what.contains("cs_ci@2#1")
    ));
    let mut foreign = attempt("attempt-failed.json");
    foreign.run_id = RunId::new("run-9999").unwrap();
    assert!(matches!(
        store.seal_attempt(&foreign),
        Err(StoreError::MissingReference(what)) if what.contains("run-9999")
    ));
    let run_id = RunId::new("run-0001").unwrap();
    assert_eq!(
        store.events(&run_id).unwrap().len(),
        1,
        "nothing else was recorded"
    );
}

#[test]
fn acceptance_cannot_be_recorded_without_its_receipt_row() {
    let (mut store, _) = seeded("trigger");
    let run_id = RunId::new("run-0001").unwrap();
    store
        .seal_attempt(&attempt("attempt-completed-with-receipt.json"))
        .unwrap();
    let result = store.transaction(|tx| {
        tx.append_event(
            &run_id,
            Some(&NodeId::new("cs_ci").unwrap()),
            "receipt_admitted",
            &json!({"receipt_id": "rcpt-forged", "verdict": "accept"}),
        )
    });
    assert!(
        result.is_err(),
        "the trigger refuses an event without its receipt row"
    );
    let events = store.events(&run_id).unwrap();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e.kind != "receipt_admitted"));
    assert!(store.summary(&run_id).unwrap()[0].last_verdict.is_none());
}

#[test]
fn a_failing_transaction_rolls_back_every_write_and_keeps_prior_state() {
    let (mut store, _) = seeded("rollback");
    let run_id = RunId::new("run-0001").unwrap();
    let mut prior = attempt("attempt-failed.json");
    prior.number = checkspan::contracts::AttemptNumber::new(2).unwrap();
    store.seal_attempt(&prior).unwrap();
    let before = store.row_counts().unwrap();
    let before_events = store.events(&run_id).unwrap();

    let sealed = attempt("attempt-completed-with-receipt.json");
    let accept = receipt("receipt-accept.json");
    let outcome: Result<(), StoreError> = store.transaction(|tx| {
        tx.append_event(&run_id, None, "note", &json!({"n": 1}))?;
        tx.insert_attempt(&sealed)?;
        tx.insert_receipt(&accept)?;
        tx.insert_artifact(
            &run_id,
            &ArtifactRef {
                digest: accept.result_digest.as_str().to_string(),
                kind: "software_check_result".into(),
                locator: "store:results/run-0001/cs_ci/1".into(),
                size_bytes: 1024,
            },
        )?;
        assert_eq!(tx.count("receipts")?, 1, "visible inside the transaction");
        Err(StoreError::Corrupt(
            "simulated failure after several writes".into(),
        ))
    });
    assert!(matches!(outcome, Err(StoreError::Corrupt(_))));
    assert_eq!(store.row_counts().unwrap(), before, "nothing persisted");
    assert_eq!(store.events(&run_id).unwrap(), before_events);
    assert!(store.receipt(&run_id, "rcpt-ci-1").unwrap().is_none());
    assert_eq!(
        store
            .attempts(&run_id, &NodeId::new("cs_ci").unwrap())
            .unwrap()
            .len(),
        1,
        "the earlier sealed attempt is intact"
    );
    // The same writes commit when the transaction succeeds.
    store
        .transaction(|tx| {
            tx.insert_attempt(&sealed)?;
            tx.insert_receipt(&accept)?;
            tx.append_event(
                &run_id,
                Some(&NodeId::new("cs_ci").unwrap()),
                "receipt_admitted",
                &json!({"receipt_id": "rcpt-ci-1", "verdict": "accept"}),
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        store.events(&run_id).unwrap().len(),
        before_events.len() + 1
    );
}

#[test]
fn sealed_records_and_events_cannot_be_updated_or_deleted() {
    let (mut store, graph) = seeded("immutable");
    let run_id = RunId::new("run-0001").unwrap();
    store.seal_attempt(&attempt("attempt-failed.json")).unwrap();
    let conn = rusqlite::Connection::open(store.path()).unwrap();
    for sql in [
        "UPDATE graphs SET spec_json = '{}'",
        "UPDATE attempts SET attempt_json = '{}'",
        "UPDATE events SET kind = 'receipt_admitted'",
        "DELETE FROM events",
    ] {
        let err = conn.execute(sql, []).unwrap_err().to_string();
        assert!(
            err.contains("immutable") || err.contains("never deleted"),
            "{sql}: {err}"
        );
    }
    assert_eq!(
        store.load_graph(&graph.graph_ref()).unwrap().unwrap(),
        graph
    );
    assert_eq!(store.events(&run_id).unwrap().len(), 2);
}

#[test]
fn gates_and_decisions_are_recorded_with_their_events_once() {
    let (mut store, _) = seeded("gates");
    let run_id = RunId::new("run-0001").unwrap();
    let packet: GatePacket =
        parse_record(&fixture("outcomes/valid/packet-resolve-work.json")).unwrap();
    let decision: GateDecision =
        parse_record(&fixture("outcomes/valid/decision-retry.json")).unwrap();
    assert!(matches!(
        store.admit_decision(&run_id, &decision),
        Err(StoreError::MissingReference(what)) if what.contains("gate-0001")
    ));
    store.open_gate(&packet).unwrap();
    assert!(matches!(
        store.open_gate(&packet),
        Err(StoreError::Duplicate(_))
    ));
    store.admit_decision(&run_id, &decision).unwrap();
    let cancel: GateDecision =
        parse_record(&fixture("outcomes/valid/decision-cancel.json")).unwrap();
    assert!(
        matches!(
            store.admit_decision(&run_id, &cancel),
            Err(StoreError::Duplicate(_))
        ),
        "one decision per packet"
    );
    let kinds: Vec<String> = store
        .events(&run_id)
        .unwrap()
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(kinds, ["run_created", "gate_opened", "decision_admitted"]);
}

#[test]
fn artifact_references_are_bounded() {
    let (mut store, _) = seeded("artifacts");
    let run_id = RunId::new("run-0001").unwrap();
    let long = "x".repeat(MAX_LOCATOR_LEN + 1);
    let err = store
        .transaction(|tx| {
            tx.insert_artifact(
                &run_id,
                &ArtifactRef {
                    digest: "sha256:00".into(),
                    kind: "blob".into(),
                    locator: long.clone(),
                    size_bytes: 1,
                },
            )
        })
        .unwrap_err();
    assert!(matches!(
        err,
        StoreError::TooLarge {
            what: "artifact locator",
            ..
        }
    ));
    store
        .transaction(|tx| {
            tx.insert_artifact(
                &run_id,
                &ArtifactRef {
                    digest: "sha256:00".into(),
                    kind: "blob".into(),
                    locator: "x".repeat(MAX_LOCATOR_LEN),
                    size_bytes: 1,
                },
            )
        })
        .unwrap();
    assert_eq!(
        store.row_counts().unwrap().last().unwrap(),
        &("artifacts".to_string(), 1)
    );
}

#[test]
fn a_newer_schema_is_refused_without_touching_the_file() {
    let path = temp_store("newer");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 7).unwrap();
        conn.execute("CREATE TABLE future (x INTEGER)", []).unwrap();
        conn.execute("INSERT INTO future VALUES (42)", []).unwrap();
    }
    let before = fs::read(&path).unwrap();
    match Store::open(&path) {
        Err(StoreError::UnsupportedSchema { found, supported }) => {
            assert_eq!(found, 7);
            assert_eq!(supported, SCHEMA_VERSION);
        }
        other => panic!("{:?}", other.map(|s| s.schema_version())),
    }
    assert_eq!(
        fs::read(&path).unwrap(),
        before,
        "the file is byte-identical"
    );
    let conn = rusqlite::Connection::open(&path).unwrap();
    let x: i64 = conn
        .query_row("SELECT x FROM future", [], |r| r.get(0))
        .unwrap();
    assert_eq!(x, 42);
    let tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'events'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0, "no Checkspan tables were created");
}

#[test]
fn a_non_sqlite_file_is_refused() {
    let path = temp_store("garbage");
    fs::write(&path, b"this is not a database").unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"this is not a database");
}
