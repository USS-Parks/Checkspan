//! Exclusive scheduling and completion ownership: deterministic ready
//! order, one active claim by default, two real controller processes cannot
//! claim one attempt, cancellation and reclaim invalidate old completions,
//! and independent nodes sharing an exclusive resource serialize.

mod common;

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use checkspan::contracts::{
    Attempt, AttemptNumber, Execution, ExecutionOutcome, GraphRun, GraphSpec, NodeId, NodeStatus,
    RecordKind, ResultRef, RunId, SchemaVersion, Timestamp, parse_record,
};
use checkspan::digests::artifact_digest;
use checkspan::scheduler::{
    Claim, ClaimOutcome, ControllerId, RECLAIMED_ERROR_CODE, Scheduler, SchedulerError,
};
use checkspan::state::kinds;
use checkspan::store::{Ledger, Store};
use common::fixture;

const NOW: &str = "2026-09-06T16:00:00Z";

fn ts(s: &str) -> Timestamp {
    Timestamp::new(s).unwrap()
}

fn node(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}

fn controller(s: &str) -> ControllerId {
    ControllerId::new(s).unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("checkspan-claims-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
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

fn graph(name: &str) -> GraphSpec {
    parse_record(&fixture(name)).unwrap()
}

/// A store with `spec` stored and `run-0001` created.
fn seeded(dir: &std::path::Path, spec: &GraphSpec) -> (Store, GraphRun) {
    let mut store = Store::open(dir.join("ledger.sqlite")).unwrap();
    store.store_graph(spec).unwrap();
    let run = run_record("run-0001", spec);
    store.create_run(&run).unwrap();
    (store, run)
}

/// A completed attempt record for `claim`, producing the node's declared
/// result type with `bytes` as the artifact.
fn completed(spec: &GraphSpec, claim: &Claim, bytes: &[u8]) -> Attempt {
    let node = spec.nodes.iter().find(|n| n.id == claim.node_id).unwrap();
    Attempt {
        record: RecordKind::Attempt,
        schema_version: SchemaVersion(1),
        run_id: claim.run_id.clone(),
        node: spec.node_ref(&claim.node_id).unwrap(),
        number: claim.number,
        owner: claim.owner.as_str().to_string(),
        started_at: ts(NOW),
        finished_at: Some(ts("2026-09-06T16:05:00Z")),
        repair_hint: None,
        input_manifest: vec![],
        dependency_receipts: claim.dependency_receipts.clone(),
        result: Some(ResultRef {
            result_type: node.result_type.clone(),
            artifact_ref: format!(
                "store:results/{}/{}/{}",
                claim.run_id, claim.node_id, claim.number
            ),
            digest: artifact_digest(bytes),
        }),
        produced_evidence: vec![],
        execution: Some(Execution {
            outcome: ExecutionOutcome::Completed,
            error_code: None,
            error_text: None,
        }),
        verifier_receipt: None,
    }
}

fn claimed(outcome: ClaimOutcome) -> Claim {
    match outcome {
        ClaimOutcome::Claimed(claim) => claim,
        other => panic!("expected a claim, got {other:?}"),
    }
}

#[test]
fn claims_follow_the_admitted_order_and_the_default_cap_is_one() {
    let spec = graph("graphs/valid/diamond.json");
    let (mut store, run) = seeded(&temp_dir("order"), &spec);
    let scheduler = Scheduler::new(&spec, &run, controller("ctl-a")).unwrap();
    let now = ts(NOW);

    let first = claimed(scheduler.claim_next(&mut store, &now).unwrap());
    assert_eq!(first.node_id.as_str(), "cs_requirements");
    assert_eq!(first.number.get(), 1);
    assert_eq!(first.fence, 1);
    assert_eq!(first.owner.as_str(), "ctl-a");
    assert!(first.dependency_receipts.is_empty());
    assert!(matches!(
        scheduler.claim_next(&mut store, &now).unwrap(),
        ClaimOutcome::Busy {
            active: 1,
            max_active: 1
        }
    ));
    let claims = store.claims(&run.run_id).unwrap();
    assert_eq!(claims.len(), 1);
    assert!(claims[0].active);
    let events = store.events(&run.run_id).unwrap();
    let dispatched = events.last().unwrap();
    assert_eq!(dispatched.kind, kinds::ATTEMPT_DISPATCHED);
    assert_eq!(dispatched.payload["owner"], "ctl-a");
    assert_eq!(dispatched.payload["fence"], 1);

    scheduler
        .complete(
            &mut store,
            &first,
            &completed(&spec, &first, b"requirements"),
        )
        .unwrap();
    assert!(store.active_claims().unwrap().is_empty());
    let second = claimed(scheduler.claim_next(&mut store, &now).unwrap());
    assert_eq!(second.node_id.as_str(), "cs_implementation");
    assert_eq!(second.fence, 2);
    scheduler
        .complete(
            &mut store,
            &second,
            &completed(&spec, &second, b"implementation"),
        )
        .unwrap();

    // Synthesis needs both retrieval nodes *accepted*, not just sealed.
    match scheduler.claim_next(&mut store, &now).unwrap() {
        ClaimOutcome::NothingReady { blocked, conflicts } => {
            assert!(conflicts.is_empty());
            let synthesis = blocked
                .iter()
                .find(|(n, _)| n.as_str() == "cs_synthesis")
                .unwrap();
            assert!(synthesis.1.contains("cs_requirements"), "{}", synthesis.1);
            assert!(synthesis.1.contains("running"), "{}", synthesis.1);
        }
        other => panic!("{other:?}"),
    }
    let views = scheduler.views(&store, &now).unwrap();
    assert_eq!(views[0].status, NodeStatus::Running, "sealed but unchecked");
    assert!(views[0].blocked_reason.is_none());
    assert_eq!(views[2].status, NodeStatus::Open);
    assert!(
        views[2]
            .blocked_reason
            .as_ref()
            .unwrap()
            .contains("cs_implementation")
    );
    let fences: Vec<u64> = store
        .claims(&run.run_id)
        .unwrap()
        .iter()
        .map(|c| c.fence)
        .collect();
    assert_eq!(
        fences,
        [1, 2],
        "fencing tokens increase and are never reused"
    );
}

#[test]
fn a_completion_after_cancellation_is_refused_and_writes_nothing() {
    let spec = graph("graphs/valid/chain-with-gate.json");
    let (mut store, run) = seeded(&temp_dir("cancel"), &spec);
    let scheduler = Scheduler::new(&spec, &run, controller("ctl-a")).unwrap();
    let now = ts(NOW);
    let claim = claimed(scheduler.claim_next(&mut store, &now).unwrap());
    assert_eq!(claim.node_id.as_str(), "cs_patch");

    scheduler.cancel(&mut store, &node("cs_patch")).unwrap();
    let before = store.events(&run.run_id).unwrap();
    let attempt = completed(&spec, &claim, b"late");
    match scheduler.complete(&mut store, &claim, &attempt) {
        Err(SchedulerError::Stale {
            held: 1,
            current: 1,
            release: Some(r),
            ..
        }) => {
            assert_eq!(r, "cancelled")
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        store.events(&run.run_id).unwrap(),
        before,
        "nothing written"
    );
    assert!(
        store
            .attempt(&run.run_id, &node("cs_patch"), 1)
            .unwrap()
            .is_none()
    );
    let views = scheduler.views(&store, &now).unwrap();
    assert_eq!(views[0].status, NodeStatus::Cancelled);
    assert!(store.active_claims().unwrap().is_empty());
    assert!(
        matches!(
            scheduler.cancel(&mut store, &node("cs_patch")),
            Err(SchedulerError::Transition(_))
        ),
        "cancelling twice is illegal"
    );
}

#[test]
fn a_reclaim_by_another_controller_invalidates_the_old_owners_completion() {
    let spec = graph("graphs/valid/chain-with-gate.json");
    let (mut store, run) = seeded(&temp_dir("reclaim"), &spec);
    let now = ts(NOW);
    let a = Scheduler::new(&spec, &run, controller("ctl-a")).unwrap();
    let b = Scheduler::new(&spec, &run, controller("ctl-b")).unwrap();
    let claim = claimed(a.claim_next(&mut store, &now).unwrap());
    assert!(
        matches!(
            b.reclaim(&mut store, &node("cs_ci"), &now),
            Err(SchedulerError::Invalid(_))
        ),
        "nothing to reclaim on an unclaimed node"
    );

    let number = b
        .reclaim(&mut store, &node("cs_patch"), &ts("2026-09-06T17:00:00Z"))
        .unwrap();
    assert_eq!(number.get(), 1);
    let sealed = store
        .attempt(&run.run_id, &node("cs_patch"), 1)
        .unwrap()
        .unwrap();
    let execution = sealed.execution.unwrap();
    assert_eq!(execution.outcome, ExecutionOutcome::Failed);
    assert_eq!(execution.error_code.unwrap().as_str(), RECLAIMED_ERROR_CODE);
    assert_eq!(
        sealed.owner, "ctl-a",
        "the record names the owner whose claim was taken"
    );
    assert!(sealed.result.is_none());
    let row = store
        .claim(&run.run_id, &node("cs_patch"), 1)
        .unwrap()
        .unwrap();
    assert!(!row.active);
    assert_eq!(row.release.as_deref(), Some("reclaimed"));

    let before = store.events(&run.run_id).unwrap();
    match a.complete(&mut store, &claim, &completed(&spec, &claim, b"too late")) {
        Err(SchedulerError::Stale {
            release: Some(r), ..
        }) => assert_eq!(r, "reclaimed"),
        other => panic!("{other:?}"),
    }
    assert_eq!(store.events(&run.run_id).unwrap(), before);
    assert_eq!(a.views(&store, &now).unwrap()[0].status, NodeStatus::Failed);
    assert!(store.claims(&run.run_id).unwrap().iter().all(|c| !c.active));
}

#[test]
fn duplicate_or_mismatched_completions_are_refused() {
    let spec = graph("graphs/valid/chain-with-gate.json");
    let (mut store, run) = seeded(&temp_dir("duplicate"), &spec);
    let scheduler = Scheduler::new(&spec, &run, controller("ctl-a")).unwrap();
    let now = ts(NOW);
    let claim = claimed(scheduler.claim_next(&mut store, &now).unwrap());
    let attempt = completed(&spec, &claim, b"patch");

    let mut other_number = attempt.clone();
    other_number.number = AttemptNumber::new(2).unwrap();
    assert!(matches!(
        scheduler.complete(&mut store, &claim, &other_number),
        Err(SchedulerError::AttemptMismatch(_))
    ));
    let mut no_execution = attempt.clone();
    no_execution.execution = None;
    no_execution.finished_at = None;
    assert!(matches!(
        scheduler.complete(&mut store, &claim, &no_execution),
        Err(SchedulerError::AttemptMismatch(_))
    ));
    let mut forged = claim.clone();
    forged.fence = 99;
    assert!(matches!(
        scheduler.complete(&mut store, &forged, &attempt),
        Err(SchedulerError::Stale {
            held: 99,
            current: 1,
            release: None,
            ..
        })
    ));
    let mut other_owner = claim.clone();
    other_owner.owner = controller("ctl-z");
    assert!(matches!(
        scheduler.complete(&mut store, &other_owner, &attempt),
        Err(SchedulerError::Stale { .. })
    ));
    let mut wrong_snapshot = attempt.clone();
    wrong_snapshot.dependency_receipts = vec![checkspan::contracts::ReceiptRef {
        id: checkspan::contracts::Ident::new("rcpt-ghost").unwrap(),
        run_id: run.run_id.clone(),
        node: spec.node_ref(&node("cs_ci")).unwrap(),
    }];
    assert!(matches!(
        scheduler.complete(&mut store, &claim, &wrong_snapshot),
        Err(SchedulerError::SnapshotMismatch)
    ));
    assert!(
        store
            .attempt(&run.run_id, &node("cs_patch"), 1)
            .unwrap()
            .is_none(),
        "nothing sealed yet"
    );

    scheduler.complete(&mut store, &claim, &attempt).unwrap();
    assert!(
        matches!(
            scheduler.complete(&mut store, &claim, &attempt),
            Err(SchedulerError::Stale { release: Some(r), .. }) if r == "completed"
        ),
        "a second completion is stale"
    );
    assert_eq!(
        store
            .attempts(&run.run_id, &node("cs_patch"))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn independent_nodes_sharing_an_exclusive_resource_serialize() {
    // Two independent tasks both claim the checkout exclusively; a third
    // shares a model endpoint. Raise the cap so only resources decide.
    let mut spec = graph("graphs/valid/isolated-target.json");
    let checkout = |access: &str| serde_json::json!({ "resource": "checkout", "access": access });
    for (i, access) in [(0, "exclusive"), (1, "exclusive")] {
        spec.nodes[i].resource_scope = vec![serde_json::from_value(checkout(access)).unwrap()];
    }
    let (mut store, run) = seeded(&temp_dir("resources"), &spec);
    let scheduler = Scheduler::new(&spec, &run, controller("ctl-a"))
        .unwrap()
        .with_max_active(3);
    let now = ts(NOW);

    let lone = claimed(scheduler.claim_next(&mut store, &now).unwrap());
    assert_eq!(lone.node_id.as_str(), "cs_lone");
    match scheduler.claim_next(&mut store, &now).unwrap() {
        ClaimOutcome::NothingReady { conflicts, blocked } => {
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].0.as_str(), "cs_side");
            assert_eq!(conflicts[0].1.as_str(), "checkout");
            assert_eq!(blocked.len(), 1, "cs_down still waits for cs_side");
        }
        other => panic!("{other:?}"),
    }
    scheduler
        .complete(&mut store, &lone, &completed(&spec, &lone, b"lone"))
        .unwrap();
    let side = claimed(scheduler.claim_next(&mut store, &now).unwrap());
    assert_eq!(side.node_id.as_str(), "cs_side");

    // Shared access on both sides runs concurrently; exclusive against a
    // shared holder does not.
    let mut shared = graph("graphs/valid/diamond.json");
    let endpoint =
        |access: &str| serde_json::json!({ "resource": "model_endpoint", "access": access });
    shared.nodes[0].resource_scope = vec![serde_json::from_value(endpoint("shared")).unwrap()];
    shared.nodes[1].resource_scope = vec![serde_json::from_value(endpoint("shared")).unwrap()];
    let dir = temp_dir("shared");
    let (mut store, run) = seeded(&dir, &shared);
    let scheduler = Scheduler::new(&shared, &run, controller("ctl-a"))
        .unwrap()
        .with_max_active(2);
    let req = claimed(scheduler.claim_next(&mut store, &now).unwrap());
    let imp = claimed(scheduler.claim_next(&mut store, &now).unwrap());
    assert_eq!(
        (req.node_id.as_str(), imp.node_id.as_str()),
        ("cs_requirements", "cs_implementation")
    );
    assert_eq!(store.active_claims().unwrap().len(), 2);

    let mut mixed = graph("graphs/valid/diamond.json");
    mixed.nodes[0].resource_scope = vec![serde_json::from_value(endpoint("shared")).unwrap()];
    mixed.nodes[1].resource_scope = vec![serde_json::from_value(endpoint("exclusive")).unwrap()];
    let (mut store, run) = seeded(&temp_dir("mixed"), &mixed);
    let scheduler = Scheduler::new(&mixed, &run, controller("ctl-a"))
        .unwrap()
        .with_max_active(2);
    claimed(scheduler.claim_next(&mut store, &now).unwrap());
    match scheduler.claim_next(&mut store, &now).unwrap() {
        ClaimOutcome::NothingReady { conflicts, .. } => {
            assert_eq!(conflicts[0].0.as_str(), "cs_implementation");
            assert_eq!(conflicts[0].1.as_str(), "model_endpoint");
        }
        other => panic!("{other:?}"),
    }
}

/// Child-process body for the two-process test. Runs only when spawned with
/// `CHECKSPAN_CLAIM_STORE` set; otherwise it is an immediate no-op. The
/// result goes to the file named by `CHECKSPAN_CLAIM_OUT`.
#[test]
fn child_claim_worker() {
    let Ok(store_path) = std::env::var("CHECKSPAN_CLAIM_STORE") else {
        return;
    };
    let who = std::env::var("CHECKSPAN_CLAIM_CONTROLLER").unwrap();
    let out = std::env::var("CHECKSPAN_CLAIM_OUT").unwrap();
    let spec = graph("graphs/valid/chain-with-gate.json");
    let run = run_record("run-0001", &spec);
    let mut store = Store::open(&store_path).unwrap();
    let scheduler = Scheduler::new(&spec, &run, controller(&who)).unwrap();
    let line = match scheduler.claim_next(&mut store, &ts(NOW)) {
        Ok(ClaimOutcome::Claimed(c)) => {
            format!("claimed {} fence={} owner={}", c.node_id, c.fence, c.owner)
        }
        Ok(ClaimOutcome::Busy { active, max_active }) => {
            format!("busy active={active} max={max_active}")
        }
        Ok(ClaimOutcome::NothingReady { .. }) => "nothing".to_string(),
        Ok(ClaimOutcome::BudgetExhausted(reason)) => format!("budget {reason}"),
        Err(e) => format!("error {e}"),
    };
    fs::write(out, line).unwrap();
}

#[test]
fn two_real_controller_processes_cannot_claim_one_attempt() {
    let spec = graph("graphs/valid/chain-with-gate.json");
    let dir = temp_dir("processes");
    let (store, run) = seeded(&dir, &spec);
    let path = store.path().to_path_buf();
    drop(store);

    let spawn = |who: &str| {
        let out = dir.join(format!("{who}.txt"));
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "child_claim_worker",
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("CHECKSPAN_CLAIM_STORE", &path)
            .env("CHECKSPAN_CLAIM_CONTROLLER", who)
            .env("CHECKSPAN_CLAIM_OUT", &out)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        (child, out)
    };
    let (mut first, first_out) = spawn("proc-1");
    let (mut second, second_out) = spawn("proc-2");
    assert!(first.wait().unwrap().success(), "first child failed");
    assert!(second.wait().unwrap().success(), "second child failed");
    let results = [
        fs::read_to_string(first_out).unwrap(),
        fs::read_to_string(second_out).unwrap(),
    ];
    let claimed_count = results
        .iter()
        .filter(|r| r.starts_with("claimed cs_patch"))
        .count();
    let busy_count = results.iter().filter(|r| r.starts_with("busy")).count();
    assert_eq!((claimed_count, busy_count), (1, 1), "{results:?}");

    let store = Store::open(&path).unwrap();
    let claims = store.claims(&run.run_id).unwrap();
    assert_eq!(claims.len(), 1, "exactly one claim row");
    assert!(claims[0].active);
    assert_eq!(claims[0].fence, 1);
    let dispatches = store
        .events(&run.run_id)
        .unwrap()
        .iter()
        .filter(|e| e.kind == kinds::ATTEMPT_DISPATCHED)
        .count();
    assert_eq!(dispatches, 1);
    let winner = results.iter().find(|r| r.starts_with("claimed")).unwrap();
    assert!(winner.contains(&format!("owner={}", claims[0].owner)));
}

#[test]
fn a_run_of_another_graph_revision_is_refused() {
    let spec = graph("graphs/valid/chain-with-gate.json");
    let mut other = spec.clone();
    other.revision = checkspan::contracts::Revision::new(9).unwrap();
    let run = run_record("run-0001", &other);
    assert!(matches!(
        Scheduler::new(&spec, &run, controller("ctl-a")),
        Err(SchedulerError::Invalid(_))
    ));
    assert!(ControllerId::new("  ").is_err());
}
