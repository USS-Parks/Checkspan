//! Node contracts: what a node must deliver, what it may consume, and the
//! policies that bound its attempts.
//!
//! A well-formed contract is only a shape. Nothing here records or implies
//! acceptance; verdicts come from verifier receipts bound to an attempt.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::graph::NodeRef;
use super::ids::{Digest, Exactly, Ident, NodeId, Revision, Version};

/// What a node does in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Produces a candidate artifact for a verifier to check.
    Task,
    /// Verifies an upstream result; needs at least one dependency.
    Check,
    /// Records an authenticated human decision; exposes exactly one `human`
    /// evidence port.
    HumanGate,
    /// Assembles accepted upstream receipts into a typed packet; needs at
    /// least one dependency.
    Sink,
}

impl NodeKind {
    /// The `kind` field text.
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Task => "task",
            NodeKind::Check => "check",
            NodeKind::HumanGate => "human_gate",
            NodeKind::Sink => "sink",
        }
    }
}

impl fmt::Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Evidence a port may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortKind {
    /// Admitted corpus snapshot, scoped retrieval trace, and exact spans.
    Rag,
    /// Gate packet and authenticated decision.
    Human,
    /// Run, check, and artifact references with digests.
    Logs,
    /// Repository identity and exact commit, tree, or patch digest.
    Code,
    /// Exact accepted node revision, receipt, and typed output.
    Proofs,
}

/// Pinned result schema: which typed output a node produces or expects.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeRef {
    /// Result schema name, such as `patch_result`.
    pub schema_id: Ident,
    /// Result schema version.
    pub version: Version,
    /// Digest of the result schema text.
    pub digest: Digest,
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.schema_id, self.version)
    }
}

/// Pinned verifier identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierRef {
    /// Verifier name.
    pub id: Ident,
    /// Verifier version.
    pub version: Version,
    /// Digest of the verifier code or profile.
    pub digest: Digest,
}

/// Reference to a versioned policy.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRef {
    /// Policy name.
    pub id: Ident,
    /// Policy version.
    pub version: Version,
}

impl fmt::Display for PolicyRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.id, self.version)
    }
}

/// Typed dependency on another node's output port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    /// The exact node revision depended on.
    pub node: NodeRef,
    /// Which of its output ports is consumed.
    pub output_port: Ident,
    /// The result type the consumer requires on that port.
    pub expected_type: TypeRef,
}

/// Declared evidence input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortSpec {
    /// Port name, unique within the node.
    pub name: Ident,
    /// What kind of evidence the port carries.
    pub kind: PortKind,
    /// The evidence record type the port accepts.
    pub expected_type: TypeRef,
    /// Whether an attempt may be dispatched without this evidence.
    pub required: bool,
    /// Sources the resolver may draw from. Declaration alone does not enforce
    /// the boundary; the resolver does.
    pub allowed_source_scope: Vec<String>,
    /// How resolved evidence is handled and retained.
    pub handling_policy_ref: PolicyRef,
}

/// The pinned acceptance contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceSpec {
    /// What acceptance establishes, and nothing more.
    pub claim: String,
    /// Checks the verifier must run. A retry can never drop one.
    pub required_checks: Vec<Ident>,
    /// The only verifier whose receipt can accept this node.
    pub verifier: VerifierRef,
    /// Verification policy the receipt must be issued under.
    pub policy_ref: PolicyRef,
}

/// Failure classes a retry policy may allow another attempt for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    /// The verifier completed and rejected the candidate.
    Rejected,
    /// The worker or verifier crashed or a required service was unavailable.
    Infrastructure,
    /// The attempt exceeded its time limit.
    Timeout,
}

/// What happens when a node's attempts run out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnExhaustion {
    /// Create a human gate packet.
    Gate,
    /// Cancel the node and its dependents.
    Cancel,
}

