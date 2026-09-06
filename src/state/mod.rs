//! The pure state reducer: every legal node transition in one place.
//!
//! A node's state is a fold over its ordered events. The reducer never reads
//! a store or a clock; it takes a state and an event and returns the next
//! state or a `TransitionError` that leaves the state unchanged. Only a
//! `ReceiptAdmitted` event with an `accept` verdict reaches `accepted`; no
//! worker event, execution outcome, or gate decision can. A timeout or crash
//! is `failed`, never `rejected`. `accepted` and `cancelled` are terminal.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contracts::{
    AttemptNumber, DecisionKind, Digest, ExecutionOutcome, GraphSpec, Ident, NodeId, NodeRef,
    NodeStatus, NodeView, ReceiptRef, RecordKind, RunId, SchemaVersion, Verdict,
};
use crate::store::StoredEvent;

/// Event kind names as recorded in the store's event log.
pub mod kinds {
    /// A run was created; run-level, ignored by node reducers.
    pub const RUN_CREATED: &str = "run_created";
    /// An attempt was claimed and dispatched.
    pub const ATTEMPT_DISPATCHED: &str = "attempt_dispatched";
    /// An attempt's execution ended and its record was sealed.
    pub const ATTEMPT_SEALED: &str = "attempt_sealed";
    /// A verifier receipt was admitted for the current attempt.
    pub const RECEIPT_ADMITTED: &str = "receipt_admitted";
    /// A gate packet was opened for the node.
    pub const GATE_OPENED: &str = "gate_opened";
    /// An authenticated decision was admitted for the node's gate packet.
    pub const DECISION_ADMITTED: &str = "decision_admitted";
    /// Policy allowed another attempt after a rejection or failure.
    pub const RETRY_ALLOWED: &str = "retry_allowed";
    /// The node was cancelled.
    pub const NODE_CANCELLED: &str = "node_cancelled";
}

/// A typed event about one node within one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum NodeEvent {
    /// The controller claimed and dispatched attempt `attempt`.
    Dispatched {
        /// Must be exactly one more than the attempts started so far.
        attempt: AttemptNumber,
    },
    /// Execution of `attempt` ended.
    ExecutionFinished {
        /// The attempt that finished.
        attempt: AttemptNumber,
        /// How it ended; never a verdict.
        outcome: ExecutionOutcome,
        /// Digest of the result, when execution produced one.
        result_digest: Option<Digest>,
    },
    /// A verifier receipt was admitted for `attempt`.
    ReceiptAdmitted {
        /// The attempt checked.
        attempt: AttemptNumber,
        /// The receipt.
        receipt: ReceiptRef,
        /// The verdict.
        verdict: Verdict,
    },
    /// A gate packet was opened for the node.
    GateOpened {
        /// The packet.
        packet: Ident,
    },
    /// An authenticated decision was admitted for the node's packet.
    DecisionAdmitted {
        /// The packet decided.
        packet: Ident,
        /// The decision.
        decision: DecisionKind,
    },
    /// Policy allowed another attempt.
    RetryAllowed,
    /// The node was cancelled.
    Cancelled,
}

