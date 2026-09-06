//! M2 acceptance exercise: durable recovery and run inspection on real
//! SQLite files with real processes. A controller killed in the middle of a
//! claim transaction leaves nothing behind; claims, counters, and fencing
//! survive closing and reopening the file; waiting, failed, gated,
//! accepted, and cancelled runs replay to the same views after a restart;
//! and no accepted state exists without its receipt.
//!
//! When `CHECKSPAN_EVIDENCE_OUT` names a directory, the inspection test also
//! writes the views and row counts it observed there as JSON, for the
//! acceptance record.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use checkspan::budget::{Disposition, Narrowing, apply_retry_policy, budget_status};
use checkspan::contracts::{
    Attempt, Exactly, Execution, ExecutionOutcome, FailureClass, GatePacket, GraphRun, GraphSpec,
    Ident, NodeId, NodeStatus, OnExhaustion, RecordKind, ResultRef, RetryPolicy, RunId,
    SchemaVersion, Timestamp, VerifierReceipt, parse_record,
};
use checkspan::digests::artifact_digest;
use checkspan::scheduler::{Claim, ClaimOutcome, ControllerId, Scheduler, SchedulerError};
use checkspan::state::kinds;
use checkspan::store::{ClaimRow, Ledger, Store};
use common::fixture;

const NOW: &str = "2026-09-06T18:00:00Z";

fn ts(s: &str) -> Timestamp {
    Timestamp::new(s).unwrap()
}

