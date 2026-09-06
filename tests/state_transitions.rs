//! The state reducer: the full transition matrix, no route to `accepted`
//! except an admitted accept receipt, timeouts are failures not rejections,
//! approval is not a pass, stale completions are refused, and replaying the
//! same ordered events yields the same view.

mod common;

use std::collections::BTreeSet;
use std::fs;

use checkspan::contracts::{
    AttemptNumber, DecisionKind, ExecutionOutcome, GraphRun, GraphSpec, Ident, NodeId, NodeRef,
    NodeStatus, ReceiptRef, RecordKind, RunId, SchemaVersion, Verdict, ViewError, parse_record,
};
use checkspan::state::{NodeEvent, NodeState, RunState, TransitionError, kinds};
use checkspan::store::Store;
use common::fixture;
use serde_json::json;

fn n(i: u32) -> AttemptNumber {
    AttemptNumber::new(i).unwrap()
}

fn receipt(id: &str) -> ReceiptRef {
    ReceiptRef {
        id: Ident::new(id).unwrap(),
        run_id: RunId::new("run-0001").unwrap(),
        node: NodeRef {
            graph_id: checkspan::contracts::GraphId::new("review_pilot").unwrap(),
            node_id: NodeId::new("cs_ci").unwrap(),
            revision: checkspan::contracts::Revision::new(2).unwrap(),
        },
    }
}

fn packet(id: &str) -> Ident {
    Ident::new(id).unwrap()
}

fn dispatched(i: u32) -> NodeEvent {
    NodeEvent::Dispatched { attempt: n(i) }
}

fn finished(i: u32, outcome: ExecutionOutcome) -> NodeEvent {
    NodeEvent::ExecutionFinished {
        attempt: n(i),
        outcome,
        result_digest: None,
    }
}

fn admitted(i: u32, verdict: Verdict) -> NodeEvent {
    NodeEvent::ReceiptAdmitted {
        attempt: n(i),
        receipt: receipt(&format!("rcpt-{i}-{}", verdict.as_str())),
        verdict,
    }
}

fn decided(id: &str, decision: DecisionKind) -> NodeEvent {
    NodeEvent::DecisionAdmitted {
        packet: packet(id),
        decision,
    }
}

/// Canonical event prefixes that put a fresh node into each reachable
/// internal state.
fn prefixes() -> Vec<(&'static str, Vec<NodeEvent>)> {
    vec![
        ("open", vec![]),
        ("running", vec![dispatched(1)]),
        (
            "checking",
            vec![dispatched(1), finished(1, ExecutionOutcome::Completed)],
        ),
        (
            "accepted",
            vec![
                dispatched(1),
                finished(1, ExecutionOutcome::Completed),
                admitted(1, Verdict::Accept),
            ],
        ),
        (
            "rejected",
            vec![
                dispatched(1),
                finished(1, ExecutionOutcome::Completed),
                admitted(1, Verdict::Reject),
            ],
        ),
        (
            "failed",
            vec![dispatched(1), finished(1, ExecutionOutcome::Failed)],
        ),
        (
            "gated_undecidable_no_packet",
            vec![
                dispatched(1),
                finished(1, ExecutionOutcome::Completed),
                admitted(1, Verdict::Undecidable),
            ],
        ),
        (
            "gated",
            vec![
                dispatched(1),
                finished(1, ExecutionOutcome::Completed),
                admitted(1, Verdict::Reject),
                NodeEvent::GateOpened {
                    packet: packet("gate-1"),
                },
            ],
        ),
        ("cancelled", vec![NodeEvent::Cancelled]),
    ]
}

/// Every event shape the matrix exercises against every state.
fn probes() -> Vec<(&'static str, NodeEvent)> {
    vec![
        ("dispatch_next", dispatched(1)),
        ("dispatch_second", dispatched(2)),
        ("finish_completed", finished(1, ExecutionOutcome::Completed)),
        ("finish_failed", finished(1, ExecutionOutcome::Failed)),
        ("finish_timed_out", finished(1, ExecutionOutcome::TimedOut)),
        ("finish_cancelled", finished(1, ExecutionOutcome::Cancelled)),
        ("finish_stale", finished(9, ExecutionOutcome::Completed)),
        ("accept", admitted(1, Verdict::Accept)),
        ("reject", admitted(1, Verdict::Reject)),
        ("undecidable", admitted(1, Verdict::Undecidable)),
        ("accept_stale", admitted(9, Verdict::Accept)),
        (
            "gate_open",
            NodeEvent::GateOpened {
                packet: packet("gate-1"),
            },
        ),
        ("decide_retry", decided("gate-1", DecisionKind::Retry)),
        ("decide_cancel", decided("gate-1", DecisionKind::Cancel)),
        ("decide_revise", decided("gate-1", DecisionKind::Revise)),
        (
            "decide_approve",
            decided("gate-1", DecisionKind::ApproveAction),
        ),
        ("decide_deny", decided("gate-1", DecisionKind::DenyAction)),
        (
            "decide_assess",
            decided("gate-1", DecisionKind::RecordAssessment),
        ),
        (
            "decide_wrong_packet",
            decided("gate-9", DecisionKind::Retry),
        ),
        ("retry_allowed", NodeEvent::RetryAllowed),
        ("cancel", NodeEvent::Cancelled),
    ]
}