impl NodeEvent {
    /// The store event kind this event is recorded as.
    pub fn kind(&self) -> &'static str {
        match self {
            NodeEvent::Dispatched { .. } => kinds::ATTEMPT_DISPATCHED,
            NodeEvent::ExecutionFinished { .. } => kinds::ATTEMPT_SEALED,
            NodeEvent::ReceiptAdmitted { .. } => kinds::RECEIPT_ADMITTED,
            NodeEvent::GateOpened { .. } => kinds::GATE_OPENED,
            NodeEvent::DecisionAdmitted { .. } => kinds::DECISION_ADMITTED,
            NodeEvent::RetryAllowed => kinds::RETRY_ALLOWED,
            NodeEvent::Cancelled => kinds::NODE_CANCELLED,
        }
    }

    /// The store payload for this event.
    pub fn payload(&self) -> serde_json::Value {
        match self {
            NodeEvent::Dispatched { attempt } => serde_json::json!({ "number": attempt }),
            NodeEvent::ExecutionFinished {
                attempt,
                outcome,
                result_digest,
            } => serde_json::json!({
                "number": attempt,
                "outcome": outcome,
                "result_digest": result_digest,
            }),
            NodeEvent::ReceiptAdmitted {
                attempt,
                receipt,
                verdict,
            } => serde_json::json!({
                "number": attempt,
                "receipt_id": receipt.id,
                "verdict": verdict,
            }),
            NodeEvent::GateOpened { packet } => serde_json::json!({ "packet_id": packet }),
            NodeEvent::DecisionAdmitted { packet, decision } => serde_json::json!({
                "packet_id": packet,
                "decision": decision,
            }),
            NodeEvent::RetryAllowed | NodeEvent::Cancelled => serde_json::json!({}),
        }
    }

    /// Decode a node-level stored event. `node` is the graph's reference for
    /// the event's node; run-level events (no `node_id`) are the caller's to
    /// skip.
    pub fn from_stored(event: &StoredEvent, node: &NodeRef) -> Result<NodeEvent, TransitionError> {
        let p = &event.payload;
        let field = |name: &str| {
            p.get(name)
                .filter(|v| !v.is_null())
                .ok_or_else(|| TransitionError::MalformedEvent {
                    seq: event.seq,
                    kind: event.kind.clone(),
                    reason: format!("missing {name}"),
                })
        };
        let malformed = |reason: String| TransitionError::MalformedEvent {
            seq: event.seq,
            kind: event.kind.clone(),
            reason,
        };
        let number = |name: &str| -> Result<AttemptNumber, TransitionError> {
            serde_json::from_value(field(name)?.clone()).map_err(|e| malformed(e.to_string()))
        };
        let decoded = match event.kind.as_str() {
            kinds::ATTEMPT_DISPATCHED => NodeEvent::Dispatched {
                attempt: number("number")?,
            },
            kinds::ATTEMPT_SEALED => NodeEvent::ExecutionFinished {
                attempt: number("number")?,
                outcome: serde_json::from_value(field("outcome")?.clone())
                    .map_err(|e| malformed(e.to_string()))?,
                result_digest: p
                    .get("result_digest")
                    .filter(|v| !v.is_null())
                    .map(|v| serde_json::from_value(v.clone()))
                    .transpose()
                    .map_err(|e| malformed(e.to_string()))?,
            },
            kinds::RECEIPT_ADMITTED => NodeEvent::ReceiptAdmitted {
                attempt: number("number")?,
                receipt: ReceiptRef {
                    id: serde_json::from_value(field("receipt_id")?.clone())
                        .map_err(|e| malformed(e.to_string()))?,
                    run_id: event.run_id.clone(),
                    node: node.clone(),
                },
                verdict: serde_json::from_value(field("verdict")?.clone())
                    .map_err(|e| malformed(e.to_string()))?,
            },
            kinds::GATE_OPENED => NodeEvent::GateOpened {
                packet: serde_json::from_value(field("packet_id")?.clone())
                    .map_err(|e| malformed(e.to_string()))?,
            },
            kinds::DECISION_ADMITTED => NodeEvent::DecisionAdmitted {
                packet: serde_json::from_value(field("packet_id")?.clone())
                    .map_err(|e| malformed(e.to_string()))?,
                decision: serde_json::from_value(field("decision")?.clone())
                    .map_err(|e| malformed(e.to_string()))?,
            },
            kinds::RETRY_ALLOWED => NodeEvent::RetryAllowed,
            kinds::NODE_CANCELLED => NodeEvent::Cancelled,
            other => {
                return Err(TransitionError::UnknownEvent {
                    seq: event.seq,
                    kind: other.to_string(),
                });
            }
        };
        Ok(decoded)
    }
}

