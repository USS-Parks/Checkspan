//! Graph admission: turn individually valid records into a validated,
//! inspectable DAG, or explain exactly why the graph cannot be dispatched.
//!
//! Admission composes the identity rules of the graph envelope, every node's
//! own contract rules, and the cross-node rules that only the whole graph can
//! answer: dependencies resolve to in-graph nodes at their exact revision,
//! output ports and result types match, there are no cycles, proof inputs do
//! not hide dependencies, and a human gate never waits on the acceptance of
//! the node it resolves. Admission reads the graph and nothing else: it does
//! not dispatch, create a run, or touch a store.
//!
//! Scope conventions: an evidence port of kind `proofs` names in-graph
//! sources as `node:<node_id>`, and each such source must also be a declared
//! dependency. A `human` port on a `human_gate` names the nodes whose
//! attempts the gate resolves as `node:<node_id>` alongside its
//! `signer:<identity>` entries.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
use std::fmt;

use crate::contracts::{
    GraphSpec, Ident, IdentityError, NodeContractError, NodeId, NodeKind, NodeRef, NodeSpec,
    PortKind, Revision, TypeRef,
};
use crate::validation::{Diagnostic, Stage};

/// The single output port every node exposes.
pub const RESULT_PORT: &str = "result";

/// Prefix of a scope entry that names an in-graph node.
pub const NODE_SCOPE_PREFIX: &str = "node:";

/// Why a structurally valid graph cannot be admitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionError {
    /// A graph envelope identity rule failed.
    Identity(IdentityError),
    /// A node's own contract rule failed.
    Contract {
        /// The node.
        node: NodeId,
        /// The rule.
        error: NodeContractError,
    },
    /// A dependency names another graph; cross-graph imports are not
    /// supported.
    ExternalDependency {
        /// The consuming node.
        node: NodeId,
        /// The dependency as written.
        dependency: NodeRef,
    },
    /// A dependency names a node that is not in this graph.
    UnresolvedDependency {
        /// The consuming node.
        node: NodeId,
        /// The dependency as written.
        dependency: NodeRef,
    },
    /// A dependency names a node at a revision other than the one here.
    DependencyRevisionMismatch {
        /// The consuming node.
        node: NodeId,
        /// The dependency as written.
        dependency: NodeRef,
        /// The revision the node actually has in this graph.
        actual: Revision,
    },
    /// A node depends on itself.
    SelfDependency(NodeId),
    /// A dependency consumes a port the producer does not expose.
    UnknownOutputPort {
        /// The consuming node.
        node: NodeId,
        /// The producing node.
        producer: NodeId,
        /// The port requested.
        port: Ident,
    },
    /// A dependency expects a result type the producer does not produce.
    IncompatibleType {
        /// The consuming node.
        node: NodeId,
        /// The producing node.
        producer: NodeId,
        /// What the consumer expects.
        expected: TypeRef,
        /// What the producer declares.
        actual: TypeRef,
    },
    /// A `proofs` port draws on an in-graph node without a dependency on it.
    HiddenProofDependency {
        /// The node with the port.
        node: NodeId,
        /// The port.
        port: Ident,
        /// The in-graph source.
        source: NodeId,
    },
    /// A `proofs` port names an in-graph node that does not exist.
    UnresolvedProofSource {
        /// The node with the port.
        node: NodeId,
        /// The port.
        port: Ident,
        /// The name given.
        source: String,
    },
    /// A `proofs` port names a source outside the graph; imported receipts
    /// are not supported.
    UnsupportedExternalImport {
        /// The node with the port.
        node: NodeId,
        /// The port.
        port: Ident,
        /// The scope entry.
        source: String,
    },
    /// A gate resolves a node that is not in this graph.
    GateResolvesUnknownNode {
        /// The gate.
        gate: NodeId,
        /// The name given.
        target: String,
    },
    /// A gate and the node it resolves are joined by an acceptance chain.
    GateWaitCycle {
        /// The gate.
        gate: NodeId,
        /// The node it resolves.
        resolves: NodeId,
        /// The acceptance chain, each node depending on the next.
        path: Vec<NodeId>,
    },
    /// The dependency graph has a cycle; each node depends on the next and
    /// the last depends on the first.
    Cycle(Vec<NodeId>),
}