/// The legal cells of the matrix: (state, probe) -> resulting status.
fn legal() -> Vec<(&'static str, &'static str, NodeStatus)> {
    use NodeStatus::*;
    vec![
        ("open", "dispatch_next", Running),
        ("open", "cancel", Cancelled),
        ("running", "finish_completed", Running),
        ("running", "finish_failed", Failed),
        ("running", "finish_timed_out", Failed),
        ("running", "finish_cancelled", Cancelled),
        ("running", "cancel", Cancelled),
        ("checking", "accept", Accepted),
        ("checking", "reject", Rejected),
        ("checking", "undecidable", Gated),
        ("checking", "cancel", Cancelled),
        ("rejected", "retry_allowed", Open),
        ("rejected", "gate_open", Gated),
        ("rejected", "cancel", Cancelled),
        ("failed", "retry_allowed", Open),
        ("failed", "gate_open", Gated),
        ("failed", "cancel", Cancelled),
        ("gated_undecidable_no_packet", "gate_open", Gated),
        ("gated_undecidable_no_packet", "cancel", Cancelled),
        ("gated", "decide_retry", Open),
        ("gated", "decide_cancel", Cancelled),
        ("gated", "decide_revise", Cancelled),
        ("gated", "decide_approve", Open),
        ("gated", "decide_deny", Open),
        ("gated", "decide_assess", Open),
        ("gated", "cancel", Cancelled),
    ]
}

#[test]
fn the_transition_matrix_is_exactly_the_legal_set() {
    let legal = legal();
    let mut accepted_via: BTreeSet<&str> = BTreeSet::new();
    for (state_name, prefix) in prefixes() {
        let state = NodeState::replay(&prefix).unwrap_or_else(|e| panic!("{state_name}: {e}"));
        for (probe_name, event) in probes() {
            let expected = legal
                .iter()
                .find(|(s, p, _)| *s == state_name && *p == probe_name)
                .map(|(_, _, status)| *status);
            match (state.apply(&event), expected) {
                (Ok(next), Some(status)) => {
                    assert_eq!(next.status, status, "{state_name} + {probe_name}");
                    if status == NodeStatus::Accepted {
                        accepted_via.insert(probe_name);
                    }
                }
                (Ok(next), None) => panic!(
                    "{state_name} + {probe_name} must be illegal, produced {:?}",
                    next.status
                ),
                (Err(e), Some(_)) => panic!("{state_name} + {probe_name} must be legal: {e}"),
                (Err(_), None) => {}
            }
        }
    }
    assert_eq!(
        accepted_via.into_iter().collect::<Vec<_>>(),
        ["accept"],
        "only an admitted accept receipt reaches accepted"
    );
}

#[test]
fn an_illegal_event_leaves_the_state_unchanged_and_names_the_reason() {
    let state = NodeState::replay(&[dispatched(1)]).unwrap();
    let before = state.clone();
    let err = state.apply(&admitted(1, Verdict::Accept)).unwrap_err();
    assert!(matches!(
        err,
        TransitionError::Illegal {
            from: NodeStatus::Running,
            event: kinds::RECEIPT_ADMITTED,
            ..
        }
    ));
    assert!(
        err.to_string()
            .contains("no verdict before execution finishes")
    );
    assert_eq!(state, before);

    let err = NodeState::open().apply(&dispatched(2)).unwrap_err();
    assert!(matches!(
        err,
        TransitionError::WrongAttemptNumber { expected: 1, .. }
    ));

    let err = state
        .apply(&finished(2, ExecutionOutcome::Completed))
        .unwrap_err();
    assert!(matches!(err, TransitionError::StaleAttempt { current: Some(c), .. } if c.get() == 1));

    let gated = NodeState::replay(&prefixes()[7].1).unwrap();
    let err = gated
        .apply(&decided("gate-9", DecisionKind::Retry))
        .unwrap_err();
    assert!(matches!(err, TransitionError::WrongPacket { .. }));
    assert!(err.to_string().contains("gate-9"));
}

