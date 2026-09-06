//! Retry limits and persistent budgets.
//!
//! Every started attempt counts, including crashes, timeouts, and reclaims,
//! because the count is derived from the run's dispatch events rather than
//! kept in memory. A node may be retried automatically only while its
//! failure class is allowed by its retry policy, it has not reached its
//! attempt cap, the graph-wide attempt budget (this run plus every run in
//! its budget lineage) has room, and the deadline has not passed. Otherwise
//! the node is exhausted and routed by its policy: `cancel` is applied here;
//! `gate` is reported for the gate-packet step to act on. A retry may carry
//! a repair hint and may narrow optional evidence ports or restrict a port's
//! sources to a subset; it can never drop a required port, add a source, or
//! touch the acceptance contract, which lives on the node and is not an
//! input to any of this.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::contracts::{
    ExecutionOutcome, FailureClass, GraphRun, GraphSpec, Ident, NodeId, NodeSpec, NodeStatus,
    OnExhaustion, PortSpec, RunId, Timestamp,
};
use crate::state::{NodeEvent, RunState, TransitionError, kinds};
use crate::store::{Ledger, Store, StoreError};

/// The graph-wide budget as it stands for one run at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetStatus {
    /// Attempts the graph revision allows in total.
    pub total_attempts: u32,
    /// Attempts started in this run.
    pub consumed_here: u32,
    /// Attempts started in every run of the budget lineage.
    pub consumed_by_lineage: u32,
    /// The lineage walked, nearest first.
    pub lineage: Vec<RunId>,
    /// The deadline, if the graph declares one.
    pub deadline: Option<Timestamp>,
    /// Whether the deadline has passed at the moment of the status.
    pub deadline_passed: bool,
}

impl BudgetStatus {
    /// Attempts consumed in total.
    pub fn consumed(&self) -> u32 {
        self.consumed_here.saturating_add(self.consumed_by_lineage)
    }

    /// Attempts left, zero when exhausted.
    pub fn remaining(&self) -> u32 {
        self.total_attempts.saturating_sub(self.consumed())
    }

    /// Why no new attempt may start, if the budget forbids one.
    pub fn exhaustion(&self) -> Option<ExhaustReason> {
        if self.deadline_passed {
            return self.deadline.clone().map(ExhaustReason::DeadlinePassed);
        }
        if self.remaining() == 0 {
            return Some(ExhaustReason::GraphBudget {
                total: self.total_attempts,
                consumed: self.consumed(),
            });
        }
        None
    }
}

/// Why a node cannot be retried automatically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ExhaustReason {
    /// The node has started its maximum number of attempts.
    NodeCap {
        /// The cap, including the first attempt.
        max_attempts: u32,
        /// Attempts started.
        started: u32,
    },
    /// The failure class is not one the policy allows another attempt for.
    ClassNotRetryable {
        /// The class of the last outcome.
        class: FailureClass,
    },
    /// The graph-wide attempt budget is spent, lineage included.
    GraphBudget {
        /// The budget.
        total: u32,
        /// Attempts consumed across the lineage.
        consumed: u32,
    },
    /// The graph deadline has passed.
    DeadlinePassed(Timestamp),
}

impl fmt::Display for ExhaustReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExhaustReason::NodeCap {
                max_attempts,
                started,
            } => write!(
                f,
                "node started {started} of {max_attempts} allowed attempts"
            ),
            ExhaustReason::ClassNotRetryable { class } => {
                write!(
                    f,
                    "the retry policy does not allow another attempt after {class:?}"
                )
            }
            ExhaustReason::GraphBudget { total, consumed } => {
                write!(
                    f,
                    "graph budget of {total} attempts is spent ({consumed} consumed)"
                )
            }
            ExhaustReason::DeadlinePassed(t) => write!(f, "graph deadline {t} has passed"),
        }
    }
}

/// What policy says about the node's next attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    /// Another attempt may start.
    Retry {
        /// The class of the outcome being retried.
        class: FailureClass,
        /// Attempts started so far.
        started: u32,
        /// The node's cap.
        max_attempts: u32,
    },
    /// No automatic retry; the policy's exhaustion route applies.
    Exhausted {
        /// Why.
        reason: ExhaustReason,
        /// Where the policy sends the node.
        route: OnExhaustion,
    },
}