fn node(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("checkspan-recovery-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn chain() -> GraphSpec {
    parse_record(&fixture("graphs/valid/chain-with-gate.json")).unwrap()
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

fn claimed(outcome: ClaimOutcome) -> Claim {
    match outcome {
        ClaimOutcome::Claimed(claim) => claim,
        other => panic!("expected a claim, got {other:?}"),
    }
}

fn finished(spec: &GraphSpec, claim: &Claim, outcome: ExecutionOutcome) -> Attempt {
    let node = spec.nodes.iter().find(|n| n.id == claim.node_id).unwrap();
    let result = (outcome == ExecutionOutcome::Completed).then(|| ResultRef {
        result_type: node.result_type.clone(),
        artifact_ref: format!("store:results/{}/{}", claim.node_id, claim.number),
        digest: artifact_digest(format!("{} result {}", claim.run_id, claim.number).as_bytes()),
    });
    Attempt {
        record: RecordKind::Attempt,
        schema_version: SchemaVersion(1),
        run_id: claim.run_id.clone(),
        node: spec.node_ref(&claim.node_id).unwrap(),
        number: claim.number,
        owner: claim.owner.as_str().to_string(),
        started_at: ts(NOW),
        finished_at: Some(ts("2026-09-06T18:05:00Z")),
        repair_hint: None,
        input_manifest: vec![],
        dependency_receipts: claim.dependency_receipts.clone(),
        result,
        produced_evidence: vec![],
        execution: Some(Execution {
            outcome,
            error_code: None,
            error_text: None,
        }),
        verifier_receipt: None,
    }
}

fn receipt(attempt: &Attempt, verdict: &str) -> VerifierReceipt {
    let name = if verdict == "accept" {
        "receipt-accept.json"
    } else {
        "receipt-reject.json"
    };
    let mut receipt: VerifierReceipt =
        parse_record(&fixture(&format!("outcomes/valid/{name}"))).unwrap();
    receipt.id = Ident::new(format!("rcpt-{}-{}", attempt.run_id, attempt.number)).unwrap();
    receipt.attempt = attempt.attempt_ref();
    receipt.result_digest = attempt.result.as_ref().unwrap().digest.clone();
    receipt
}

fn scheduler<'a>(spec: &'a GraphSpec, run: &'a GraphRun, who: &str) -> Scheduler<'a> {
    Scheduler::new(spec, run, ControllerId::new(who).unwrap()).unwrap()
}

/// Child body: hold an open claim transaction until killed. Runs only when
/// `CHECKSPAN_RECOVERY_STORE` is set; otherwise an immediate no-op.
#[test]
fn child_holds_a_claim_transaction_until_killed() {
    let Ok(store_path) = std::env::var("CHECKSPAN_RECOVERY_STORE") else {
        return;
    };
    let marker = PathBuf::from(std::env::var("CHECKSPAN_RECOVERY_MARKER").unwrap());
    let mut store = Store::open(&store_path).unwrap();
    let run_id = RunId::new("run-0001").unwrap();
    let node_id = node("cs_patch");
    let _never: Result<(), _> = store.transaction(|tx| {
        let fence = tx.next_fence(&run_id)?;
        tx.insert_claim(&ClaimRow {
            run_id: run_id.clone(),
            node_id: node_id.clone(),
            number: checkspan::contracts::AttemptNumber::new(1).unwrap(),
            owner: "doomed-controller".into(),
            fence,
            dependency_receipts: vec![],
            resources: vec![],
            active: true,
            release: None,
            claimed_at: String::new(),
            released_at: None,
        })?;
        tx.append_event(
            &run_id,
            Some(&node_id),
            kinds::ATTEMPT_DISPATCHED,
            &serde_json::json!({ "number": 1, "owner": "doomed-controller", "fence": fence }),
        )?;
        fs::write(&marker, b"in transaction").unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

#[test]
fn a_controller_killed_mid_transaction_leaves_no_partial_claim() {
    let spec = chain();
    let dir = temp_dir("kill");
    let path = dir.join("ledger.sqlite");
    let marker = dir.join("in-transaction");
    let run = run_record("run-0001", &spec);
    {
        let mut store = Store::open(&path).unwrap();
        store.store_graph(&spec).unwrap();
        store.create_run(&run).unwrap();
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "child_holds_a_claim_transaction_until_killed",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("CHECKSPAN_RECOVERY_STORE", &path)
        .env("CHECKSPAN_RECOVERY_MARKER", &marker)
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !marker.exists() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "child never reached the middle of its transaction"
        );
        assert!(child.try_wait().unwrap().is_none(), "child exited early");
        std::thread::sleep(Duration::from_millis(50));
    }
    child.kill().unwrap();
    child.wait().unwrap();

    let mut store = Store::open(&path).unwrap();
    assert!(
        store.active_claims().unwrap().is_empty(),
        "the uncommitted claim is gone"
    );
    assert!(store.claims(&run.run_id).unwrap().is_empty());
    assert_eq!(store.dispatch_count(&run.run_id).unwrap(), 0);
    let events = store.events(&run.run_id).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, kinds::RUN_CREATED);
    let views = scheduler(&spec, &run, "survivor")
        .views(&store, &ts(NOW))
        .unwrap();
    assert!(views.iter().all(|v| v.status == NodeStatus::Open));

    // The dead process's lock is gone: a surviving controller claims normally.
    let claim = claimed(
        scheduler(&spec, &run, "survivor")
            .claim_next(&mut store, &ts(NOW))
            .unwrap(),
    );
    assert_eq!(claim.node_id.as_str(), "cs_patch");
    assert_eq!(claim.fence, 1, "the doomed claim's token was never issued");
    assert_eq!(store.dispatch_count(&run.run_id).unwrap(), 1);
}

#[test]
fn claims_counters_and_fencing_survive_a_restart() {
    let spec = chain();
    let dir = temp_dir("restart");
    let path = dir.join("ledger.sqlite");
    let run = run_record("run-0001", &spec);
    let now = ts(NOW);
    let (claim, views_before) = {
        let mut store = Store::open(&path).unwrap();
        store.store_graph(&spec).unwrap();
        store.create_run(&run).unwrap();
        let a = scheduler(&spec, &run, "ctl-a");
        let claim = claimed(a.claim_next(&mut store, &now).unwrap());
        (claim, a.views(&store, &now).unwrap())
    };

    // "Restart": nothing but the file survives.
    let mut store = Store::open(&path).unwrap();
    let a = scheduler(&spec, &run, "ctl-a");
    assert_eq!(a.views(&store, &now).unwrap(), views_before);
    let row = store
        .claim(&run.run_id, &node("cs_patch"), 1)
        .unwrap()
        .unwrap();
    assert!(row.active);
    assert_eq!((row.fence, row.owner.as_str()), (1, "ctl-a"));
    assert_eq!(store.dispatch_count(&run.run_id).unwrap(), 1);
    assert!(
        matches!(
            a.claim_next(&mut store, &now).unwrap(),
            ClaimOutcome::Busy { active: 1, .. }
        ),
        "the persisted claim still counts"
    );

    // The old controller is presumed dead; another takes over.
    let b = scheduler(&spec, &run, "ctl-b");
    b.reclaim(&mut store, &node("cs_patch"), &now).unwrap();
    match a.complete(
        &mut store,
        &claim,
        &finished(&spec, &claim, ExecutionOutcome::Completed),
    ) {
        Err(SchedulerError::Stale {
            release: Some(r), ..
        }) => assert_eq!(r, "reclaimed"),
        other => panic!("{other:?}"),
    }
    assert!(
        store
            .attempt(&run.run_id, &node("cs_patch"), 1)
            .unwrap()
            .unwrap()
            .result
            .is_none()
    );

    // And again after another reopen: the refusal and the counters persist.
    drop(store);
    let mut store = Store::open(&path).unwrap();
    assert!(matches!(
        a.complete(
            &mut store,
            &claim,
            &finished(&spec, &claim, ExecutionOutcome::Completed)
        ),
        Err(SchedulerError::Stale { .. })
    ));
    assert_eq!(store.dispatch_count(&run.run_id).unwrap(), 1);
    assert_eq!(
        budget_status(&store, &spec, &run, &now).unwrap().consumed(),
        1
    );
    assert_eq!(a.views(&store, &now).unwrap()[0].status, NodeStatus::Failed);
    assert!(matches!(
        apply_retry_policy(
            &mut store,
            &spec,
            &run,
            &node("cs_patch"),
            &Narrowing::default(),
            &now
        )
        .unwrap(),
        Disposition::Retry {
            class: FailureClass::Infrastructure,
            started: 1,
            ..
        }
    ));
    let second = claimed(
        scheduler(&spec, &run, "ctl-c")
            .claim_next(&mut store, &now)
            .unwrap(),
    );
    assert_eq!(
        (second.number.get(), second.fence),
        (2, 2),
        "tokens continue, never reused"
    );
}

#[test]
fn waiting_failed_gated_accepted_and_cancelled_runs_replay_the_same_after_reopen() {
    let mut spec = chain();
    spec.nodes[0].retry_policy = RetryPolicy {
        version: Exactly,
        max_attempts: 3,
        allowed_failure_classes: vec![FailureClass::Infrastructure],
        on_exhaustion: OnExhaustion::Gate,
    };
    let dir = temp_dir("inspect");
    let path = dir.join("ledger.sqlite");
    let now = ts(NOW);
    let runs: Vec<GraphRun> = [
        "run-wait",
        "run-fail",
        "run-gate",
        "run-accept",
        "run-cancel",
    ]
    .iter()
    .map(|id| run_record(id, &spec))
    .collect();
    let patch = node("cs_patch");

    let before = {
        let mut store = Store::open(&path).unwrap();
        store.store_graph(&spec).unwrap();
        for run in &runs {
            store.create_run(run).unwrap();
        }
        // run-wait: nothing dispatched.
        // run-fail: one attempt timed out.
        let s = scheduler(&spec, &runs[1], "ctl");
        let c = claimed(s.claim_next(&mut store, &now).unwrap());
        s.complete(
            &mut store,
            &c,
            &finished(&spec, &c, ExecutionOutcome::TimedOut),
        )
        .unwrap();
        // run-gate: rejected, policy exhausts to a gate, packet opened.
        let s = scheduler(&spec, &runs[2], "ctl");
        let c = claimed(s.claim_next(&mut store, &now).unwrap());
        let attempt = finished(&spec, &c, ExecutionOutcome::Completed);
        s.complete(&mut store, &c, &attempt).unwrap();
        store.admit_receipt(&receipt(&attempt, "reject")).unwrap();
        let disposition = apply_retry_policy(
            &mut store,
            &spec,
            &runs[2],
            &patch,
            &Narrowing::default(),
            &now,
        )
        .unwrap();
        assert!(matches!(
            disposition,
            Disposition::Exhausted {
                route: OnExhaustion::Gate,
                ..
            }
        ));
        let mut packet: GatePacket =
            parse_record(&fixture("outcomes/valid/packet-resolve-work.json")).unwrap();
        packet.run_id = runs[2].run_id.clone();
        packet.node = spec.node_ref(&patch).unwrap();
        store.open_gate(&packet).unwrap();
        // run-accept: completed and accepted by receipt.
        let s = scheduler(&spec, &runs[3], "ctl");
        let c = claimed(s.claim_next(&mut store, &now).unwrap());
        let attempt = finished(&spec, &c, ExecutionOutcome::Completed);
        s.complete(&mut store, &c, &attempt).unwrap();
        store.admit_receipt(&receipt(&attempt, "accept")).unwrap();
        // run-cancel: claimed then cancelled.
        let s = scheduler(&spec, &runs[4], "ctl");
        claimed(s.claim_next(&mut store, &now).unwrap());
        s.cancel(&mut store, &patch).unwrap();
        snapshot(&store, &spec, &runs, &now)
    };

    let store = Store::open(&path).unwrap();
    let after = snapshot(&store, &spec, &runs, &now);
    assert_eq!(
        after, before,
        "replayed views, summaries, and counts are identical after reopening"
    );

    let expected_patch = [
        NodeStatus::Open,
        NodeStatus::Failed,
        NodeStatus::Gated,
        NodeStatus::Accepted,
        NodeStatus::Cancelled,
    ];
    for (run, expected) in runs.iter().zip(expected_patch) {
        let views = scheduler(&spec, run, "inspector")
            .views(&store, &now)
            .unwrap();
        assert_eq!(views[0].status, expected, "{}", run.run_id);
        assert_eq!(views[0].check_bindings(), vec![], "{}", run.run_id);
        assert_eq!(
            views[1].status,
            NodeStatus::Open,
            "{}: cs_ci waits",
            run.run_id
        );
        if expected == NodeStatus::Accepted {
            assert!(
                views[1].blocked_reason.is_none(),
                "cs_ci is ready once cs_patch is accepted"
            );
        } else {
            assert!(
                views[1]
                    .blocked_reason
                    .as_ref()
                    .unwrap()
                    .contains("cs_patch")
            );
        }
        if expected == NodeStatus::Accepted {
            let receipt = views[0]
                .receipt
                .as_ref()
                .expect("accepted only with a receipt");
            let stored = store
                .receipt(&run.run_id, receipt.id.as_str())
                .unwrap()
                .unwrap();
            assert_eq!(stored.verdict, checkspan::contracts::Verdict::Accept);
        } else {
            assert!(views[0].receipt.is_none() || expected == NodeStatus::Rejected);
        }
        if expected == NodeStatus::Gated {
            assert_eq!(
                views[0].waiting_on_gate.as_ref().unwrap().as_str(),
                "gate-0001"
            );
        }
    }
    let accepted_views = runs
        .iter()
        .flat_map(|run| {
            scheduler(&spec, run, "inspector")
                .views(&store, &now)
                .unwrap()
        })
        .filter(|v| v.status == NodeStatus::Accepted)
        .count();
    assert_eq!(
        accepted_views, 1,
        "exactly the receipt-backed node is accepted"
    );

    if let Ok(out) = std::env::var("CHECKSPAN_EVIDENCE_OUT") {
        let out = Path::new(&out);
        fs::create_dir_all(out).unwrap();
        fs::write(
            out.join("inspection.json"),
            serde_json::to_string_pretty(&after).unwrap(),
        )
        .unwrap();
        fs::write(
            out.join("retention.txt"),
            format!(
                "ledger bytes: {}\nrow counts: {:?}\n",
                fs::metadata(&path).unwrap().len(),
                store.row_counts().unwrap()
            ),
        )
        .unwrap();
    }
}

fn snapshot(
    store: &Store,
    spec: &GraphSpec,
    runs: &[GraphRun],
    now: &Timestamp,
) -> serde_json::Value {
    let mut per_run = serde_json::Map::new();
    for run in runs {
        let views = scheduler(spec, run, "inspector").views(store, now).unwrap();
        let summary: Vec<serde_json::Value> = store
            .summary(&run.run_id)
            .unwrap()
            .into_iter()
            .map(|s| {
                serde_json::json!({
                    "node": s.node_id, "attempts": s.attempts, "receipts": s.receipts, "last_verdict": s.last_verdict
                })
            })
            .collect();
        per_run.insert(
            run.run_id.as_str().to_string(),
            serde_json::json!({
                "views": views,
                "summary": summary,
                "dispatches": store.dispatch_count(&run.run_id).unwrap(),
                "active_claims": store.claims(&run.run_id).unwrap().iter().filter(|c| c.active).count(),
                "events": store.events(&run.run_id).unwrap().len(),
            }),
        );
    }
    serde_json::json!({
        "runs": per_run,
        "row_counts": store.row_counts().unwrap(),
    })
}
