//! Graph envelopes: a graph revision, the nodes it names, its required
//! targets, and one run of it.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::attempt::ReceiptRef;
use super::ids::{Digest, GraphId, NodeId, Revision, RunId, Timestamp};
use super::node::{NodeSpec, TypeRef};
use super::record::{Record, RecordKind, SchemaVersion};

/// Exact reference to one graph revision.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphRef {
    /// The graph.
    pub graph_id: GraphId,
    /// The graph revision.
    pub revision: Revision,
}

/// Exact reference to one node revision inside one graph.
///
/// Equality covers all three parts: the same `node_id` in another graph or at
/// another revision is a different node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeRef {
    /// The graph that owns the node.
    pub graph_id: GraphId,
    /// The graph-local node name.
    pub node_id: NodeId,
    /// The node's contract revision.
    pub revision: Revision,
}

impl fmt::Display for NodeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}@{}", self.graph_id, self.node_id, self.revision)
    }
}

/// Graph-wide limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphBudget {
    /// Maximum started attempts across the whole run, including failures.
    pub total_attempts: u32,
    /// Absolute deadline after which no new attempt may start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<Timestamp>,
}

/// One immutable revision of a graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphSpec {
    /// Always `graph_spec`.
    pub record: RecordKind,
    /// Always `1` for this build.
    pub schema_version: SchemaVersion,
    /// The graph.
    pub graph_id: GraphId,
    /// This revision of the graph.
    pub revision: Revision,
    /// Human-facing label; never part of identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// The nodes in this revision.
    pub nodes: Vec<NodeSpec>,
    /// Nodes that must all be accepted for a run to be complete.
    pub targets: Vec<NodeRef>,
    /// Graph-wide limits.
    pub budget: GraphBudget,
    /// The earlier revision of this graph that this one replaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<GraphRef>,
}

impl Record for GraphSpec {
    const KIND: RecordKind = RecordKind::GraphSpec;
    const VERSION: SchemaVersion = SchemaVersion(1);
    const SCHEMA_ID: &'static str = "https://checkspan.invalid/schemas/v1/graph-spec.schema.json";
}

/// An accepted receipt from another run, explicitly admitted to stand in for
/// one node of this run. Nothing from another run is reused without one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportedReceipt {
    /// The node of this graph the import satisfies.
    pub node_id: NodeId,
    /// The receipt, in the run that issued it.
    pub receipt: ReceiptRef,
    /// The result digest the receipt must have checked.
    pub result_digest: Digest,
    /// The result type the sealed attempt must declare.
    pub result_type: TypeRef,
    /// What the receipt is about.
    pub subject: String,
    /// When the operator admitted the import.
    pub admitted_at: Timestamp,
    /// Moment after which the import can no longer admit fresh work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<Timestamp>,
}

/// One execution of one graph revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphRun {
    /// Always `graph_run`.
    pub record: RecordKind,
    /// Always `1` for this build.
    pub schema_version: SchemaVersion,
    /// This run.
    pub run_id: RunId,
    /// The exact graph revision being run.
    pub graph_ref: GraphRef,
    /// Earlier run whose consumed budget this run inherits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_lineage_ref: Option<RunId>,
    /// Receipts from other runs admitted to stand in for nodes of this run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub admitted_imports: Vec<ImportedReceipt>,
}

impl Record for GraphRun {
    const KIND: RecordKind = RecordKind::GraphRun;
    const VERSION: SchemaVersion = SchemaVersion(1);
    const SCHEMA_ID: &'static str = "https://checkspan.invalid/schemas/v1/graph-run.schema.json";
}