/// A retry's narrowing: never a change to what must be delivered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Narrowing {
    /// Guidance for the next attempt; it never edits the contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_hint: Option<String>,
    /// Optional evidence ports the next attempt will not receive.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped_ports: Vec<Ident>,
    /// Ports whose allowed sources shrink to the listed subset.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restricted_sources: Vec<(Ident, Vec<String>)>,
}

/// Why a narrowing is not a narrowing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NarrowingError {
    /// The port is not declared on the node.
    UnknownPort(Ident),
    /// The port is required; only optional ports can be dropped.
    RequiredPort(Ident),
    /// The port is listed twice.
    DuplicatePort(Ident),
    /// A restricted port is also dropped.
    RestrictedAndDropped(Ident),
    /// A restriction would leave a port with no source.
    EmptySources(Ident),
    /// A restriction names a source outside the port's approved scope; that
    /// widens the contract and needs a new approved revision.
    WidensScope {
        /// The port.
        port: Ident,
        /// The source not in the contract.
        source: String,
    },
}

impl fmt::Display for NarrowingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NarrowingError::UnknownPort(p) => write!(f, "port {p} is not declared on the node"),
            NarrowingError::RequiredPort(p) => {
                write!(f, "port {p} is required and cannot be dropped")
            }
            NarrowingError::DuplicatePort(p) => write!(f, "port {p} is listed more than once"),
            NarrowingError::RestrictedAndDropped(p) => {
                write!(f, "port {p} is both dropped and restricted")
            }
            NarrowingError::EmptySources(p) => write!(f, "port {p} would be left with no source"),
            NarrowingError::WidensScope { port, source } => write!(
                f,
                "source {source:?} is outside port {port}'s approved scope; widening needs a new approved contract"
            ),
        }
    }
}

impl std::error::Error for NarrowingError {}

/// Check a narrowing against the node contract and return the evidence
/// ports the next attempt may use. The acceptance contract is not an input
/// and cannot be affected.
pub fn check_narrowing(
    node: &NodeSpec,
    narrowing: &Narrowing,
) -> Result<Vec<PortSpec>, NarrowingError> {
    let mut seen: BTreeSet<&Ident> = BTreeSet::new();
    for port in &narrowing.dropped_ports {
        if !seen.insert(port) {
            return Err(NarrowingError::DuplicatePort(port.clone()));
        }
        let declared = node
            .evidence_ports
            .iter()
            .find(|p| &p.name == port)
            .ok_or_else(|| NarrowingError::UnknownPort(port.clone()))?;
        if declared.required {
            return Err(NarrowingError::RequiredPort(port.clone()));
        }
    }
    let mut ports: Vec<PortSpec> = node
        .evidence_ports
        .iter()
        .filter(|p| !narrowing.dropped_ports.contains(&p.name))
        .cloned()
        .collect();
    let mut restricted: BTreeSet<&Ident> = BTreeSet::new();
    for (port, sources) in &narrowing.restricted_sources {
        if !restricted.insert(port) {
            return Err(NarrowingError::DuplicatePort(port.clone()));
        }
        if narrowing.dropped_ports.contains(port) {
            return Err(NarrowingError::RestrictedAndDropped(port.clone()));
        }
        let effective = ports
            .iter_mut()
            .find(|p| &p.name == port)
            .ok_or_else(|| NarrowingError::UnknownPort(port.clone()))?;
        if sources.is_empty() {
            return Err(NarrowingError::EmptySources(port.clone()));
        }
        for source in sources {
            if !effective.allowed_source_scope.contains(source) {
                return Err(NarrowingError::WidensScope {
                    port: port.clone(),
                    source: source.clone(),
                });
            }
        }
        effective.allowed_source_scope = sources.clone();
    }
    Ok(ports)
}

/// Why a budget operation failed.
#[derive(Debug)]
pub enum BudgetError {
    /// The store failed.
    Store(StoreError),
    /// The reducer refused the event the operation would record.
    Transition(TransitionError),
    /// The narrowing is not a narrowing.
    Narrowing(NarrowingError),
    /// The node is not in a state that has an outcome to retry.
    NoOutcome {
        /// The node.
        node: NodeId,
        /// Its status.
        status: NodeStatus,
    },
    /// The graph, run, or lineage is not usable.
    Invalid(String),
}