#[test]
fn a_timeout_is_a_failure_never_a_rejection_and_cannot_take_a_verdict() {
    let timed_out =
        NodeState::replay(&[dispatched(1), finished(1, ExecutionOutcome::TimedOut)]).unwrap();
    assert_eq!(timed_out.status, NodeStatus::Failed);
    assert!(timed_out.receipt.is_none());
    for verdict in Verdict::ALL {
        assert!(timed_out.apply(&admitted(1, verdict)).is_err(), "{verdict}");
    }
    let crashed =
        NodeState::replay(&[dispatched(1), finished(1, ExecutionOutcome::Failed)]).unwrap();
    assert_eq!(crashed.status, NodeStatus::Failed);
    assert_ne!(crashed.status, NodeStatus::Rejected);
}

#[test]
fn approval_reopens_the_node_and_never_accepts_it() {
    let gated = NodeState::replay(&prefixes()[7].1).unwrap();
    for decision in [
        DecisionKind::ApproveAction,
        DecisionKind::DenyAction,
        DecisionKind::RecordAssessment,
        DecisionKind::Retry,
    ] {
        let next = gated.apply(&decided("gate-1", decision)).unwrap();
        assert_eq!(next.status, NodeStatus::Open, "{decision}");
        assert!(next.receipt.is_none(), "{decision}");
        assert!(next.waiting_on_gate.is_none(), "{decision}");
        assert_eq!(next.attempts_started, 1, "{decision}: history is kept");
        let dispatched_again = next.apply(&dispatched(2)).unwrap();
        assert_eq!(dispatched_again.current_attempt, Some(n(2)));
    }
    for decision in [DecisionKind::Cancel, DecisionKind::Revise] {
        assert_eq!(
            gated.apply(&decided("gate-1", decision)).unwrap().status,
            NodeStatus::Cancelled,
            "{decision}"
        );
    }
}

#[test]
fn stale_completions_are_refused_after_a_retry() {
    let second = NodeState::replay(&[
        dispatched(1),
        finished(1, ExecutionOutcome::Completed),
        admitted(1, Verdict::Reject),
        NodeEvent::RetryAllowed,
        dispatched(2),
    ])
    .unwrap();
    assert_eq!(second.current_attempt, Some(n(2)));
    assert_eq!(second.attempts_started, 2);
    assert!(
        second.receipt.is_none(),
        "the rejection receipt belongs to attempt 1"
    );
    assert!(matches!(
        second.apply(&finished(1, ExecutionOutcome::Completed)),
        Err(TransitionError::StaleAttempt { .. })
    ));
    let checking = second
        .apply(&finished(2, ExecutionOutcome::Completed))
        .unwrap();
    assert!(matches!(
        checking.apply(&admitted(1, Verdict::Accept)),
        Err(TransitionError::StaleAttempt { .. })
    ));
    let accepted = checking.apply(&admitted(2, Verdict::Accept)).unwrap();
    assert_eq!(accepted.status, NodeStatus::Accepted);
    assert_eq!(
        accepted.receipt.as_ref().unwrap().id.as_str(),
        "rcpt-2-accept"
    );
    assert!(accepted.is_terminal());
    assert!(accepted.apply(&NodeEvent::Cancelled).is_err(), "terminal");
}

#[test]
fn undecidable_gates_the_node_and_the_view_needs_its_packet() {
    let undecided = NodeState::replay(&prefixes()[6].1).unwrap();
    assert_eq!(undecided.status, NodeStatus::Gated);
    let run_id = RunId::new("run-0001").unwrap();
    let node = receipt("x").node;
    let view = undecided.to_view(&run_id, &node);
    assert_eq!(
        view.check_bindings(),
        vec![ViewError::GateRequired],
        "not consistent until the packet opens"
    );
    let gated = undecided
        .apply(&NodeEvent::GateOpened {
            packet: packet("gate-2"),
        })
        .unwrap();
    let view = gated.to_view(&run_id, &node);
    assert_eq!(view.check_bindings(), vec![]);
    assert_eq!(view.waiting_on_gate.unwrap().as_str(), "gate-2");
    assert!(
        gated
            .apply(&NodeEvent::GateOpened {
                packet: packet("gate-3"),
            })
            .is_err()
    );
}