/// Why an event is not legal in the current state. The state is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    /// The event is not allowed from this status.
    Illegal {
        /// The status the node was in.
        from: NodeStatus,
        /// The event kind.
        event: &'static str,
        /// Why.
        reason: String,
    },
    /// The event names an attempt other than the current one.
    StaleAttempt {
        /// The attempt the event names.
        event_attempt: AttemptNumber,
        /// The current attempt.
        current: Option<AttemptNumber>,
    },
    /// A dispatched attempt number is not the next one.
    WrongAttemptNumber {
        /// The number dispatched.
        dispatched: AttemptNumber,
        /// The number expected.
        expected: u32,
    },
    /// A decision names a packet other than the one the node waits on.
    WrongPacket {
        /// The packet decided.
        decided: Ident,
        /// The packet the node waits on.
        waiting_on: Option<Ident>,
    },
    /// A stored event kind the reducer does not know.
    UnknownEvent {
        /// Its sequence number.
        seq: u64,
        /// Its kind.
        kind: String,
    },
    /// A stored event with a missing or ill-typed payload field.
    MalformedEvent {
        /// Its sequence number.
        seq: u64,
        /// Its kind.
        kind: String,
        /// What was wrong.
        reason: String,
    },
    /// An event names a node that is not in the graph.
    UnknownNode(NodeId),
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransitionError::Illegal {
                from,
                event,
                reason,
            } => write!(f, "{event} is not legal from {from}: {reason}"),
            TransitionError::StaleAttempt {
                event_attempt,
                current,
            } => write!(
                f,
                "event is about attempt {event_attempt} but the current attempt is {}",
                current.map_or("none".to_string(), |c| c.to_string())
            ),
            TransitionError::WrongAttemptNumber {
                dispatched,
                expected,
            } => write!(
                f,
                "dispatched attempt {dispatched} but the next attempt is {expected}"
            ),
            TransitionError::WrongPacket {
                decided,
                waiting_on,
            } => write!(
                f,
                "decision is for packet {decided} but the node waits on {}",
                waiting_on
                    .as_ref()
                    .map_or("no packet".to_string(), |p| p.to_string())
            ),
            TransitionError::UnknownEvent { seq, kind } => {
                write!(f, "event {seq} has unknown kind {kind:?}")
            }
            TransitionError::MalformedEvent { seq, kind, reason } => {
                write!(f, "event {seq} ({kind}) is malformed: {reason}")
            }
            TransitionError::UnknownNode(id) => write!(f, "node {id} is not in the graph"),
        }
    }
}

impl std::error::Error for TransitionError {}

/// The reducer's state for one node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeState {
    /// Current status.
    pub status: NodeStatus,
    /// The attempt the status describes.
    pub current_attempt: Option<AttemptNumber>,
    /// Attempts started so far, including failures.
    pub attempts_started: u32,
    /// Whether the current attempt's execution finished and is being checked.
    pub checking: bool,
    /// The receipt that determined an `accepted` or `rejected` status.
    pub receipt: Option<ReceiptRef>,
    /// The gate packet a `gated` node waits on, once it is opened.
    pub waiting_on_gate: Option<Ident>,
}

impl Default for NodeState {
    fn default() -> Self {
        NodeState {
            status: NodeStatus::Open,
            current_attempt: None,
            attempts_started: 0,
            checking: false,
            receipt: None,
            waiting_on_gate: None,
        }
    }
}

impl NodeState {
    /// The initial state: open, no attempts.
    pub fn open() -> Self {
        Self::default()
    }

    /// Whether no further event can ever apply.
    pub fn is_terminal(&self) -> bool {
        matches!(self.status, NodeStatus::Accepted | NodeStatus::Cancelled)
    }

    fn illegal(&self, event: &NodeEvent, reason: &str) -> TransitionError {
        TransitionError::Illegal {
            from: self.status,
            event: event.kind(),
            reason: reason.to_string(),
        }
    }

    fn require_current(
        &self,
        event: &NodeEvent,
        attempt: AttemptNumber,
    ) -> Result<(), TransitionError> {
        if self.current_attempt == Some(attempt) {
            Ok(())
        } else if self.is_terminal() {
            Err(self.illegal(event, "the node is terminal"))
        } else {
            Err(TransitionError::StaleAttempt {
                event_attempt: attempt,
                current: self.current_attempt,
            })
        }
    }

