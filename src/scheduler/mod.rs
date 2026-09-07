//! Exclusive scheduling and completion ownership.
//!
//! One controller claims ready work inside one `BEGIN IMMEDIATE` transaction:
//! it replays the run's event log, resolves dependencies, picks the first
//! ready node in the admitted dependency order that no active claim
//! conflicts with, and writes the claim row and the `attempt_dispatched`
//! event together. The claim carries a fencing token. A completion is
//! accepted only while the claim is active under that exact token; after a
//! cancellation or a reclaim by another controller the token no longer
//! matches and the completion is refused with nothing written. At most
//! `max_active` claims (default one) are held at a time, and nodes that need
//! exclusive access to a resource an active claim holds are skipped, so
//! independent nodes sharing a checkout serialize. No command is executed
//! here; running the claimed work belongs to the controller and the
//! verifier host.

use std::fmt;

use crate::budget::{ExhaustReason, Narrowing, budget_status, pending_narrowing};
use crate::contracts::{
    Attempt, AttemptNumber, Execution, ExecutionOutcome, GraphRun, GraphSpec, Ident, NodeId,
    NodeSpec, NodeStatus, NodeView, ReceiptRef, RecordKind, ResourceAccess, RunId, SchemaVersion,
    Timestamp,
};
use crate::deps::{Blocker, DependencyService};
use crate::graph::{AdmittedGraph, admit};
use crate::state::{NodeEvent, RunState, TransitionError, kinds};
use crate::store::{ClaimRow, Ledger, Store, StoreError};

/// Error code sealed into an attempt whose claim was taken over.
pub const RECLAIMED_ERROR_CODE: &str = "claim_reclaimed";

/// Identity of a controller process: non-empty text such as
/// `host/pid/instance`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ControllerId(String);

impl ControllerId {
    /// Build a controller identity; empty text is refused.
    pub fn new(text: impl Into<String>) -> Result<Self, SchedulerError> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(SchedulerError::Invalid(
                "controller identity is empty".into(),
            ));
        }
        Ok(ControllerId(text))
    }

    /// The identity text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ControllerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A claim this controller holds on one attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// The run.
    pub run_id: RunId,
    /// The node.
    pub node_id: NodeId,
    /// The attempt number dispatched.
    pub number: AttemptNumber,
    /// The fencing token issued with the claim.
    pub fence: u64,
    /// The owning controller.
    pub owner: ControllerId,
    /// Dependency receipts pinned at dispatch.
    pub dependency_receipts: Vec<ReceiptRef>,
    /// The narrowing recorded by the retry that reopened the node, if any.
    pub narrowing: Option<Narrowing>,
}

/// What a claim attempt produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimOutcome {
    /// An attempt was claimed and dispatched.
    Claimed(Claim),
    /// The active-claim cap is reached.
    Busy {
        /// Active claims in the store.
        active: usize,
        /// The cap.
        max_active: usize,
    },
    /// The graph-wide budget or deadline forbids any new attempt.
    BudgetExhausted(ExhaustReason),
    /// No open node can be dispatched right now.
    NothingReady {
        /// Open nodes with unresolved dependencies and why.
        blocked: Vec<(NodeId, String)>,
        /// Ready nodes skipped because an active claim holds a resource
        /// they need, with the resource.
        conflicts: Vec<(NodeId, Ident)>,
    },
}

/// Why a scheduling operation failed. Nothing is written on error.
#[derive(Debug)]
pub enum SchedulerError {
    /// The graph or run record is not usable.
    Invalid(String),
    /// The store failed.
    Store(StoreError),
    /// The reducer refused the event the operation would record.
    Transition(TransitionError),
    /// No active claim exists for the attempt.
    NotClaimed {
        /// The node.
        node: NodeId,
        /// The attempt.
        number: AttemptNumber,
    },
    /// The claim is no longer held under the caller's token: it was
    /// released, cancelled, or reclaimed.
    Stale {
        /// The node.
        node: NodeId,
        /// The attempt.
        number: AttemptNumber,
        /// The token the caller holds.
        held: u64,
        /// The claim row's token.
        current: u64,
        /// How the row was released, if it was.
        release: Option<String>,
    },
    /// The attempt record does not describe the claimed attempt.
    AttemptMismatch(String),
    /// The attempt's pinned dependency receipts differ from the claim's.
    SnapshotMismatch,
    /// A dependency recheck failed.
    Blocked(Vec<Blocker>),
}