/// Bounded retry behaviour for one node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryPolicy {
    /// Retry policy format version; only `1` is understood.
    pub version: Exactly<1>,
    /// Attempts allowed in total, including the first and every failure.
    pub max_attempts: u32,
    /// Failure classes that permit another attempt.
    pub allowed_failure_classes: Vec<FailureClass>,
    /// Routing when `max_attempts` is reached.
    pub on_exhaustion: OnExhaustion,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            version: Exactly,
            max_attempts: 3,
            allowed_failure_classes: vec![
                FailureClass::Rejected,
                FailureClass::Infrastructure,
                FailureClass::Timeout,
            ],
            on_exhaustion: OnExhaustion::Gate,
        }
    }
}

/// How a node uses a named resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceAccess {
    /// No other attempt may hold the resource at the same time.
    Exclusive,
    /// Other shared holders may run concurrently.
    Shared,
}

/// A resource the node needs while an attempt runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceClaim {
    /// Resource name, such as a checkout or a model endpoint.
    pub resource: Ident,
    /// Required access mode.
    pub access: ResourceAccess,
}

/// One immutable node contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeSpec {
    /// Graph-local name.
    pub id: NodeId,
    /// Contract revision.
    pub revision: Revision,
    /// What the node does.
    pub kind: NodeKind,
    /// Human-facing label; never part of identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// The intent given to the worker.
    pub prompt: String,
    /// Typed outputs of other nodes this node consumes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deps: Vec<Dependency>,
    /// Evidence inputs the node may pull.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_ports: Vec<PortSpec>,
    /// The typed result the node produces on its `result` port.
    pub result_type: TypeRef,
    /// The pinned acceptance contract.
    pub acceptance: AcceptanceSpec,
    /// Bounded retry behaviour; defaults to three attempts, any failure class,
    /// gate on exhaustion.
    #[serde(default)]
    pub retry_policy: RetryPolicy,
    /// Policy governing which privileged actions the node may request.
    pub authority_policy_ref: PolicyRef,
    /// Resources an attempt holds while it runs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_scope: Vec<ResourceClaim>,
    /// The earlier revision of this node that this contract replaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<NodeRef>,
}

/// A contract rule that a structurally valid node breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeContractError {
    /// `prompt` is empty.
    EmptyPrompt,
    /// `acceptance.required_checks` is empty.
    NoRequiredChecks,
    /// A required check is listed twice.
    DuplicateRequiredCheck(Ident),
    /// The same node output port is depended on twice.
    DuplicateDependency {
        /// The node depended on.
        node: NodeRef,
        /// The port depended on.
        output_port: Ident,
    },
    /// Two evidence ports share a name.
    DuplicatePortName(Ident),
    /// An evidence port allows no sources at all.
    EmptySourceScope(Ident),
    /// A resource is claimed twice.
    DuplicateResource(Ident),
    /// A `check` or `sink` node has no dependencies.
    KindRequiresDependencies(NodeKind),
    /// A `human_gate` node does not have exactly one `human` port.
    HumanGateNeedsOneHumanPort {
        /// How many `human` ports were declared.
        found: usize,
    },
    /// A node that is not a `human_gate` declares a `human` port.
    HumanPortOnNonGate {
        /// The node kind.
        kind: NodeKind,
        /// The offending port.
        port: Ident,
    },
    /// `retry_policy.max_attempts` is zero.
    ZeroMaxAttempts,
    /// `supersedes` names a different node.
    SupersedesDifferentNode(NodeRef),
    /// `supersedes` does not name an earlier revision.
    SupersedesNotEarlier(NodeRef),
}