/// A cross-field identity rule that a structurally valid record breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    /// Two nodes share one `id`.
    DuplicateNodeId(NodeId),
    /// `targets` is empty.
    NoTargets,
    /// The same target is listed twice.
    DuplicateTarget(NodeRef),
    /// A target names a different graph.
    ForeignTarget(NodeRef),
    /// A target names a node that is not in this graph.
    UnknownTarget(NodeRef),
    /// A target names a node at a revision other than the one in this graph.
    TargetRevisionMismatch {
        /// The target as written.
        target: NodeRef,
        /// The revision the node actually has here.
        actual: Revision,
    },
    /// `supersedes` names a different graph.
    SupersedesForeignGraph(GraphRef),
    /// `supersedes` does not name an earlier revision.
    SupersedesNotEarlier(GraphRef),
    /// `budget.total_attempts` is zero.
    ZeroAttemptBudget,
    /// A run names itself as its own budget lineage.
    SelfLineage(RunId),
    /// Two imports stand in for the same node.
    DuplicateImport(NodeId),
    /// An import names a receipt from this very run.
    SelfImport(NodeId),
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdentityError::DuplicateNodeId(id) => {
                write!(f, "node id {id:?} appears more than once")
            }
            IdentityError::NoTargets => f.write_str("graph declares no targets"),
            IdentityError::DuplicateTarget(t) => write!(f, "target {t} is listed more than once"),
            IdentityError::ForeignTarget(t) => write!(f, "target {t} names another graph"),
            IdentityError::UnknownTarget(t) => {
                write!(f, "target {t} is not a node in this graph")
            }
            IdentityError::TargetRevisionMismatch { target, actual } => write!(
                f,
                "target {target} does not match the node's revision {actual} in this graph"
            ),
            IdentityError::SupersedesForeignGraph(g) => write!(
                f,
                "supersedes names another graph {}@{}",
                g.graph_id, g.revision
            ),
            IdentityError::SupersedesNotEarlier(g) => write!(
                f,
                "supersedes revision {} is not earlier than this revision",
                g.revision
            ),
            IdentityError::ZeroAttemptBudget => {
                f.write_str("budget.total_attempts must be at least 1")
            }
            IdentityError::SelfLineage(r) => {
                write!(f, "run {r} names itself as budget lineage")
            }
            IdentityError::DuplicateImport(n) => {
                write!(f, "node {n} has more than one admitted import")
            }
            IdentityError::SelfImport(n) => {
                write!(f, "the import for node {n} names a receipt from this run")
            }
        }
    }
}

impl std::error::Error for IdentityError {}

impl GraphSpec {
    /// Exact reference to this graph revision.
    pub fn graph_ref(&self) -> GraphRef {
        GraphRef {
            graph_id: self.graph_id.clone(),
            revision: self.revision,
        }
    }

    /// Full reference to a node of this graph, if it is present.
    pub fn node_ref(&self, node_id: &NodeId) -> Option<NodeRef> {
        self.nodes
            .iter()
            .find(|n| &n.id == node_id)
            .map(|n| NodeRef {
                graph_id: self.graph_id.clone(),
                node_id: n.id.clone(),
                revision: n.revision,
            })
    }

    /// Check every cross-field identity rule, returning all violations in
    /// document order.
    pub fn check_identity(&self) -> Vec<IdentityError> {
        let mut errors = Vec::new();
        let mut seen = HashSet::new();
        for node in &self.nodes {
            if !seen.insert(&node.id) {
                errors.push(IdentityError::DuplicateNodeId(node.id.clone()));
            }
        }
        if self.targets.is_empty() {
            errors.push(IdentityError::NoTargets);
        }
        let mut seen_targets = HashSet::new();
        for target in &self.targets {
            if !seen_targets.insert(target) {
                errors.push(IdentityError::DuplicateTarget(target.clone()));
                continue;
            }
            if target.graph_id != self.graph_id {
                errors.push(IdentityError::ForeignTarget(target.clone()));
                continue;
            }
            match self.nodes.iter().find(|n| n.id == target.node_id) {
                None => errors.push(IdentityError::UnknownTarget(target.clone())),
                Some(node) if node.revision != target.revision => {
                    errors.push(IdentityError::TargetRevisionMismatch {
                        target: target.clone(),
                        actual: node.revision,
                    });
                }
                Some(_) => {}
            }
        }
        if let Some(prev) = &self.supersedes {
            if prev.graph_id != self.graph_id {
                errors.push(IdentityError::SupersedesForeignGraph(prev.clone()));
            } else if prev.revision >= self.revision {
                errors.push(IdentityError::SupersedesNotEarlier(prev.clone()));
            }
        }
        if self.budget.total_attempts == 0 {
            errors.push(IdentityError::ZeroAttemptBudget);
        }
        errors
    }
}

impl GraphRun {
    /// Check every cross-field identity rule.
    pub fn check_identity(&self) -> Vec<IdentityError> {
        let mut errors = Vec::new();
        if self.budget_lineage_ref.as_ref() == Some(&self.run_id) {
            errors.push(IdentityError::SelfLineage(self.run_id.clone()));
        }
        let mut seen = HashSet::new();
        for import in &self.admitted_imports {
            if !seen.insert(&import.node_id) {
                errors.push(IdentityError::DuplicateImport(import.node_id.clone()));
            }
            if import.receipt.run_id == self.run_id {
                errors.push(IdentityError::SelfImport(import.node_id.clone()));
            }
        }
        errors
    }
}
