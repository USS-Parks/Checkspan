//! Derived, read-only status of one node within one run.
//!
//! A view is a projection of attempts and receipts. It cannot say `accepted`
//! or `rejected` without naming the receipt that determined it, and a
//! receipt about another node or run cannot be borrowed.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::attempt::ReceiptRef;
use super::graph::NodeRef;
use super::ids::{AttemptNumber, Ident, RunId};
use super::record::{Record, RecordKind, SchemaVersion};

/// Node status within a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    /// Not running; may be blocked on prerequisites, budget, or a gate.
    Open,
    /// An attempt is executing or being checked.
    Running,
    /// A contract-matching verifier receipt accepted the current attempt.
    Accepted,
    /// The verifier completed and rejected the current attempt.
    Rejected,
    /// The current attempt ended in an operational failure.
    Failed,
    /// Waiting on a human gate packet.
    Gated,
    /// Cancelled; no further attempts.
    Cancelled,
}

impl NodeStatus {
    /// Every status, for coverage checks.
    pub const ALL: [NodeStatus; 7] = [
        NodeStatus::Open,
        NodeStatus::Running,
        NodeStatus::Accepted,
        NodeStatus::Rejected,
        NodeStatus::Failed,
        NodeStatus::Gated,
        NodeStatus::Cancelled,
    ];

    /// The `status` field text.
    pub fn as_str(self) -> &'static str {
        match self {
            NodeStatus::Open => "open",
            NodeStatus::Running => "running",
            NodeStatus::Accepted => "accepted",
            NodeStatus::Rejected => "rejected",
            NodeStatus::Failed => "failed",
            NodeStatus::Gated => "gated",
            NodeStatus::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Derived status of one node within one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeView {
    /// Always `node_view`.
    pub record: RecordKind,
    /// Always `1` for this build.
    pub schema_version: SchemaVersion,
    /// The run.
    pub run_id: RunId,
    /// The node.
    pub node: NodeRef,
    /// Current status.
    pub status: NodeStatus,
    /// The attempt the status describes; required unless `open` or
    /// `cancelled` before any attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_attempt: Option<AttemptNumber>,
    /// The verifier receipt that determined an `accepted` or `rejected`
    /// status; required for those and forbidden otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ReceiptRef>,
    /// Why an `open` node cannot be dispatched yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
    /// The gate packet a `gated` node waits on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_on_gate: Option<Ident>,
}

impl Record for NodeView {
    const KIND: RecordKind = RecordKind::NodeView;
    const VERSION: SchemaVersion = SchemaVersion(1);
    const SCHEMA_ID: &'static str = "https://checkspan.invalid/schemas/v1/node-view.schema.json";
}

/// A rule that a structurally valid view breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewError {
    /// `accepted` or `rejected` without the receipt that determined it.
    ReceiptRequired(NodeStatus),
    /// A receipt on a status that no receipt determines.
    ReceiptNotAllowed(NodeStatus),
    /// The receipt is about another node.
    ReceiptForOtherNode(ReceiptRef),
    /// The receipt was issued in another run.
    ReceiptForOtherRun(ReceiptRef),
    /// The status describes an attempt but none is named.
    AttemptRequired(NodeStatus),
    /// `gated` without the packet being waited on.
    GateRequired,
    /// A gate reference on a status that is not `gated`.
    GateNotAllowed(NodeStatus),
    /// A blocked reason on a status that is not `open`.
    BlockedReasonNotAllowed(NodeStatus),
}

impl fmt::Display for ViewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ViewError::ReceiptRequired(s) => {
                write!(f, "status {s} requires the receipt that determined it")
            }
            ViewError::ReceiptNotAllowed(s) => write!(f, "status {s} cannot carry a receipt"),
            ViewError::ReceiptForOtherNode(r) => write!(f, "receipt {r} is about another node"),
            ViewError::ReceiptForOtherRun(r) => {
                write!(f, "receipt {r} was issued in another run")
            }
            ViewError::AttemptRequired(s) => write!(f, "status {s} requires a current attempt"),
            ViewError::GateRequired => f.write_str("status gated requires waiting_on_gate"),
            ViewError::GateNotAllowed(s) => write!(f, "status {s} cannot wait on a gate"),
            ViewError::BlockedReasonNotAllowed(s) => {
                write!(f, "status {s} cannot carry a blocked_reason")
            }
        }
    }
}

impl std::error::Error for ViewError {}

impl NodeView {
    /// Check every rule that concerns this record alone.
    pub fn check_bindings(&self) -> Vec<ViewError> {
        let mut errors = Vec::new();
        let status = self.status;
        let needs_receipt = matches!(status, NodeStatus::Accepted | NodeStatus::Rejected);
        match (&self.receipt, needs_receipt) {
            (None, true) => errors.push(ViewError::ReceiptRequired(status)),
            (Some(_), false) => errors.push(ViewError::ReceiptNotAllowed(status)),
            (Some(receipt), true) => {
                if receipt.node != self.node {
                    errors.push(ViewError::ReceiptForOtherNode(receipt.clone()));
                }
                if receipt.run_id != self.run_id {
                    errors.push(ViewError::ReceiptForOtherRun(receipt.clone()));
                }
            }
            (None, false) => {}
        }
        let needs_attempt = matches!(
            status,
            NodeStatus::Running
                | NodeStatus::Accepted
                | NodeStatus::Rejected
                | NodeStatus::Failed
                | NodeStatus::Gated
        );
        if needs_attempt && self.current_attempt.is_none() {
            errors.push(ViewError::AttemptRequired(status));
        }
        match (status, &self.waiting_on_gate) {
            (NodeStatus::Gated, None) => errors.push(ViewError::GateRequired),
            (other, Some(_)) if other != NodeStatus::Gated => {
                errors.push(ViewError::GateNotAllowed(other));
            }
            _ => {}
        }
        if status != NodeStatus::Open && self.blocked_reason.is_some() {
            errors.push(ViewError::BlockedReasonNotAllowed(status));
        }
        errors
    }
}