impl fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchedulerError::Invalid(what) => write!(f, "{what}"),
            SchedulerError::Store(e) => write!(f, "{e}"),
            SchedulerError::Transition(e) => write!(f, "{e}"),
            SchedulerError::NotClaimed { node, number } => {
                write!(f, "no claim exists for {node}#{number}")
            }
            SchedulerError::Stale {
                node,
                number,
                held,
                current,
                release,
            } => write!(
                f,
                "claim on {node}#{number} is stale: caller holds token {held}, row has token {current}{}",
                release
                    .as_ref()
                    .map(|r| format!(" and was released as {r}"))
                    .unwrap_or_default()
            ),
            SchedulerError::AttemptMismatch(what) => write!(f, "attempt mismatch: {what}"),
            SchedulerError::SnapshotMismatch => {
                f.write_str("the attempt's dependency receipts are not the ones pinned at dispatch")
            }
            SchedulerError::Blocked(blockers) => write!(
                f,
                "blocked: {}",
                blockers
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        }
    }
}

impl std::error::Error for SchedulerError {}

impl From<StoreError> for SchedulerError {
    fn from(e: StoreError) -> Self {
        SchedulerError::Store(e)
    }
}

impl From<TransitionError> for SchedulerError {
    fn from(e: TransitionError) -> Self {
        SchedulerError::Transition(e)
    }
}

/// The scheduler for one run, acting as one controller.
pub struct Scheduler<'a> {
    spec: &'a GraphSpec,
    graph: AdmittedGraph,
    run: &'a GraphRun,
    controller: ControllerId,
    max_active: usize,
}

/// The first resource of `node` that an active claim makes unavailable.
fn conflict(node: &NodeSpec, active: &[ClaimRow]) -> Option<Ident> {
    for wanted in &node.resource_scope {
        for claim in active {
            for held in &claim.resources {
                if held.resource != wanted.resource {
                    continue;
                }
                if wanted.access == ResourceAccess::Exclusive
                    || held.access == ResourceAccess::Exclusive
                {
                    return Some(wanted.resource.clone());
                }
            }
        }
    }
    None
}

