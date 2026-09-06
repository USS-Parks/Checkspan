//! Run-scoped dependency resolution and explicit imports.
//!
//! A node's dependencies resolve only against receipts admitted in this run,
//! or against receipts from other runs that the run's record lists as
//! admitted imports. Nothing else is looked up, so an older run's accepted
//! node never satisfies a new run by accident. Every receipt used must be an
//! accept verdict about the exact node revision, the sealed attempt must
//! declare the result type the consumer expects, the result digest must
//! match, and the receipt must be neither expired nor revoked at the moment
//! of use. Resolution at dispatch yields an immutable snapshot of receipt
//! references; the same checks run again against that snapshot before the
//! dependent's acceptance is recorded.

#![allow(
    clippy::result_large_err,
    reason = "blockers are diagnostic reports built off the hot path; boxing them would only obscure the matches"
)]

use std::collections::BTreeSet;
use std::fmt;

use crate::contracts::{
    Dependency, Digest, GraphRun, GraphSpec, NodeId, NodeRef, NodeSpec, NodeStatus, ReceiptRef,
    Timestamp, TypeRef, Verdict,
};
use crate::state::RunState;
use crate::store::{Ledger, StoreError};

/// Where a resolved dependency's receipt comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// A receipt admitted in this run.
    SameRun,
    /// A receipt from another run, listed in this run's admitted imports.
    Imported,
}

/// One dependency bound to the exact receipt that satisfies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDependency {
    /// The dependency as declared.
    pub dependency: Dependency,
    /// The receipt that satisfies it.
    pub receipt: ReceiptRef,
    /// Where the receipt comes from.
    pub provenance: Provenance,
    /// Digest of the result the receipt accepted.
    pub result_digest: Digest,
}

/// The immutable dependency snapshot pinned at dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencySnapshot {
    /// The consuming node.
    pub node: NodeId,
    /// Every dependency, in declaration order.
    pub resolved: Vec<ResolvedDependency>,
}

impl DependencySnapshot {
    /// The receipt references to pin in the attempt record.
    pub fn receipt_refs(&self) -> Vec<ReceiptRef> {
        self.resolved.iter().map(|r| r.receipt.clone()).collect()
    }
}

/// Why a dependency cannot be used now. Historical receipts are untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocker {
    /// The dependency node is not accepted in this run and has no import.
    NotAccepted {
        /// The dependency.
        node: NodeId,
        /// Its current status in this run.
        status: NodeStatus,
    },
    /// The accepted node's view names no receipt.
    NoReceipt(NodeId),
    /// The receipt is not stored where the reference says.
    ReceiptMissing(ReceiptRef),
    /// The receipt's verdict is not `accept`.
    NotAnAcceptance {
        /// The receipt.
        receipt: ReceiptRef,
        /// Its verdict.
        verdict: Verdict,
    },
    /// The receipt is about a different node revision.
    NodeMismatch {
        /// The node revision the dependency requires.
        expected: NodeRef,
        /// The node revision the receipt is about.
        found: NodeRef,
    },
    /// The sealed attempt the receipt refers to is not stored.
    AttemptMissing(ReceiptRef),
    /// The sealed attempt has no result.
    NoResult(ReceiptRef),
    /// The sealed attempt declares a different result type.
    ResultTypeMismatch {
        /// The type the consumer expects.
        expected: TypeRef,
        /// The type the producer declared.
        found: TypeRef,
    },
    /// The result digest differs from what was expected or checked.
    SubjectMismatch {
        /// The digest expected.
        expected: Digest,
        /// The digest found.
        found: Digest,
    },
    /// The receipt or import is no longer valid at `now`.
    Expired {
        /// The receipt.
        receipt: ReceiptRef,
        /// When it stopped being valid.
        valid_until: Timestamp,
    },
    /// The receipt was revoked in its run.
    Revoked(ReceiptRef),
    /// The dependency names a node that is not in the graph.
    UnknownNode(NodeId),
    /// The store could not be read.
    Store(String),
}

impl fmt::Display for Blocker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Blocker::NotAccepted { node, status } => {
                write!(f, "dependency {node} is {status} in this run, not accepted")
            }
            Blocker::NoReceipt(node) => write!(f, "dependency {node} names no receipt"),
            Blocker::ReceiptMissing(r) => write!(f, "receipt {r} is not stored"),
            Blocker::NotAnAcceptance { receipt, verdict } => {
                write!(
                    f,
                    "receipt {receipt} is a {verdict} verdict, not an acceptance"
                )
            }
            Blocker::NodeMismatch { expected, found } => {
                write!(f, "receipt is about {found}, not {expected}")
            }
            Blocker::AttemptMissing(r) => write!(f, "the attempt behind receipt {r} is not stored"),
            Blocker::NoResult(r) => write!(f, "the attempt behind receipt {r} has no result"),
            Blocker::ResultTypeMismatch { expected, found } => {
                write!(
                    f,
                    "producer declares {found} but the consumer expects {expected}"
                )
            }
            Blocker::SubjectMismatch { expected, found } => {
                write!(f, "result digest {found} is not the expected {expected}")
            }
            Blocker::Expired {
                receipt,
                valid_until,
            } => write!(f, "receipt {receipt} expired at {valid_until}"),
            Blocker::Revoked(r) => write!(f, "receipt {r} was revoked"),
            Blocker::UnknownNode(n) => write!(f, "node {n} is not in the graph"),
            Blocker::Store(e) => write!(f, "store error: {e}"),
        }
    }
}