    /// Apply one event, returning the next state. On error the state is
    /// unchanged and the error says why.
    pub fn apply(&self, event: &NodeEvent) -> Result<NodeState, TransitionError> {
        use NodeStatus::*;
        let mut next = self.clone();
        match (self.status, event) {
            (Accepted, _) => return Err(self.illegal(event, "accepted is terminal")),
            (Cancelled, _) => return Err(self.illegal(event, "cancelled is terminal")),

            (Open, NodeEvent::Dispatched { attempt }) => {
                let expected = self.attempts_started + 1;
                if attempt.get() != expected {
                    return Err(TransitionError::WrongAttemptNumber {
                        dispatched: *attempt,
                        expected,
                    });
                }
                next.status = Running;
                next.current_attempt = Some(*attempt);
                next.attempts_started = expected;
                next.checking = false;
                next.receipt = None;
                next.waiting_on_gate = None;
            }
            (Open, NodeEvent::Cancelled) => {
                next.status = Cancelled;
                next.waiting_on_gate = None;
            }
            (Open, _) => {
                return Err(self.illegal(event, "an open node can only be dispatched or cancelled"));
            }

            (
                Running,
                NodeEvent::ExecutionFinished {
                    attempt, outcome, ..
                },
            ) => {
                self.require_current(event, *attempt)?;
                if self.checking {
                    return Err(self.illegal(event, "execution of this attempt already finished"));
                }
                match outcome {
                    ExecutionOutcome::Completed => next.checking = true,
                    ExecutionOutcome::Failed | ExecutionOutcome::TimedOut => next.status = Failed,
                    ExecutionOutcome::Cancelled => next.status = Cancelled,
                }
            }
            (
                Running,
                NodeEvent::ReceiptAdmitted {
                    attempt,
                    receipt,
                    verdict,
                },
            ) => {
                self.require_current(event, *attempt)?;
                if !self.checking {
                    return Err(self.illegal(event, "no verdict before execution finishes"));
                }
                next.checking = false;
                match verdict {
                    Verdict::Accept => {
                        next.status = Accepted;
                        next.receipt = Some(receipt.clone());
                    }
                    Verdict::Reject => {
                        next.status = Rejected;
                        next.receipt = Some(receipt.clone());
                    }
                    Verdict::Undecidable => {
                        next.status = Gated;
                        next.receipt = None;
                        next.waiting_on_gate = None;
                    }
                }
            }
            (Running, NodeEvent::Cancelled) => {
                next.status = Cancelled;
                next.checking = false;
            }
            (Running, _) => {
                return Err(self.illegal(
                    event,
                    "a running node can only finish, be checked, or be cancelled",
                ));
            }

            (Rejected | Failed, NodeEvent::RetryAllowed) => {
                next.status = Open;
                next.current_attempt = None;
                next.receipt = None;
            }
            (Rejected | Failed, NodeEvent::GateOpened { packet }) => {
                next.status = Gated;
                next.receipt = None;
                next.waiting_on_gate = Some(packet.clone());
            }
            (Rejected | Failed, NodeEvent::Cancelled) => {
                next.status = Cancelled;
            }
            (Rejected | Failed, NodeEvent::Dispatched { .. }) => {
                return Err(self.illegal(
                    event,
                    "another attempt needs a retry_allowed event under policy",
                ));
            }
            (Rejected | Failed, _) => {
                return Err(self.illegal(event, "not legal after a verdict or failure"));
            }

            (Gated, NodeEvent::GateOpened { packet }) => {
                if self.waiting_on_gate.is_some() {
                    return Err(self.illegal(event, "a packet is already open for this node"));
                }
                next.waiting_on_gate = Some(packet.clone());
            }
            (Gated, NodeEvent::DecisionAdmitted { packet, decision }) => {
                if self.waiting_on_gate.as_ref() != Some(packet) {
                    return Err(TransitionError::WrongPacket {
                        decided: packet.clone(),
                        waiting_on: self.waiting_on_gate.clone(),
                    });
                }
                next.waiting_on_gate = None;
                next.receipt = None;
                match decision {
                    DecisionKind::Retry
                    | DecisionKind::ApproveAction
                    | DecisionKind::DenyAction
                    | DecisionKind::RecordAssessment => {
                        next.status = Open;
                        next.current_attempt = None;
                    }
                    DecisionKind::Cancel | DecisionKind::Revise => {
                        next.status = Cancelled;
                    }
                }
            }
            (Gated, NodeEvent::Cancelled) => {
                next.status = Cancelled;
                next.waiting_on_gate = None;
            }
            (Gated, _) => return Err(self.illegal(event, "a gated node waits for its decision")),
        }
        Ok(next)
    }

