//! Retry limits and persistent budgets: every started attempt counts,
//! including failures; restart and revision cannot reset the budget;
//! narrower hints preserve mandatory checks; widened source scope requires a
//! new approved contract.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use checkspan::budget::{
    Disposition, ExhaustReason, Narrowing, NarrowingError, apply_retry_policy, budget_status,
    check_narrowing, disposition,
};
use checkspan::contracts::{
    Attempt, Exactly, Execution, ExecutionOutcome, FailureClass, GraphRun, GraphSpec, Ident,
    NodeId, NodeSpec, NodeStatus, OnExhaustion, PortKind, PortSpec, RecordKind, ResultRef,
    RetryPolicy, Revision, RunId, SchemaVersion, Timestamp, VerifierReceipt, parse_record,
};
use checkspan::digests::artifact_digest;
use checkspan::scheduler::{Claim, ClaimOutcome, ControllerId, Scheduler};
use checkspan::state::{RunState, kinds};
use checkspan::store::Store;
use common::fixture;

const NOW: &str = "2026-09-06T16:00:00Z";

fn ts(s: &str) -> Timestamp {
    Timestamp::new(s).unwrap()
}

fn node(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("checkspan-budget-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_record(run_id: &str, spec: &GraphSpec, lineage: Option<&str>) -> GraphRun {
    GraphRun {
        record: RecordKind::GraphRun,
        schema_version: SchemaVersion(1),
        run_id: RunId::new(run_id).unwrap(),
        graph_ref: spec.graph_ref(),
        budget_lineage_ref: lineage.map(|l| RunId::new(l).unwrap()),
        admitted_imports: vec![],
    }
}

fn chain() -> GraphSpec {
    parse_record(&fixture("graphs/valid/chain-with-gate.json")).unwrap()
}

fn seeded(dir: &Path, spec: &GraphSpec) -> (Store, GraphRun) {
    let mut store = Store::open(dir.join("ledger.sqlite")).unwrap();
    store.store_graph(spec).unwrap();
    let run = run_record("run-0001", spec, None);
    store.create_run(&run).unwrap();
    (store, run)
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
        digest: artifact_digest(format!("result {}", claim.number).as_bytes()),
    });
    Attempt {
        record: RecordKind::Attempt,
        schema_version: SchemaVersion(1),
        run_id: claim.run_id.clone(),
        node: spec.node_ref(&claim.node_id).unwrap(),
        number: claim.number,
        owner: claim.owner.as_str().to_string(),
        started_at: ts(NOW),
        finished_at: Some(ts("2026-09-06T16:05:00Z")),
        repair_hint: claim.narrowing.as_ref().and_then(|n| n.repair_hint.clone()),
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

fn reject(attempt: &Attempt) -> VerifierReceipt {
    let mut receipt: VerifierReceipt =
        parse_record(&fixture("outcomes/valid/receipt-reject.json")).unwrap();
    receipt.id = Ident::new(format!("rcpt-reject-{}", attempt.number)).unwrap();
    receipt.attempt = attempt.attempt_ref();
    receipt.result_digest = attempt.result.as_ref().unwrap().digest.clone();
    receipt
}

fn status_of(store: &Store, spec: &GraphSpec, run: &GraphRun, node_id: &str) -> NodeStatus {
    RunState::replay(
        run.run_id.clone(),
        spec,
        &store.events(&run.run_id).unwrap(),
    )
    .unwrap()
    .node(&node(node_id))
    .unwrap()
    .status
}

#[test]
fn every_started_attempt_counts_including_failures_until_the_node_cap() {
    let spec = chain();
    let dir = temp_dir("cap");
    let (mut store, run) = seeded(&dir, &spec);
    let now = ts(NOW);
    let a = Scheduler::new(&spec, &run, ControllerId::new("ctl-a").unwrap()).unwrap();
    let b = Scheduler::new(&spec, &run, ControllerId::new("ctl-b").unwrap()).unwrap();
    let patch = node("cs_patch");

    // Attempt 1: claimed by A, reclaimed by B (an infrastructure failure).
    claimed(a.claim_next(&mut store, &now).unwrap());
    b.reclaim(&mut store, &patch, &now).unwrap();
    assert_eq!(
        budget_status(&store, &spec, &run, &now)
            .unwrap()
            .consumed_here,
        1
    );
    let decision =
        apply_retry_policy(&mut store, &spec, &run, &patch, &Narrowing::default(), &now).unwrap();
    assert_eq!(
        decision,
        Disposition::Retry {
            class: FailureClass::Infrastructure,
            started: 1,
            max_attempts: 3
        }
    );
    assert_eq!(status_of(&store, &spec, &run, "cs_patch"), NodeStatus::Open);
    assert_eq!(
        store.events(&run.run_id).unwrap().last().unwrap().kind,
        kinds::RETRY_ALLOWED
    );

    // Attempt 2: completes and is rejected by the verifier.
    let second = claimed(a.claim_next(&mut store, &now).unwrap());
    assert_eq!(second.number.get(), 2);
    let attempt = finished(&spec, &second, ExecutionOutcome::Completed);
    a.complete(&mut store, &second, &attempt).unwrap();
    store.admit_receipt(&reject(&attempt)).unwrap();
    assert_eq!(
        status_of(&store, &spec, &run, "cs_patch"),
        NodeStatus::Rejected
    );
    assert!(matches!(
        apply_retry_policy(&mut store, &spec, &run, &patch, &Narrowing::default(), &now).unwrap(),
        Disposition::Retry {
            class: FailureClass::Rejected,
            started: 2,
            ..
        }
    ));

    // Attempt 3: times out. Three started attempts reach the cap.
    let third = claimed(a.claim_next(&mut store, &now).unwrap());
    assert_eq!(third.number.get(), 3);
    a.complete(
        &mut store,
        &third,
        &finished(&spec, &third, ExecutionOutcome::TimedOut),
    )
    .unwrap();
    assert_eq!(
        status_of(&store, &spec, &run, "cs_patch"),
        NodeStatus::Failed
    );
    let events_before = store.events(&run.run_id).unwrap().len();
    let decision =
        apply_retry_policy(&mut store, &spec, &run, &patch, &Narrowing::default(), &now).unwrap();
    assert_eq!(
        decision,
        Disposition::Exhausted {
            reason: ExhaustReason::NodeCap {
                max_attempts: 3,
                started: 3
            },
            route: OnExhaustion::Gate,
        }
    );
    assert_eq!(
        store.events(&run.run_id).unwrap().len(),
        events_before,
        "gate routing writes nothing here"
    );
    assert_eq!(
        status_of(&store, &spec, &run, "cs_patch"),
        NodeStatus::Failed
    );
    let status = budget_status(&store, &spec, &run, &now).unwrap();
    assert_eq!(
        (
            status.consumed_here,
            status.consumed_by_lineage,
            status.remaining()
        ),
        (3, 0, 6)
    );

    // A node with no failed or rejected attempt has nothing to retry.
    assert!(matches!(
        apply_retry_policy(
            &mut store,
            &spec,
            &run,
            &node("cs_ci"),
            &Narrowing::default(),
            &now
        ),
        Err(checkspan::budget::BudgetError::NoOutcome {
            status: NodeStatus::Open,
            ..
        })
    ));

    // Restart: the counts and the decision are the same from the file.
    drop(store);
    let store = Store::open(dir.join("ledger.sqlite")).unwrap();
    let again = budget_status(&store, &spec, &run, &now).unwrap();
    assert_eq!(again, status);
    let state = RunState::replay(
        run.run_id.clone(),
        &spec,
        &store.events(&run.run_id).unwrap(),
    )
    .unwrap();
    assert_eq!(
        disposition(&store, &spec, &run, &state, &patch, &now).unwrap(),
        decision
    );
}

#[test]
fn a_failure_class_outside_the_policy_exhausts_and_cancel_routes_cancel() {
    let mut spec = chain();
    spec.nodes[0].retry_policy = RetryPolicy {
        version: Exactly,
        max_attempts: 3,
        allowed_failure_classes: vec![FailureClass::Infrastructure],
        on_exhaustion: OnExhaustion::Cancel,
    };
    let (mut store, run) = seeded(&temp_dir("class"), &spec);
    let now = ts(NOW);
    let scheduler = Scheduler::new(&spec, &run, ControllerId::new("ctl-a").unwrap()).unwrap();
    let patch = node("cs_patch");

    let claim = claimed(scheduler.claim_next(&mut store, &now).unwrap());
    let attempt = finished(&spec, &claim, ExecutionOutcome::Completed);
    scheduler.complete(&mut store, &claim, &attempt).unwrap();
    store.admit_receipt(&reject(&attempt)).unwrap();
    let decision =
        apply_retry_policy(&mut store, &spec, &run, &patch, &Narrowing::default(), &now).unwrap();
    assert_eq!(
        decision,
        Disposition::Exhausted {
            reason: ExhaustReason::ClassNotRetryable {
                class: FailureClass::Rejected
            },
            route: OnExhaustion::Cancel,
        }
    );
    let last = store.events(&run.run_id).unwrap().pop().unwrap();
    assert_eq!(last.kind, kinds::NODE_CANCELLED);
    assert_eq!(last.payload["exhausted"]["reason"], "class_not_retryable");
    assert_eq!(
        status_of(&store, &spec, &run, "cs_patch"),
        NodeStatus::Cancelled
    );

    // Timeouts are their own class.
    let run2 = run_record("run-0002", &spec, None);
    store.create_run(&run2).unwrap();
    let scheduler = Scheduler::new(&spec, &run2, ControllerId::new("ctl-a").unwrap()).unwrap();
    let claim = claimed(scheduler.claim_next(&mut store, &now).unwrap());
    scheduler
        .complete(
            &mut store,
            &claim,
            &finished(&spec, &claim, ExecutionOutcome::TimedOut),
        )
        .unwrap();
    assert!(matches!(
        apply_retry_policy(
            &mut store,
            &spec,
            &run2,
            &patch,
            &Narrowing::default(),
            &now
        )
        .unwrap(),
        Disposition::Exhausted {
            reason: ExhaustReason::ClassNotRetryable {
                class: FailureClass::Timeout
            },
            ..
        }
    ));
}

#[test]
fn the_graph_budget_persists_through_lineage_revision_and_restart() {
    let mut spec: GraphSpec =
        parse_record(&fixture("contracts/valid/graph-spec-minimal.json")).unwrap();
    spec.nodes[0].retry_policy.max_attempts = 10;
    assert_eq!(spec.budget.total_attempts, 3);
    let dir = temp_dir("lineage");
    let (mut store, run1) = seeded(&dir, &spec);
    let now = ts(NOW);
    let packet = node("cs_packet");
    let a = Scheduler::new(&spec, &run1, ControllerId::new("ctl-a").unwrap()).unwrap();
    let b = Scheduler::new(&spec, &run1, ControllerId::new("ctl-b").unwrap()).unwrap();
    for _ in 0..2 {
        claimed(a.claim_next(&mut store, &now).unwrap());
        b.reclaim(&mut store, &packet, &now).unwrap();
        apply_retry_policy(
            &mut store,
            &spec,
            &run1,
            &packet,
            &Narrowing::default(),
            &now,
        )
        .unwrap();
    }
    assert_eq!(
        budget_status(&store, &spec, &run1, &now)
            .unwrap()
            .consumed_here,
        2
    );

    // A restart of the same graph inherits the two attempts already spent.
    let run2 = run_record("run-0002", &spec, Some("run-0001"));
    store.create_run(&run2).unwrap();
    let status = budget_status(&store, &spec, &run2, &now).unwrap();
    assert_eq!(
        (
            status.consumed_here,
            status.consumed_by_lineage,
            status.remaining()
        ),
        (0, 2, 1)
    );
    assert_eq!(status.lineage, vec![run1.run_id.clone()]);
    let a2 = Scheduler::new(&spec, &run2, ControllerId::new("ctl-a").unwrap()).unwrap();
    let b2 = Scheduler::new(&spec, &run2, ControllerId::new("ctl-b").unwrap()).unwrap();
    claimed(a2.claim_next(&mut store, &now).unwrap());
    b2.reclaim(&mut store, &packet, &now).unwrap();
    let decision = apply_retry_policy(
        &mut store,
        &spec,
        &run2,
        &packet,
        &Narrowing::default(),
        &now,
    )
    .unwrap();
    assert_eq!(
        decision,
        Disposition::Exhausted {
            reason: ExhaustReason::GraphBudget {
                total: 3,
                consumed: 3
            },
            route: OnExhaustion::Gate,
        }
    );
    assert!(matches!(
        a2.claim_next(&mut store, &now).unwrap(),
        ClaimOutcome::BudgetExhausted(ExhaustReason::GraphBudget {
            total: 3,
            consumed: 3
        })
    ));

    // A new graph revision with the same budget and lineage cannot start over.
    let mut revised = spec.clone();
    revised.revision = Revision::new(2).unwrap();
    revised.supersedes = Some(spec.graph_ref());
    revised.nodes[0].prompt.push_str(" (revised)");
    store.store_graph(&revised).unwrap();
    let run3 = run_record("run-0003", &revised, Some("run-0002"));
    store.create_run(&run3).unwrap();
    let status = budget_status(&store, &revised, &run3, &now).unwrap();
    assert_eq!(status.lineage, [run2.run_id.clone(), run1.run_id.clone()]);
    assert_eq!(status.consumed(), 3);
    let a3 = Scheduler::new(&revised, &run3, ControllerId::new("ctl-a").unwrap()).unwrap();
    assert!(matches!(
        a3.claim_next(&mut store, &now).unwrap(),
        ClaimOutcome::BudgetExhausted(ExhaustReason::GraphBudget { .. })
    ));

    // Only an explicit, larger budget in a new revision buys more attempts.
    let mut bigger = revised.clone();
    bigger.revision = Revision::new(3).unwrap();
    bigger.supersedes = Some(revised.graph_ref());
    bigger.budget.total_attempts = 5;
    store.store_graph(&bigger).unwrap();
    let run4 = run_record("run-0004", &bigger, Some("run-0003"));
    store.create_run(&run4).unwrap();
    let status = budget_status(&store, &bigger, &run4, &now).unwrap();
    assert_eq!((status.consumed(), status.remaining()), (3, 2));

    // A lineage that names an unknown run is refused.
    let orphan = run_record("run-0009", &bigger, Some("run-0099"));
    store.create_run(&orphan).unwrap();
    assert!(matches!(
        budget_status(&store, &bigger, &orphan, &now),
        Err(checkspan::budget::BudgetError::Invalid(what)) if what.contains("run-0099")
    ));

    // Restart: the file says the same thing.
    drop(store);
    let store = Store::open(dir.join("ledger.sqlite")).unwrap();
    assert_eq!(
        budget_status(&store, &revised, &run3, &now)
            .unwrap()
            .consumed(),
        3
    );
    assert_eq!(
        budget_status(&store, &bigger, &run4, &now)
            .unwrap()
            .remaining(),
        2
    );
}

#[test]
fn a_passed_deadline_blocks_every_new_attempt() {
    let mut spec = chain();
    spec.budget.deadline = Some(ts("2026-09-06T12:00:00Z"));
    let (mut store, run) = seeded(&temp_dir("deadline"), &spec);
    let scheduler = Scheduler::new(&spec, &run, ControllerId::new("ctl-a").unwrap()).unwrap();
    let other = Scheduler::new(&spec, &run, ControllerId::new("ctl-b").unwrap()).unwrap();
    let before = ts("2026-09-06T11:00:00Z");
    let after = ts("2026-09-06T12:00:00Z");
    assert!(matches!(
        scheduler.claim_next(&mut store, &after).unwrap(),
        ClaimOutcome::BudgetExhausted(ExhaustReason::DeadlinePassed(_))
    ));
    let claim = claimed(scheduler.claim_next(&mut store, &before).unwrap());
    other.reclaim(&mut store, &claim.node_id, &after).unwrap();
    assert!(matches!(
        apply_retry_policy(
            &mut store,
            &spec,
            &run,
            &claim.node_id,
            &Narrowing::default(),
            &after
        )
        .unwrap(),
        Disposition::Exhausted {
            reason: ExhaustReason::DeadlinePassed(_),
            route: OnExhaustion::Gate
        }
    ));
    assert!(matches!(
        apply_retry_policy(
            &mut store,
            &spec,
            &run,
            &claim.node_id,
            &Narrowing::default(),
            &before
        )
        .unwrap(),
        Disposition::Retry { .. }
    ));
}

#[test]
fn narrowing_only_shrinks_and_never_touches_acceptance() {
    let research: NodeSpec =
        serde_json::from_str(&fixture("nodes/valid/task-research.json")).unwrap();
    let corpus = Ident::new("corpus").unwrap();
    let prior = Ident::new("prior").unwrap();
    let before = research.clone();

    let ports = check_narrowing(
        &research,
        &Narrowing {
            repair_hint: Some("only the null-input branch".into()),
            dropped_ports: vec![prior.clone()],
            restricted_sources: vec![],
        },
    )
    .unwrap();
    assert_eq!(
        ports.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["corpus"]
    );
    assert_eq!(research, before, "the contract is not mutated");
    assert_eq!(
        research.acceptance.required_checks,
        before.acceptance.required_checks
    );

    let restricted = check_narrowing(
        &research,
        &Narrowing {
            repair_hint: None,
            dropped_ports: vec![],
            restricted_sources: vec![(corpus.clone(), vec!["corpus:pilot-docs@3".into()])],
        },
    )
    .unwrap();
    assert_eq!(restricted[0].allowed_source_scope, ["corpus:pilot-docs@3"]);

    type Expect = fn(&NarrowingError) -> bool;
    let cases: Vec<(Narrowing, Expect)> = vec![
        (
            Narrowing {
                dropped_ports: vec![corpus.clone()],
                ..Default::default()
            },
            |e| matches!(e, NarrowingError::RequiredPort(p) if p.as_str() == "corpus"),
        ),
        (
            Narrowing {
                dropped_ports: vec![Ident::new("ghost").unwrap()],
                ..Default::default()
            },
            |e| matches!(e, NarrowingError::UnknownPort(_)),
        ),
        (
            Narrowing {
                dropped_ports: vec![prior.clone(), prior.clone()],
                ..Default::default()
            },
            |e| matches!(e, NarrowingError::DuplicatePort(_)),
        ),
        (
            Narrowing {
                dropped_ports: vec![prior.clone()],
                restricted_sources: vec![(prior.clone(), vec!["graph:review_pilot".into()])],
                ..Default::default()
            },
            |e| matches!(e, NarrowingError::RestrictedAndDropped(_)),
        ),
        (
            Narrowing {
                restricted_sources: vec![(corpus.clone(), vec![])],
                ..Default::default()
            },
            |e| matches!(e, NarrowingError::EmptySources(_)),
        ),
        (
            Narrowing {
                restricted_sources: vec![(
                    corpus.clone(),
                    vec!["corpus:pilot-docs@3".into(), "corpus:other@1".into()],
                )],
                ..Default::default()
            },
            |e| matches!(e, NarrowingError::WidensScope { source, .. } if source == "corpus:other@1"),
        ),
    ];
    for (narrowing, expected) in cases {
        let err = check_narrowing(&research, &narrowing).unwrap_err();
        assert!(expected(&err), "{narrowing:?}: {err}");
    }
}

#[test]
fn a_recorded_narrowing_reaches_the_next_claim_and_the_contract_stays_put() {
    let mut spec = chain();
    let template = spec.nodes[0].evidence_ports[0].clone();
    spec.nodes[0].evidence_ports.push(PortSpec {
        name: Ident::new("hints").unwrap(),
        kind: PortKind::Logs,
        expected_type: template.expected_type.clone(),
        required: false,
        allowed_source_scope: vec!["local:notes".into()],
        handling_policy_ref: template.handling_policy_ref.clone(),
    });
    let (mut store, run) = seeded(&temp_dir("narrowing"), &spec);
    let stored_digest = store.store_graph(&spec).unwrap().digest;
    let now = ts(NOW);
    let a = Scheduler::new(&spec, &run, ControllerId::new("ctl-a").unwrap()).unwrap();
    let b = Scheduler::new(&spec, &run, ControllerId::new("ctl-b").unwrap()).unwrap();
    let patch = node("cs_patch");
    claimed(a.claim_next(&mut store, &now).unwrap());
    b.reclaim(&mut store, &patch, &now).unwrap();

    // Dropping the required port is refused and records nothing.
    let events_before = store.events(&run.run_id).unwrap().len();
    let refused = apply_retry_policy(
        &mut store,
        &spec,
        &run,
        &patch,
        &Narrowing {
            dropped_ports: vec![Ident::new("candidate").unwrap()],
            ..Default::default()
        },
        &now,
    );
    assert!(matches!(
        refused,
        Err(checkspan::budget::BudgetError::Narrowing(
            NarrowingError::RequiredPort(_)
        ))
    ));
    assert_eq!(store.events(&run.run_id).unwrap().len(), events_before);
    assert_eq!(
        status_of(&store, &spec, &run, "cs_patch"),
        NodeStatus::Failed
    );

    let narrowing = Narrowing {
        repair_hint: Some("Retry with the same obligation; skip the optional notes.".into()),
        dropped_ports: vec![Ident::new("hints").unwrap()],
        restricted_sources: vec![],
    };
    assert!(matches!(
        apply_retry_policy(&mut store, &spec, &run, &patch, &narrowing, &now).unwrap(),
        Disposition::Retry { .. }
    ));
    let recorded = store.events(&run.run_id).unwrap().pop().unwrap();
    assert_eq!(recorded.kind, kinds::RETRY_ALLOWED);
    assert_eq!(recorded.payload["dropped_ports"][0], "hints");
    assert!(
        recorded.payload["repair_hint"]
            .as_str()
            .unwrap()
            .contains("same obligation")
    );

    let next = claimed(a.claim_next(&mut store, &now).unwrap());
    assert_eq!(next.number.get(), 2);
    assert_eq!(next.narrowing, Some(narrowing.clone()));
    let attempt = finished(&spec, &next, ExecutionOutcome::Completed);
    assert_eq!(
        attempt.repair_hint, narrowing.repair_hint,
        "the hint travels on the attempt"
    );
    a.complete(&mut store, &next, &attempt).unwrap();
    let third = store.claims(&run.run_id).unwrap();
    assert_eq!(third.len(), 2);

    // The stored contract is byte-identical: acceptance and ports unchanged.
    assert_eq!(store.store_graph(&spec).unwrap().digest, stored_digest);
    let loaded = store.load_graph(&spec.graph_ref()).unwrap().unwrap();
    assert_eq!(loaded, spec);
    assert_eq!(loaded.nodes[0].acceptance.required_checks.len(), 2);
    assert_eq!(loaded.nodes[0].evidence_ports.len(), 2);
}

#[test]
fn a_widened_scope_requires_a_new_approved_contract_revision() {
    let spec = chain();
    let (mut store, run) = seeded(&temp_dir("widen"), &spec);
    let now = ts(NOW);
    let a = Scheduler::new(&spec, &run, ControllerId::new("ctl-a").unwrap()).unwrap();
    let b = Scheduler::new(&spec, &run, ControllerId::new("ctl-b").unwrap()).unwrap();
    let patch = node("cs_patch");
    claimed(a.claim_next(&mut store, &now).unwrap());
    b.reclaim(&mut store, &patch, &now).unwrap();

    let widening = Narrowing {
        restricted_sources: vec![(
            Ident::new("candidate").unwrap(),
            vec![
                "repo:USS-Parks/Checkspan".into(),
                "repo:someone-else/fork".into(),
            ],
        )],
        ..Default::default()
    };
    assert!(matches!(
        apply_retry_policy(&mut store, &spec, &run, &patch, &widening, &now),
        Err(checkspan::budget::BudgetError::Narrowing(NarrowingError::WidensScope { source, .. }))
            if source == "repo:someone-else/fork"
    ));
    assert_eq!(
        status_of(&store, &spec, &run, "cs_patch"),
        NodeStatus::Failed
    );

    // The approved path: a new node revision in a new graph revision.
    let mut wider = spec.clone();
    wider.revision = Revision::new(2).unwrap();
    wider.supersedes = Some(spec.graph_ref());
    wider.nodes[0].revision = Revision::new(2).unwrap();
    wider.nodes[0].supersedes = Some(spec.node_ref(&patch).unwrap());
    wider.nodes[0].evidence_ports[0]
        .allowed_source_scope
        .push("repo:someone-else/fork".into());
    for node in wider.nodes.iter_mut().skip(1) {
        for dep in &mut node.deps {
            if dep.node.node_id == patch {
                dep.node.revision = Revision::new(2).unwrap();
            }
        }
    }
    store.store_graph(&wider).unwrap();
    let run2 = run_record("run-0002", &wider, Some("run-0001"));
    store.create_run(&run2).unwrap();
    let scheduler = Scheduler::new(&wider, &run2, ControllerId::new("ctl-a").unwrap()).unwrap();
    let claim = claimed(scheduler.claim_next(&mut store, &now).unwrap());
    assert_eq!(claim.node_id, patch);
    assert_eq!(
        budget_status(&store, &wider, &run2, &now)
            .unwrap()
            .consumed(),
        2,
        "the old run's attempt still counts"
    );
}