impl fmt::Display for BudgetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BudgetError::Store(e) => write!(f, "{e}"),
            BudgetError::Transition(e) => write!(f, "{e}"),
            BudgetError::Narrowing(e) => write!(f, "{e}"),
            BudgetError::NoOutcome { node, status } => {
                write!(
                    f,
                    "node {node} is {status}; there is no failed or rejected attempt to retry"
                )
            }
            BudgetError::Invalid(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for BudgetError {}

impl From<StoreError> for BudgetError {
    fn from(e: StoreError) -> Self {
        BudgetError::Store(e)
    }
}

impl From<TransitionError> for BudgetError {
    fn from(e: TransitionError) -> Self {
        BudgetError::Transition(e)
    }
}

impl From<NarrowingError> for BudgetError {
    fn from(e: NarrowingError) -> Self {
        BudgetError::Narrowing(e)
    }
}

/// The graph-wide budget of `run` at `now`: this run's dispatches plus those
/// of every run in its budget lineage, against the graph's declared budget.
pub fn budget_status(
    ledger: &dyn Ledger,
    spec: &GraphSpec,
    run: &GraphRun,
    now: &Timestamp,
) -> Result<BudgetStatus, BudgetError> {
    let consumed_here = ledger.dispatch_count(&run.run_id)?;
    let mut lineage = Vec::new();
    let mut consumed_by_lineage: u32 = 0;
    let mut next = run.budget_lineage_ref.clone();
    let mut seen: BTreeSet<RunId> = BTreeSet::from([run.run_id.clone()]);
    while let Some(ancestor_id) = next {
        if !seen.insert(ancestor_id.clone()) {
            return Err(BudgetError::Invalid(format!(
                "budget lineage of run {} loops through {ancestor_id}",
                run.run_id
            )));
        }
        let ancestor = ledger.run(&ancestor_id)?.ok_or_else(|| {
            BudgetError::Invalid(format!(
                "budget lineage of run {} names unknown run {ancestor_id}",
                run.run_id
            ))
        })?;
        consumed_by_lineage =
            consumed_by_lineage.saturating_add(ledger.dispatch_count(&ancestor_id)?);
        lineage.push(ancestor_id);
        next = ancestor.budget_lineage_ref;
    }
    let deadline = spec.budget.deadline.clone();
    let deadline_passed = deadline
        .as_ref()
        .is_some_and(|deadline| !now.is_before(deadline));
    Ok(BudgetStatus {
        total_attempts: spec.budget.total_attempts,
        consumed_here,
        consumed_by_lineage,
        lineage,
        deadline,
        deadline_passed,
    })
}

/// The failure class of a node's current outcome, from its status and, for
/// failures, the sealed attempt's execution outcome.
pub fn failure_class(
    ledger: &dyn Ledger,
    run_id: &RunId,
    node_id: &NodeId,
    state: &RunState,
) -> Result<Option<FailureClass>, BudgetError> {
    let node_state = state
        .node(node_id)
        .ok_or_else(|| BudgetError::Invalid(format!("node {node_id} is not in the graph")))?;
    Ok(match node_state.status {
        NodeStatus::Rejected => Some(FailureClass::Rejected),
        NodeStatus::Failed => {
            let number = node_state.current_attempt.map(|n| n.get()).unwrap_or(0);
            let outcome = ledger
                .attempt(run_id, node_id, number)?
                .and_then(|a| a.execution)
                .map(|e| e.outcome);
            Some(match outcome {
                Some(ExecutionOutcome::TimedOut) => FailureClass::Timeout,
                _ => FailureClass::Infrastructure,
            })
        }
        _ => None,
    })
}

/// What policy says about retrying `node_id` now. Nothing is written.
pub fn disposition(
    ledger: &dyn Ledger,
    spec: &GraphSpec,
    run: &GraphRun,
    state: &RunState,
    node_id: &NodeId,
    now: &Timestamp,
) -> Result<Disposition, BudgetError> {
    let node = spec
        .nodes
        .iter()
        .find(|n| &n.id == node_id)
        .ok_or_else(|| BudgetError::Invalid(format!("node {node_id} is not in the graph")))?;
    let node_state = state.node(node_id).expect("node checked above");
    let class = failure_class(ledger, &run.run_id, node_id, state)?.ok_or_else(|| {
        BudgetError::NoOutcome {
            node: node_id.clone(),
            status: node_state.status,
        }
    })?;
    let policy = &node.retry_policy;
    let started = node_state.attempts_started;
    let exhausted = |reason| Disposition::Exhausted {
        reason,
        route: policy.on_exhaustion,
    };
    if !policy.allowed_failure_classes.contains(&class) {
        return Ok(exhausted(ExhaustReason::ClassNotRetryable { class }));
    }
    if started >= policy.max_attempts {
        return Ok(exhausted(ExhaustReason::NodeCap {
            max_attempts: policy.max_attempts,
            started,
        }));
    }
    if let Some(reason) = budget_status(ledger, spec, run, now)?.exhaustion() {
        return Ok(exhausted(reason));
    }
    Ok(Disposition::Retry {
        class,
        started,
        max_attempts: policy.max_attempts,
    })
}

/// Decide and apply the retry policy for a failed or rejected node in one
/// transaction. A permitted retry records `retry_allowed` with the checked
/// narrowing; an exhausted node routed to `cancel` is cancelled; an
/// exhausted node routed to `gate` is left as it is for the gate-packet step,
/// and the disposition says so.
pub fn apply_retry_policy(
    store: &mut Store,
    spec: &GraphSpec,
    run: &GraphRun,
    node_id: &NodeId,
    narrowing: &Narrowing,
    now: &Timestamp,
) -> Result<Disposition, BudgetError> {
    let node = spec
        .nodes
        .iter()
        .find(|n| &n.id == node_id)
        .ok_or_else(|| BudgetError::Invalid(format!("node {node_id} is not in the graph")))?;
    let mut failure: Option<BudgetError> = None;
    let result = store.transaction(|tx| {
        let events = tx.events(&run.run_id)?;
        let state = match RunState::replay(run.run_id.clone(), spec, &events) {
            Ok(state) => state,
            Err(e) => {
                failure = Some(e.into());
                return Err(StoreError::Corrupt("replay failed".into()));
            }
        };
        let decision = match disposition(tx, spec, run, &state, node_id, now) {
            Ok(decision) => decision,
            Err(e) => {
                failure = Some(e);
                return Err(StoreError::Corrupt("no disposition".into()));
            }
        };
        let node_state = state.node(node_id).expect("node checked above");
        match &decision {
            Disposition::Retry { .. } => {
                if let Err(e) = check_narrowing(node, narrowing) {
                    failure = Some(e.into());
                    return Err(StoreError::Corrupt("narrowing refused".into()));
                }
                if let Err(e) = node_state.apply(&NodeEvent::RetryAllowed) {
                    failure = Some(e.into());
                    return Err(StoreError::Corrupt("illegal retry".into()));
                }
                let payload = serde_json::json!({
                    "repair_hint": narrowing.repair_hint,
                    "dropped_ports": narrowing.dropped_ports,
                    "restricted_sources": narrowing.restricted_sources,
                });
                tx.append_event(&run.run_id, Some(node_id), kinds::RETRY_ALLOWED, &payload)?;
            }
            Disposition::Exhausted {
                route: OnExhaustion::Cancel,
                reason,
            } => {
                if let Err(e) = node_state.apply(&NodeEvent::Cancelled) {
                    failure = Some(e.into());
                    return Err(StoreError::Corrupt("illegal cancel".into()));
                }
                tx.append_event(
                    &run.run_id,
                    Some(node_id),
                    kinds::NODE_CANCELLED,
                    &serde_json::json!({ "exhausted": reason }),
                )?;
            }
            Disposition::Exhausted {
                route: OnExhaustion::Gate,
                ..
            } => {}
        }
        Ok(decision)
    });
    match (result, failure) {
        (_, Some(e)) => Err(e),
        (Ok(decision), None) => Ok(decision),
        (Err(e), None) => Err(e.into()),
    }
}

/// The repair plan recorded by the most recent `retry_allowed` event for a
/// node since its last dispatch, if any.
pub fn pending_narrowing(
    events: &[crate::store::StoredEvent],
    node_id: &NodeId,
) -> Option<Narrowing> {
    let mut pending = None;
    for event in events {
        if event.node_id.as_ref() != Some(node_id) {
            continue;
        }
        match event.kind.as_str() {
            kinds::RETRY_ALLOWED => {
                pending = serde_json::from_value(event.payload.clone()).ok();
            }
            kinds::ATTEMPT_DISPATCHED => pending = None,
            _ => {}
        }
    }
    pending
}