impl fmt::Display for NodeContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeContractError::EmptyPrompt => f.write_str("prompt is empty"),
            NodeContractError::NoRequiredChecks => {
                f.write_str("acceptance.required_checks is empty")
            }
            NodeContractError::DuplicateRequiredCheck(c) => {
                write!(f, "required check {c:?} is listed more than once")
            }
            NodeContractError::DuplicateDependency { node, output_port } => {
                write!(
                    f,
                    "dependency on {node} port {output_port:?} is listed more than once"
                )
            }
            NodeContractError::DuplicatePortName(p) => {
                write!(f, "evidence port {p:?} is declared more than once")
            }
            NodeContractError::EmptySourceScope(p) => {
                write!(f, "evidence port {p:?} allows no sources")
            }
            NodeContractError::DuplicateResource(r) => {
                write!(f, "resource {r:?} is claimed more than once")
            }
            NodeContractError::KindRequiresDependencies(k) => {
                write!(f, "a {k} node needs at least one dependency")
            }
            NodeContractError::HumanGateNeedsOneHumanPort { found } => write!(
                f,
                "a human_gate node needs exactly one human evidence port, found {found}"
            ),
            NodeContractError::HumanPortOnNonGate { kind, port } => {
                write!(
                    f,
                    "a {kind} node cannot declare human evidence port {port:?}"
                )
            }
            NodeContractError::ZeroMaxAttempts => {
                f.write_str("retry_policy.max_attempts must be at least 1")
            }
            NodeContractError::SupersedesDifferentNode(n) => {
                write!(f, "supersedes names a different node {n}")
            }
            NodeContractError::SupersedesNotEarlier(n) => write!(
                f,
                "supersedes revision {} is not earlier than this revision",
                n.revision
            ),
        }
    }
}

impl std::error::Error for NodeContractError {}

impl NodeSpec {
    /// Check every rule that concerns this node alone, returning all
    /// violations in document order. Cross-node rules (type compatibility,
    /// cycles, dependencies on nodes outside the graph) belong to graph
    /// admission.
    pub fn check_contract(&self) -> Vec<NodeContractError> {
        let mut errors = Vec::new();
        if self.prompt.trim().is_empty() {
            errors.push(NodeContractError::EmptyPrompt);
        }
        if self.acceptance.required_checks.is_empty() {
            errors.push(NodeContractError::NoRequiredChecks);
        }
        let mut seen_checks = HashSet::new();
        for check in &self.acceptance.required_checks {
            if !seen_checks.insert(check) {
                errors.push(NodeContractError::DuplicateRequiredCheck(check.clone()));
            }
        }
        let mut seen_deps = HashSet::new();
        for dep in &self.deps {
            if !seen_deps.insert((&dep.node, &dep.output_port)) {
                errors.push(NodeContractError::DuplicateDependency {
                    node: dep.node.clone(),
                    output_port: dep.output_port.clone(),
                });
            }
        }
        let mut seen_ports = HashSet::new();
        let mut human_ports = 0;
        for port in &self.evidence_ports {
            if !seen_ports.insert(&port.name) {
                errors.push(NodeContractError::DuplicatePortName(port.name.clone()));
            }
            if port.allowed_source_scope.is_empty() {
                errors.push(NodeContractError::EmptySourceScope(port.name.clone()));
            }
            if port.kind == PortKind::Human {
                human_ports += 1;
                if self.kind != NodeKind::HumanGate {
                    errors.push(NodeContractError::HumanPortOnNonGate {
                        kind: self.kind,
                        port: port.name.clone(),
                    });
                }
            }
        }
        let mut seen_resources = HashSet::new();
        for claim in &self.resource_scope {
            if !seen_resources.insert(&claim.resource) {
                errors.push(NodeContractError::DuplicateResource(claim.resource.clone()));
            }
        }
        match self.kind {
            NodeKind::Check | NodeKind::Sink if self.deps.is_empty() => {
                errors.push(NodeContractError::KindRequiresDependencies(self.kind));
            }
            NodeKind::HumanGate if human_ports != 1 => {
                errors.push(NodeContractError::HumanGateNeedsOneHumanPort { found: human_ports });
            }
            _ => {}
        }
        if self.retry_policy.max_attempts == 0 {
            errors.push(NodeContractError::ZeroMaxAttempts);
        }
        if let Some(prev) = &self.supersedes {
            if prev.node_id != self.id {
                errors.push(NodeContractError::SupersedesDifferentNode(prev.clone()));
            } else if prev.revision >= self.revision {
                errors.push(NodeContractError::SupersedesNotEarlier(prev.clone()));
            }
        }
        errors
    }
}