impl<'a> Scheduler<'a> {
    /// A scheduler over an admitted graph and its run, acting as
    /// `controller`, with the default cap of one active claim.
    pub fn new(
        spec: &'a GraphSpec,
        run: &'a GraphRun,
        controller: ControllerId,
    ) -> Result<Self, SchedulerError> {
        if run.graph_ref != spec.graph_ref() {
            return Err(SchedulerError::Invalid(format!(
                "run {} is of graph {}@{}, not {}@{}",
                run.run_id,
                run.graph_ref.graph_id,
                run.graph_ref.revision,
                spec.graph_id,
                spec.revision
            )));
        }
        let graph = admit(spec).map_err(|errors| {
            SchedulerError::Invalid(format!(
                "graph not admitted: {}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ))
        })?;
        Ok(Scheduler {
            spec,
            graph,
            run,
            controller,
            max_active: 1,
        })
    }

    /// Allow up to `max_active` claims to be held at once (default one).
    pub fn with_max_active(mut self, max_active: usize) -> Self {
        self.max_active = max_active.max(1);
        self
    }

    /// The controller this scheduler acts as.
    pub fn controller(&self) -> &ControllerId {
        &self.controller
    }

    /// The admitted graph.
    pub fn graph(&self) -> &AdmittedGraph {
        &self.graph
    }

    fn state(&self, ledger: &dyn Ledger) -> Result<RunState, SchedulerError> {
        let events = ledger.events(&self.run.run_id)?;
        Ok(RunState::replay(
            self.run.run_id.clone(),
            self.spec,
            &events,
        )?)
    }

    fn node(&self, node_id: &NodeId) -> Result<&'a NodeSpec, SchedulerError> {
        self.spec
            .nodes
            .iter()
            .find(|n| &n.id == node_id)
            .ok_or_else(|| SchedulerError::Invalid(format!("node {node_id} is not in the graph")))
    }

    /// Derived views with `blocked_reason` filled for open nodes.
    pub fn views(
        &self,
        ledger: &dyn Ledger,
        now: &Timestamp,
    ) -> Result<Vec<NodeView>, SchedulerError> {
        let state = self.state(ledger)?;
        let service = DependencyService::new(self.spec, self.run, &state, ledger, now);
        let mut views = state.views();
        for view in &mut views {
            if view.status == NodeStatus::Open {
                view.blocked_reason = service.blocked_reason(&view.node.node_id);
            }
        }
        Ok(views)
    }

    /// Claim the next ready attempt in one transaction, or say why none was
    /// claimed. The ready order is the admitted dependency order.
    pub fn claim_next(
        &self,
        store: &mut Store,
        now: &Timestamp,
    ) -> Result<ClaimOutcome, SchedulerError> {
        let run_id = self.run.run_id.clone();
        let controller = self.controller.clone();
        let max_active = self.max_active;
        let mut failure: Option<SchedulerError> = None;
        let outcome = store.transaction(|tx| {
            let events = tx.events(&run_id)?;
            let state = match RunState::replay(run_id.clone(), self.spec, &events) {
                Ok(state) => state,
                Err(e) => {
                    failure = Some(e.into());
                    return Err(StoreError::Corrupt("replay failed".into()));
                }
            };
            let budget = match budget_status(tx, self.spec, self.run, now) {
                Ok(budget) => budget,
                Err(e) => {
                    failure = Some(SchedulerError::Invalid(e.to_string()));
                    return Err(StoreError::Corrupt("budget unavailable".into()));
                }
            };
            if let Some(reason) = budget.exhaustion() {
                return Ok(ClaimOutcome::BudgetExhausted(reason));
            }
            let active = tx.active_claims()?;
            if active.len() >= max_active {
                return Ok(ClaimOutcome::Busy {
                    active: active.len(),
                    max_active,
                });
            }
            let service = DependencyService::new(self.spec, self.run, &state, tx, now);
            let mut blocked = Vec::new();
            let mut conflicts = Vec::new();
            for node_id in &self.graph.order {
                let Some(node_state) = state.node(node_id) else {
                    continue;
                };
                if node_state.status != NodeStatus::Open {
                    continue;
                }
                let snapshot = match service.resolve(node_id) {
                    Ok(snapshot) => snapshot,
                    Err(blockers) => {
                        blocked.push((
                            node_id.clone(),
                            blockers
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join("; "),
                        ));
                        continue;
                    }
                };
                let node = self
                    .spec
                    .nodes
                    .iter()
                    .find(|n| &n.id == node_id)
                    .expect("admitted");
                if let Some(resource) = conflict(node, &active) {
                    conflicts.push((node_id.clone(), resource));
                    continue;
                }
                let number = AttemptNumber::new(node_state.attempts_started + 1)
                    .expect("attempt numbers start at 1");
                let event = NodeEvent::Dispatched { attempt: number };
                if let Err(e) = node_state.apply(&event) {
                    failure = Some(e.into());
                    return Err(StoreError::Corrupt("illegal dispatch".into()));
                }
                let fence = tx.next_fence(&run_id)?;
                let dependency_receipts = snapshot.receipt_refs();
                tx.insert_claim(&ClaimRow {
                    run_id: run_id.clone(),
                    node_id: node_id.clone(),
                    number,
                    owner: controller.as_str().to_string(),
                    fence,
                    dependency_receipts: dependency_receipts.clone(),
                    resources: node.resource_scope.clone(),
                    active: true,
                    release: None,
                    claimed_at: String::new(),
                    released_at: None,
                })?;
                let mut payload = event.payload();
                payload["owner"] = serde_json::Value::String(controller.as_str().to_string());
                payload["fence"] = serde_json::json!(fence);
                payload["dependency_receipts"] = serde_json::json!(dependency_receipts);
                tx.append_event(&run_id, Some(node_id), kinds::ATTEMPT_DISPATCHED, &payload)?;
                return Ok(ClaimOutcome::Claimed(Claim {
                    run_id: run_id.clone(),
                    node_id: node_id.clone(),
                    number,
                    fence,
                    owner: controller.clone(),
                    dependency_receipts,
                    narrowing: pending_narrowing(&events, node_id),
                }));
            }
            Ok(ClaimOutcome::NothingReady { blocked, conflicts })
        });
        match (outcome, failure) {
            (_, Some(e)) => Err(e),
            (Ok(outcome), None) => Ok(outcome),
            (Err(e), None) => Err(e.into()),
        }
    }

    /// Verify that `claim` is still held under its token and return the row.
    fn held_claim(&self, ledger: &dyn Ledger, claim: &Claim) -> Result<ClaimRow, SchedulerError> {
        let row = ledger
            .claim(&claim.run_id, &claim.node_id, claim.number.get())?
            .ok_or_else(|| SchedulerError::NotClaimed {
                node: claim.node_id.clone(),
                number: claim.number,
            })?;
        if !row.active || row.fence != claim.fence || row.owner != claim.owner.as_str() {
            return Err(SchedulerError::Stale {
                node: claim.node_id.clone(),
                number: claim.number,
                held: claim.fence,
                current: row.fence,
                release: row.release.clone(),
            });
        }
        Ok(row)
    }

    /// Seal the finished attempt of a claim this controller still holds and
    /// release the claim, in one transaction. A completion for a released,
    /// cancelled, or reclaimed claim is refused and nothing is written.
    pub fn complete(
        &self,
        store: &mut Store,
        claim: &Claim,
        attempt: &Attempt,
    ) -> Result<(), SchedulerError> {
        if attempt.run_id != claim.run_id
            || attempt.node.node_id != claim.node_id
            || attempt.number != claim.number
        {
            return Err(SchedulerError::AttemptMismatch(format!(
                "attempt {} does not describe claim {}/{}#{}",
                attempt.attempt_ref(),
                claim.run_id,
                claim.node_id,
                claim.number
            )));
        }
        let expected_node = self.graph.spec.node_ref(&claim.node_id).ok_or_else(|| {
            SchedulerError::Invalid(format!("node {} is not in the graph", claim.node_id))
        })?;
        if attempt.node != expected_node {
            return Err(SchedulerError::AttemptMismatch(format!(
                "attempt is about {}, the graph has {}",
                attempt.node, expected_node
            )));
        }
        let execution = attempt.execution.as_ref().ok_or_else(|| {
            SchedulerError::AttemptMismatch("a completion must carry an execution record".into())
        })?;
        let mut failure: Option<SchedulerError> = None;
        let result = store.transaction(|tx| {
            let row = match self.held_claim(tx, claim) {
                Ok(row) => row,
                Err(e) => {
                    failure = Some(e);
                    return Err(StoreError::Corrupt("stale claim".into()));
                }
            };
            if row.dependency_receipts != attempt.dependency_receipts {
                failure = Some(SchedulerError::SnapshotMismatch);
                return Err(StoreError::Corrupt("snapshot mismatch".into()));
            }
            let events = tx.events(&claim.run_id)?;
            let state = match RunState::replay(claim.run_id.clone(), self.spec, &events) {
                Ok(state) => state,
                Err(e) => {
                    failure = Some(e.into());
                    return Err(StoreError::Corrupt("replay failed".into()));
                }
            };
            let event = NodeEvent::ExecutionFinished {
                attempt: claim.number,
                outcome: execution.outcome,
                result_digest: attempt.result.as_ref().map(|r| r.digest.clone()),
            };
            if let Err(e) = state
                .node(&claim.node_id)
                .expect("admitted node")
                .apply(&event)
            {
                failure = Some(e.into());
                return Err(StoreError::Corrupt("illegal completion".into()));
            }
            tx.seal_attempt(attempt)?;
            let released = tx.release_claim(
                &claim.run_id,
                &claim.node_id,
                claim.number.get(),
                claim.fence,
                "completed",
            )?;
            if !released {
                failure = Some(SchedulerError::Stale {
                    node: claim.node_id.clone(),
                    number: claim.number,
                    held: claim.fence,
                    current: row.fence,
                    release: row.release,
                });
                return Err(StoreError::Corrupt("claim vanished".into()));
            }
            Ok(())
        });
        match (result, failure) {
            (_, Some(e)) => Err(e),
            (Ok(()), None) => Ok(()),
            (Err(e), None) => Err(e.into()),
        }
    }

    /// Cancel a node: release its active claim (if any) and record
    /// `node_cancelled`, in one transaction. Any later completion of the
    /// released claim is refused.
    pub fn cancel(&self, store: &mut Store, node_id: &NodeId) -> Result<(), SchedulerError> {
        self.node(node_id)?;
        let run_id = self.run.run_id.clone();
        let mut failure: Option<SchedulerError> = None;
        let result = store.transaction(|tx| {
            let events = tx.events(&run_id)?;
            let state = match RunState::replay(run_id.clone(), self.spec, &events) {
                Ok(state) => state,
                Err(e) => {
                    failure = Some(e.into());
                    return Err(StoreError::Corrupt("replay failed".into()));
                }
            };
            let node_state = state.node(node_id).expect("admitted node");
            if let Err(e) = node_state.apply(&NodeEvent::Cancelled) {
                failure = Some(e.into());
                return Err(StoreError::Corrupt("illegal cancel".into()));
            }
            for row in tx.active_claims()? {
                if row.run_id == run_id && &row.node_id == node_id {
                    tx.release_claim(&run_id, node_id, row.number.get(), row.fence, "cancelled")?;
                }
            }
            tx.append_event(
                &run_id,
                Some(node_id),
                kinds::NODE_CANCELLED,
                &serde_json::json!({ "by": self.controller.as_str() }),
            )?;
            Ok(())
        });
        match (result, failure) {
            (_, Some(e)) => Err(e),
            (Ok(()), None) => Ok(()),
            (Err(e), None) => Err(e.into()),
        }
    }

    /// Take over the active claim of a node whose owner is presumed dead:
    /// seal the attempt as failed with [`RECLAIMED_ERROR_CODE`] and release
    /// the claim as `reclaimed`, in one transaction. The previous owner's
    /// completion is refused from then on.
    pub fn reclaim(
        &self,
        store: &mut Store,
        node_id: &NodeId,
        now: &Timestamp,
    ) -> Result<AttemptNumber, SchedulerError> {
        let node_ref = self.graph.spec.node_ref(node_id).ok_or_else(|| {
            SchedulerError::Invalid(format!("node {node_id} is not in the graph"))
        })?;
        let run_id = self.run.run_id.clone();
        let mut failure: Option<SchedulerError> = None;
        let result = store.transaction(|tx| {
            let row = tx
                .active_claims()?
                .into_iter()
                .find(|r| r.run_id == run_id && &r.node_id == node_id);
            let Some(row) = row else {
                failure = Some(SchedulerError::Invalid(format!(
                    "node {node_id} has no active claim to reclaim"
                )));
                return Err(StoreError::Corrupt("no claim".into()));
            };
            let events = tx.events(&run_id)?;
            let state = match RunState::replay(run_id.clone(), self.spec, &events) {
                Ok(state) => state,
                Err(e) => {
                    failure = Some(e.into());
                    return Err(StoreError::Corrupt("replay failed".into()));
                }
            };
            let started_at = Timestamp::new(row.claimed_at.clone())
                .map_err(|e| StoreError::Corrupt(e.to_string()))?;
            let attempt = Attempt {
                record: RecordKind::Attempt,
                schema_version: SchemaVersion(1),
                run_id: run_id.clone(),
                node: node_ref.clone(),
                number: row.number,
                owner: row.owner.clone(),
                started_at,
                finished_at: Some(now.clone()),
                repair_hint: None,
                input_manifest: Vec::new(),
                dependency_receipts: row.dependency_receipts.clone(),
                result: None,
                produced_evidence: Vec::new(),
                execution: Some(Execution {
                    outcome: ExecutionOutcome::Failed,
                    error_code: Some(Ident::new(RECLAIMED_ERROR_CODE).expect("constant")),
                    error_text: Some(format!(
                        "claim held by {} reclaimed by {}",
                        row.owner, self.controller
                    )),
                }),
                verifier_receipt: None,
            };
            let event = NodeEvent::ExecutionFinished {
                attempt: row.number,
                outcome: ExecutionOutcome::Failed,
                result_digest: None,
            };
            if let Err(e) = state.node(node_id).expect("admitted node").apply(&event) {
                failure = Some(e.into());
                return Err(StoreError::Corrupt("illegal reclaim".into()));
            }
            tx.seal_attempt(&attempt)?;
            tx.release_claim(&run_id, node_id, row.number.get(), row.fence, "reclaimed")?;
            Ok(row.number)
        });
        match (result, failure) {
            (_, Some(e)) => Err(e),
            (Ok(number), None) => Ok(number),
            (Err(e), None) => Err(e.into()),
        }
    }
}