impl AdmissionError {
    /// The node the error is reported against, when it concerns one node.
    pub fn node(&self) -> Option<&NodeId> {
        match self {
            AdmissionError::Identity(_) => None,
            AdmissionError::Contract { node, .. }
            | AdmissionError::ExternalDependency { node, .. }
            | AdmissionError::UnresolvedDependency { node, .. }
            | AdmissionError::DependencyRevisionMismatch { node, .. }
            | AdmissionError::UnknownOutputPort { node, .. }
            | AdmissionError::IncompatibleType { node, .. }
            | AdmissionError::HiddenProofDependency { node, .. }
            | AdmissionError::UnresolvedProofSource { node, .. }
            | AdmissionError::UnsupportedExternalImport { node, .. } => Some(node),
            AdmissionError::SelfDependency(node) => Some(node),
            AdmissionError::GateResolvesUnknownNode { gate, .. }
            | AdmissionError::GateWaitCycle { gate, .. } => Some(gate),
            AdmissionError::Cycle(path) => path.first(),
        }
    }
}

fn join(ids: &[NodeId]) -> String {
    ids.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" -> ")
}

impl fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdmissionError::Identity(e) => write!(f, "{e}"),
            AdmissionError::Contract { node, error } => write!(f, "node {node}: {error}"),
            AdmissionError::ExternalDependency { node, dependency } => write!(
                f,
                "node {node} depends on {dependency} in another graph; cross-graph imports are not supported"
            ),
            AdmissionError::UnresolvedDependency { node, dependency } => write!(
                f,
                "node {node} depends on {dependency}, which is not in this graph"
            ),
            AdmissionError::DependencyRevisionMismatch {
                node,
                dependency,
                actual,
            } => write!(
                f,
                "node {node} depends on {dependency} but that node is at revision {actual} here"
            ),
            AdmissionError::SelfDependency(node) => write!(f, "node {node} depends on itself"),
            AdmissionError::UnknownOutputPort {
                node,
                producer,
                port,
            } => write!(
                f,
                "node {node} consumes port {port:?} of {producer}, which only exposes {RESULT_PORT:?}"
            ),
            AdmissionError::IncompatibleType {
                node,
                producer,
                expected,
                actual,
            } => {
                if expected.schema_id == actual.schema_id && expected.version == actual.version {
                    write!(
                        f,
                        "node {node} expects {expected} from {producer} with digest {}, but {producer} declares digest {}",
                        expected.digest, actual.digest
                    )
                } else {
                    write!(
                        f,
                        "node {node} expects {expected} from {producer}, which produces {actual}"
                    )
                }
            }
            AdmissionError::HiddenProofDependency { node, port, source } => write!(
                f,
                "node {node} proof port {port:?} draws on node {source} without declaring a dependency on it"
            ),
            AdmissionError::UnresolvedProofSource { node, port, source } => write!(
                f,
                "node {node} proof port {port:?} names node {source:?}, which is not in this graph"
            ),
            AdmissionError::UnsupportedExternalImport { node, port, source } => write!(
                f,
                "node {node} proof port {port:?} names external source {source:?}; only in-graph `node:<id>` sources are supported"
            ),
            AdmissionError::GateResolvesUnknownNode { gate, target } => write!(
                f,
                "gate {gate} resolves {target:?}, which is not in this graph"
            ),
            AdmissionError::GateWaitCycle {
                gate,
                resolves,
                path,
            } => write!(
                f,
                "gate {gate} resolves {resolves} but the acceptance chain {} joins them; a gate cannot wait on the node it resolves",
                join(path)
            ),
            AdmissionError::Cycle(path) => {
                write!(f, "dependency cycle: {}", join(path))?;
                if let Some(first) = path.first() {
                    write!(f, " -> {first}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for AdmissionError {}

/// Which nodes a human gate resolves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateBinding {
    /// The gate node.
    pub gate: NodeId,
    /// The nodes whose attempts its decisions route.
    pub resolves: Vec<NodeId>,
}

/// A graph that passed admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedGraph {
    /// The graph as admitted.
    pub spec: GraphSpec,
    /// Every node in a deterministic dependency order: producers before
    /// consumers, ties broken by document order.
    pub order: Vec<NodeId>,
    /// The nodes some target depends on, including the targets, in `order`.
    pub required: Vec<NodeId>,
    /// The nodes no target depends on, in `order`. They may still run, but
    /// completion does not wait for them.
    pub optional: Vec<NodeId>,
    /// Every human gate and the nodes it resolves.
    pub gates: Vec<GateBinding>,
}

impl AdmittedGraph {
    /// The node with this id.
    pub fn node(&self, id: &NodeId) -> Option<&NodeSpec> {
        self.spec.nodes.iter().find(|n| &n.id == id)
    }

    /// The nodes `id` directly depends on, in declaration order.
    pub fn producers(&self, id: &NodeId) -> Vec<NodeId> {
        self.node(id)
            .map(|n| n.deps.iter().map(|d| d.node.node_id.clone()).collect())
            .unwrap_or_default()
    }

    /// The nodes that directly depend on `id`, in document order.
    pub fn dependents(&self, id: &NodeId) -> Vec<NodeId> {
        self.spec
            .nodes
            .iter()
            .filter(|n| n.deps.iter().any(|d| &d.node.node_id == id))
            .map(|n| n.id.clone())
            .collect()
    }
}

/// Admit `spec`, returning every reason it cannot be dispatched in a
/// deterministic order, or the admitted graph.
pub fn admit(spec: &GraphSpec) -> Result<AdmittedGraph, Vec<AdmissionError>> {
    let mut errors: Vec<AdmissionError> = spec
        .check_identity()
        .into_iter()
        .map(AdmissionError::Identity)
        .collect();
    for node in &spec.nodes {
        for error in node.check_contract() {
            errors.push(AdmissionError::Contract {
                node: node.id.clone(),
                error,
            });
        }
    }

    let n = spec.nodes.len();
    let mut index: HashMap<&NodeId, usize> = HashMap::new();
    for (i, node) in spec.nodes.iter().enumerate() {
        index.entry(&node.id).or_insert(i);
    }

    // deps_of[consumer] = producers, in declaration order, resolved in-graph.
    let mut deps_of: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (ci, node) in spec.nodes.iter().enumerate() {
        for dep in &node.deps {
            if dep.node.graph_id != spec.graph_id {
                errors.push(AdmissionError::ExternalDependency {
                    node: node.id.clone(),
                    dependency: dep.node.clone(),
                });
                continue;
            }
            let Some(&pi) = index.get(&dep.node.node_id) else {
                errors.push(AdmissionError::UnresolvedDependency {
                    node: node.id.clone(),
                    dependency: dep.node.clone(),
                });
                continue;
            };
            let producer = &spec.nodes[pi];
            if producer.id == node.id {
                errors.push(AdmissionError::SelfDependency(node.id.clone()));
                continue;
            }
            if producer.revision != dep.node.revision {
                errors.push(AdmissionError::DependencyRevisionMismatch {
                    node: node.id.clone(),
                    dependency: dep.node.clone(),
                    actual: producer.revision,
                });
            }
            if dep.output_port.as_str() != RESULT_PORT {
                errors.push(AdmissionError::UnknownOutputPort {
                    node: node.id.clone(),
                    producer: producer.id.clone(),
                    port: dep.output_port.clone(),
                });
            }
            if dep.expected_type != producer.result_type {
                errors.push(AdmissionError::IncompatibleType {
                    node: node.id.clone(),
                    producer: producer.id.clone(),
                    expected: dep.expected_type.clone(),
                    actual: producer.result_type.clone(),
                });
            }
            if !deps_of[ci].contains(&pi) {
                deps_of[ci].push(pi);
            }
        }
        for port in node
            .evidence_ports
            .iter()
            .filter(|p| p.kind == PortKind::Proofs)
        {
            for source in &port.allowed_source_scope {
                let Some(name) = source.strip_prefix(NODE_SCOPE_PREFIX) else {
                    errors.push(AdmissionError::UnsupportedExternalImport {
                        node: node.id.clone(),
                        port: port.name.clone(),
                        source: source.clone(),
                    });
                    continue;
                };
                let Some(&si) = NodeId::new(name)
                    .ok()
                    .and_then(|id| index.get(&id).copied())
                    .as_ref()
                else {
                    errors.push(AdmissionError::UnresolvedProofSource {
                        node: node.id.clone(),
                        port: port.name.clone(),
                        source: name.to_string(),
                    });
                    continue;
                };
                if !deps_of[ci].contains(&si) {
                    errors.push(AdmissionError::HiddenProofDependency {
                        node: node.id.clone(),
                        port: port.name.clone(),
                        source: spec.nodes[si].id.clone(),
                    });
                }
            }
        }
    }

    let mut gates = Vec::new();
    let mut gate_pairs: Vec<(usize, usize)> = Vec::new();
    for (gi, node) in spec.nodes.iter().enumerate() {
        if node.kind != NodeKind::HumanGate {
            continue;
        }
        let mut resolves = Vec::new();
        for port in node
            .evidence_ports
            .iter()
            .filter(|p| p.kind == PortKind::Human)
        {
            for source in &port.allowed_source_scope {
                let Some(name) = source.strip_prefix(NODE_SCOPE_PREFIX) else {
                    continue;
                };
                match NodeId::new(name)
                    .ok()
                    .and_then(|id| index.get(&id).map(|&ti| (id, ti)))
                {
                    Some((id, ti)) => {
                        resolves.push(id);
                        gate_pairs.push((gi, ti));
                    }
                    None => errors.push(AdmissionError::GateResolvesUnknownNode {
                        gate: node.id.clone(),
                        target: name.to_string(),
                    }),
                }
            }
        }
        gates.push(GateBinding {
            gate: node.id.clone(),
            resolves,
        });
    }

    // Kahn's algorithm with document-order tie-breaking.
    let mut indegree = vec![0usize; n];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (ci, producers) in deps_of.iter().enumerate() {
        for &pi in producers {
            indegree[ci] += 1;
            dependents[pi].push(ci);
        }
    }
    let mut ready: BinaryHeap<Reverse<usize>> =
        (0..n).filter(|&i| indegree[i] == 0).map(Reverse).collect();
    let mut order_idx = Vec::with_capacity(n);
    while let Some(Reverse(i)) = ready.pop() {
        order_idx.push(i);
        for &d in &dependents[i] {
            indegree[d] -= 1;
            if indegree[d] == 0 {
                ready.push(Reverse(d));
            }
        }
    }
    let has_cycle = order_idx.len() < n;
    if has_cycle {
        let remaining: HashSet<usize> = (0..n).filter(|&i| indegree[i] > 0).collect();
        let start = (0..n)
            .find(|i| remaining.contains(i))
            .expect("a leftover node exists");
        let mut path = vec![start];
        let mut position: HashMap<usize, usize> = HashMap::from([(start, 0)]);
        let mut current = start;
        loop {
            let next = deps_of[current]
                .iter()
                .copied()
                .find(|p| remaining.contains(p))
                .expect("a leftover node has a leftover producer");
            if let Some(&pos) = position.get(&next) {
                errors.push(AdmissionError::Cycle(
                    path[pos..]
                        .iter()
                        .map(|&i| spec.nodes[i].id.clone())
                        .collect(),
                ));
                break;
            }
            position.insert(next, path.len());
            path.push(next);
            current = next;
        }
    } else {
        for (gi, ti) in gate_pairs {
            let gate = spec.nodes[gi].id.clone();
            let resolves = spec.nodes[ti].id.clone();
            let chain = if gi == ti {
                Some(vec![gi])
            } else {
                dependency_path(&deps_of, gi, ti).or_else(|| dependency_path(&deps_of, ti, gi))
            };
            if let Some(chain) = chain {
                errors.push(AdmissionError::GateWaitCycle {
                    gate,
                    resolves,
                    path: chain.iter().map(|&i| spec.nodes[i].id.clone()).collect(),
                });
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let mut required: HashSet<usize> = HashSet::new();
    let mut queue: VecDeque<usize> = spec
        .targets
        .iter()
        .filter_map(|t| index.get(&t.node_id).copied())
        .collect();
    while let Some(i) = queue.pop_front() {
        if required.insert(i) {
            queue.extend(deps_of[i].iter().copied());
        }
    }
    let id_at = |i: &usize| spec.nodes[*i].id.clone();
    Ok(AdmittedGraph {
        spec: spec.clone(),
        order: order_idx.iter().map(id_at).collect(),
        required: order_idx
            .iter()
            .filter(|i| required.contains(i))
            .map(id_at)
            .collect(),
        optional: order_idx
            .iter()
            .filter(|i| !required.contains(i))
            .map(id_at)
            .collect(),
        gates,
    })
}

/// The dependency chain from `from` to `to` (each node depending on the
/// next), if `from` transitively depends on `to`.
fn dependency_path(deps_of: &[Vec<usize>], from: usize, to: usize) -> Option<Vec<usize>> {
    let mut previous: HashMap<usize, usize> = HashMap::new();
    let mut queue = VecDeque::from([from]);
    let mut seen = HashSet::from([from]);
    while let Some(current) = queue.pop_front() {
        for &next in &deps_of[current] {
            if seen.insert(next) {
                previous.insert(next, current);
                if next == to {
                    let mut path = vec![to];
                    let mut at = to;
                    while let Some(&p) = previous.get(&at) {
                        path.push(p);
                        at = p;
                    }
                    path.reverse();
                    return Some(path);
                }
                queue.push_back(next);
            }
        }
    }
    None
}

/// Admission errors as validation diagnostics, each at the node it concerns.
pub fn admission_diagnostics(spec: &GraphSpec) -> Vec<Diagnostic> {
    match admit(spec) {
        Ok(_) => Vec::new(),
        Err(errors) => errors
            .iter()
            .map(|e| Diagnostic {
                stage: Stage::Admission,
                path: e
                    .node()
                    .and_then(|id| spec.nodes.iter().position(|n| &n.id == id))
                    .map(|i| format!("/nodes/{i}"))
                    .unwrap_or_default(),
                message: e.to_string(),
            })
            .collect(),
    }
}