impl std::error::Error for Blocker {}

/// Resolves dependencies for one run against the ledger at one moment.
pub struct DependencyService<'a> {
    spec: &'a GraphSpec,
    run: &'a GraphRun,
    state: &'a RunState,
    store: &'a dyn Ledger,
    now: &'a Timestamp,
}

impl<'a> DependencyService<'a> {
    /// A service over an admitted graph, its run record, the run's replayed
    /// state, the store, and the moment freshness is judged at.
    pub fn new(
        spec: &'a GraphSpec,
        run: &'a GraphRun,
        state: &'a RunState,
        store: &'a dyn Ledger,
        now: &'a Timestamp,
    ) -> Self {
        DependencyService {
            spec,
            run,
            state,
            store,
            now,
        }
    }

    fn node(&self, id: &NodeId) -> Result<&'a NodeSpec, Blocker> {
        self.spec
            .nodes
            .iter()
            .find(|n| &n.id == id)
            .ok_or_else(|| Blocker::UnknownNode(id.clone()))
    }

    /// Check one receipt reference against everything a dependency needs:
    /// stored, an acceptance, about `expected_node`, over a sealed attempt
    /// that declares `expected_type`, with a result digest that matches the
    /// receipt (and `expected_digest` when given), unexpired, unrevoked.
    /// Returns the accepted result digest.
    pub fn admissible(
        &self,
        receipt_ref: &ReceiptRef,
        expected_node: &NodeRef,
        expected_type: &TypeRef,
        expected_digest: Option<&Digest>,
    ) -> Result<Digest, Blocker> {
        let store_err = |e: StoreError| Blocker::Store(e.to_string());
        let receipt = self
            .store
            .receipt(&receipt_ref.run_id, receipt_ref.id.as_str())
            .map_err(store_err)?
            .ok_or_else(|| Blocker::ReceiptMissing(receipt_ref.clone()))?;
        if receipt.verdict != Verdict::Accept {
            return Err(Blocker::NotAnAcceptance {
                receipt: receipt_ref.clone(),
                verdict: receipt.verdict,
            });
        }
        if &receipt.attempt.node != expected_node {
            return Err(Blocker::NodeMismatch {
                expected: expected_node.clone(),
                found: receipt.attempt.node.clone(),
            });
        }
        let attempt = self
            .store
            .attempt(
                &receipt.attempt.run_id,
                &receipt.attempt.node.node_id,
                receipt.attempt.number.get(),
            )
            .map_err(store_err)?
            .ok_or_else(|| Blocker::AttemptMissing(receipt_ref.clone()))?;
        let result = attempt
            .result
            .as_ref()
            .ok_or_else(|| Blocker::NoResult(receipt_ref.clone()))?;
        if &result.result_type != expected_type {
            return Err(Blocker::ResultTypeMismatch {
                expected: expected_type.clone(),
                found: result.result_type.clone(),
            });
        }
        if result.digest != receipt.result_digest {
            return Err(Blocker::SubjectMismatch {
                expected: result.digest.clone(),
                found: receipt.result_digest.clone(),
            });
        }
        if let Some(expected) = expected_digest
            && expected != &receipt.result_digest
        {
            return Err(Blocker::SubjectMismatch {
                expected: expected.clone(),
                found: receipt.result_digest.clone(),
            });
        }
        if let Some(valid_until) = &receipt.validity.valid_until
            && !self.now.is_before(valid_until)
        {
            return Err(Blocker::Expired {
                receipt: receipt_ref.clone(),
                valid_until: valid_until.clone(),
            });
        }
        let revoked = self
            .store
            .revoked_receipts(&receipt_ref.run_id)
            .map_err(store_err)?;
        if revoked.contains(receipt_ref.id.as_str()) {
            return Err(Blocker::Revoked(receipt_ref.clone()));
        }
        Ok(receipt.result_digest)
    }

    /// Resolve one dependency: this run's acceptance first, then an admitted
    /// import, else blocked.
    fn resolve_one(&self, dep: &Dependency) -> Result<ResolvedDependency, Blocker> {
        let producer_id = &dep.node.node_id;
        self.node(producer_id)?;
        if let Some(state) = self.state.node(producer_id)
            && state.status == NodeStatus::Accepted
        {
            let receipt = state
                .receipt
                .clone()
                .ok_or_else(|| Blocker::NoReceipt(producer_id.clone()))?;
            let digest = self.admissible(&receipt, &dep.node, &dep.expected_type, None)?;
            return Ok(ResolvedDependency {
                dependency: dep.clone(),
                receipt,
                provenance: Provenance::SameRun,
                result_digest: digest,
            });
        }
        if let Some(import) = self
            .run
            .admitted_imports
            .iter()
            .find(|i| &i.node_id == producer_id)
        {
            if let Some(valid_until) = &import.valid_until
                && !self.now.is_before(valid_until)
            {
                return Err(Blocker::Expired {
                    receipt: import.receipt.clone(),
                    valid_until: valid_until.clone(),
                });
            }
            if import.result_type != dep.expected_type {
                return Err(Blocker::ResultTypeMismatch {
                    expected: dep.expected_type.clone(),
                    found: import.result_type.clone(),
                });
            }
            let digest = self.admissible(
                &import.receipt,
                &dep.node,
                &dep.expected_type,
                Some(&import.result_digest),
            )?;
            return Ok(ResolvedDependency {
                dependency: dep.clone(),
                receipt: import.receipt.clone(),
                provenance: Provenance::Imported,
                result_digest: digest,
            });
        }
        let status = self
            .state
            .node(producer_id)
            .map(|s| s.status)
            .unwrap_or(NodeStatus::Open);
        Err(Blocker::NotAccepted {
            node: producer_id.clone(),
            status,
        })
    }

    /// Resolve every dependency of `node_id` into a snapshot, or return every
    /// blocker in declaration order.
    pub fn resolve(&self, node_id: &NodeId) -> Result<DependencySnapshot, Vec<Blocker>> {
        let node = self.node(node_id).map_err(|b| vec![b])?;
        let mut resolved = Vec::new();
        let mut blockers = Vec::new();
        for dep in &node.deps {
            match self.resolve_one(dep) {
                Ok(r) => resolved.push(r),
                Err(b) => blockers.push(b),
            }
        }
        if blockers.is_empty() {
            Ok(DependencySnapshot {
                node: node_id.clone(),
                resolved,
            })
        } else {
            Err(blockers)
        }
    }

    /// Recheck a pinned snapshot at `now`: every receipt must still be
    /// admissible for the exact node and type it was pinned for.
    pub fn recheck(&self, snapshot: &DependencySnapshot) -> Result<(), Vec<Blocker>> {
        let blockers: Vec<Blocker> = snapshot
            .resolved
            .iter()
            .filter_map(|r| {
                self.admissible(
                    &r.receipt,
                    &r.dependency.node,
                    &r.dependency.expected_type,
                    Some(&r.result_digest),
                )
                .err()
            })
            .collect();
        if blockers.is_empty() {
            Ok(())
        } else {
            Err(blockers)
        }
    }

    /// Recheck the receipt references pinned in an attempt record against
    /// the consuming node's declared dependencies. Every declared dependency
    /// must be pinned, every pinned receipt must belong to a declared
    /// dependency, and each must still be admissible.
    pub fn recheck_refs(
        &self,
        node_id: &NodeId,
        pinned: &[ReceiptRef],
    ) -> Result<(), Vec<Blocker>> {
        let node = self.node(node_id).map_err(|b| vec![b])?;
        let mut blockers = Vec::new();
        let mut used: BTreeSet<usize> = BTreeSet::new();
        for dep in &node.deps {
            let found = pinned.iter().enumerate().find(|(_, r)| r.node == dep.node);
            match found {
                Some((i, receipt)) => {
                    used.insert(i);
                    if let Err(b) = self.admissible(receipt, &dep.node, &dep.expected_type, None) {
                        blockers.push(b);
                    }
                }
                None => blockers.push(Blocker::NotAccepted {
                    node: dep.node.node_id.clone(),
                    status: self
                        .state
                        .node(&dep.node.node_id)
                        .map(|s| s.status)
                        .unwrap_or(NodeStatus::Open),
                }),
            }
        }
        for (i, extra) in pinned.iter().enumerate() {
            if !used.contains(&i) {
                blockers.push(Blocker::NodeMismatch {
                    expected: NodeRef {
                        graph_id: self.spec.graph_id.clone(),
                        node_id: node_id.clone(),
                        revision: node.revision,
                    },
                    found: extra.node.clone(),
                });
            }
        }
        if blockers.is_empty() {
            Ok(())
        } else {
            Err(blockers)
        }
    }

    /// Human-readable reason an open node cannot be dispatched, if any.
    pub fn blocked_reason(&self, node_id: &NodeId) -> Option<String> {
        match self.resolve(node_id) {
            Ok(_) => None,
            Err(blockers) => Some(
                blockers
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; "),
            ),
        }
    }
}