#[test]
fn replaying_the_same_events_yields_the_same_view() {
    let events = vec![
        dispatched(1),
        finished(1, ExecutionOutcome::Failed),
        NodeEvent::RetryAllowed,
        dispatched(2),
        finished(2, ExecutionOutcome::Completed),
        admitted(2, Verdict::Reject),
        NodeEvent::GateOpened {
            packet: packet("gate-1"),
        },
        decided("gate-1", DecisionKind::Retry),
        dispatched(3),
        finished(3, ExecutionOutcome::Completed),
        admitted(3, Verdict::Accept),
    ];
    let first = NodeState::replay(&events).unwrap();
    let second = NodeState::replay(&events).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.status, NodeStatus::Accepted);
    assert_eq!(first.attempts_started, 3);
    let run_id = RunId::new("run-0001").unwrap();
    let node = receipt("x").node;
    assert_eq!(
        first.to_view(&run_id, &node),
        second.to_view(&run_id, &node)
    );
    for i in 1..events.len() {
        let mut reordered = events.clone();
        reordered.swap(i - 1, i);
        assert!(
            NodeState::replay(&reordered).is_err(),
            "swapping events {} and {} must break the replay",
            i - 1,
            i
        );
    }
}

#[test]
fn every_state_has_a_valid_view_once_consistent() {
    let run_id = RunId::new("run-0001").unwrap();
    let node = receipt("x").node;
    for (name, prefix) in prefixes() {
        let state = NodeState::replay(&prefix).unwrap();
        let view = state.to_view(&run_id, &node);
        if name == "gated_undecidable_no_packet" {
            continue;
        }
        assert_eq!(view.check_bindings(), vec![], "{name}");
        assert_eq!(view.status, state.status);
    }
}

#[test]
fn a_run_state_replays_the_store_event_log_into_views() {
    let dir = std::env::temp_dir().join(format!("checkspan-state-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut store = Store::open(dir.join("ledger.sqlite")).unwrap();
    let spec: GraphSpec = parse_record(&fixture("contracts/valid/graph-spec-full.json")).unwrap();
    store.store_graph(&spec).unwrap();
    let run_id = RunId::new("run-0001").unwrap();
    store
        .create_run(&GraphRun {
            record: RecordKind::GraphRun,
            schema_version: SchemaVersion(1),
            run_id: run_id.clone(),
            graph_ref: spec.graph_ref(),
            budget_lineage_ref: None,
        })
        .unwrap();
    let ci = NodeId::new("cs_ci").unwrap();
    let patch = NodeId::new("cs_patch").unwrap();
    store
        .transaction(|tx| {
            tx.append_event(
                &run_id,
                Some(&ci),
                kinds::ATTEMPT_DISPATCHED,
                &json!({"number": 1}),
            )?;
            Ok(())
        })
        .unwrap();
    let sealed: checkspan::contracts::Attempt = parse_record(&fixture(
        "outcomes/valid/attempt-completed-with-receipt.json",
    ))
    .unwrap();
    store.seal_attempt(&sealed).unwrap();
    let accept: checkspan::contracts::VerifierReceipt =
        parse_record(&fixture("outcomes/valid/receipt-accept.json")).unwrap();
    store.admit_receipt(&accept).unwrap();
    store
        .transaction(|tx| {
            tx.append_event(
                &run_id,
                Some(&patch),
                kinds::ATTEMPT_DISPATCHED,
                &json!({"number": 1}),
            )?;
            tx.append_event(&run_id, Some(&patch), kinds::NODE_CANCELLED, &json!({}))?;
            Ok(())
        })
        .unwrap();

    let events = store.events(&run_id).unwrap();
    let run = RunState::replay(run_id.clone(), &spec, &events).unwrap();
    let again = RunState::replay(run_id.clone(), &spec, &events).unwrap();
    assert_eq!(run, again);
    let views = run.views();
    assert_eq!(views.len(), 3);
    assert_eq!(views[0].node.node_id, patch);
    assert_eq!(views[0].status, NodeStatus::Cancelled);
    assert_eq!(views[1].node.node_id, ci);
    assert_eq!(views[1].node.revision.get(), 2);
    assert_eq!(views[1].status, NodeStatus::Accepted);
    assert_eq!(views[1].receipt.as_ref().unwrap().id.as_str(), "rcpt-ci-1");
    assert_eq!(
        views[2].status,
        NodeStatus::Open,
        "the untouched sink is open"
    );
    for view in &views {
        assert_eq!(view.check_bindings(), vec![], "{}", view.node);
    }

    // A receipt event recorded for a node the store never dispatched cannot
    // be replayed into acceptance.
    let mut broken = events.clone();
    broken.remove(1);
    let err = RunState::replay(run_id.clone(), &spec, &broken).unwrap_err();
    assert!(
        matches!(
            err,
            TransitionError::Illegal {
                from: NodeStatus::Open,
                ..
            }
        ),
        "{err}"
    );
    let mut unknown = events.clone();
    unknown[1].node_id = Some(NodeId::new("cs_ghost").unwrap());
    assert!(matches!(
        RunState::replay(run_id, &spec, &unknown).unwrap_err(),
        TransitionError::UnknownNode(_)
    ));
}