    /// Fold `events` from the open state.
    pub fn replay<'a>(
        events: impl IntoIterator<Item = &'a NodeEvent>,
    ) -> Result<NodeState, TransitionError> {
        events
            .into_iter()
            .try_fold(NodeState::open(), |state, event| state.apply(event))
    }

    /// The derived view. `blocked_reason` is left for the scheduler, which
    /// knows about dependencies and budgets.
    pub fn to_view(&self, run_id: &RunId, node: &NodeRef) -> NodeView {
        NodeView {
            record: RecordKind::NodeView,
            schema_version: SchemaVersion(1),
            run_id: run_id.clone(),
            node: node.clone(),
            status: self.status,
            current_attempt: self.current_attempt,
            receipt: self.receipt.clone(),
            blocked_reason: None,
            waiting_on_gate: self.waiting_on_gate.clone(),
        }
    }
}

/// Reducer state for every node of one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunState {
    /// The run.
    pub run_id: RunId,
    /// Node references from the graph revision, in document order.
    pub nodes: Vec<NodeRef>,
    /// State per node.
    pub states: BTreeMap<NodeId, NodeState>,
}

impl RunState {
    /// Every node open, no attempts.
    pub fn new(run_id: RunId, spec: &GraphSpec) -> Self {
        let nodes: Vec<NodeRef> = spec
            .nodes
            .iter()
            .map(|n| NodeRef {
                graph_id: spec.graph_id.clone(),
                node_id: n.id.clone(),
                revision: n.revision,
            })
            .collect();
        let states = nodes
            .iter()
            .map(|n| (n.node_id.clone(), NodeState::open()))
            .collect();
        RunState {
            run_id,
            nodes,
            states,
        }
    }

    /// Apply one node event.
    pub fn apply(&mut self, node_id: &NodeId, event: &NodeEvent) -> Result<(), TransitionError> {
        let state = self
            .states
            .get(node_id)
            .ok_or_else(|| TransitionError::UnknownNode(node_id.clone()))?;
        let next = state.apply(event)?;
        self.states.insert(node_id.clone(), next);
        Ok(())
    }

    /// Fold a run's stored event log. Run-level events are skipped; the
    /// first illegal, unknown, or malformed event stops the replay.
    pub fn replay(
        run_id: RunId,
        spec: &GraphSpec,
        events: &[StoredEvent],
    ) -> Result<RunState, TransitionError> {
        let mut run = RunState::new(run_id, spec);
        for stored in events {
            let Some(node_id) = &stored.node_id else {
                continue;
            };
            let node = run
                .nodes
                .iter()
                .find(|n| &n.node_id == node_id)
                .cloned()
                .ok_or_else(|| TransitionError::UnknownNode(node_id.clone()))?;
            let event = NodeEvent::from_stored(stored, &node)?;
            run.apply(node_id, &event)?;
        }
        Ok(run)
    }

    /// The state of one node.
    pub fn node(&self, node_id: &NodeId) -> Option<&NodeState> {
        self.states.get(node_id)
    }

    /// Derived views for every node, in graph document order.
    pub fn views(&self) -> Vec<NodeView> {
        self.nodes
            .iter()
            .map(|node| {
                self.states
                    .get(&node.node_id)
                    .map(|state| state.to_view(&self.run_id, node))
                    .expect("every graph node has a state")
            })
            .collect()
    }
}
