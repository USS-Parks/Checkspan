//! Issue and admit verifier receipts.
//!
//! A receipt is the only path to acceptance, and admission is where every
//! binding is enforced: the verifier the contract pinned, the policy, the
//! sealed current attempt, the exact result digest, the full verification
//! context, freshness, and — for an accepting verdict — the pre-acceptance
//! recheck of every pinned dependency. Whatever fails, nothing is written;
//! the check runs inside the same transaction as the write, against the
//! ledger it will write to.
//!
//! Admitting a receipt validates its bindings, not its issuer's honesty.
//! The provenance level says who vouches for the verdict and travels with
//! the receipt; admission never upgrades it.

use std::fmt;

use crate::contracts::{
    Attempt, ExecutionOutcome, GraphRun, GraphSpec, Ident, NodeSpec, ReceiptProvenance, RecordKind,
    SchemaVersion, Timestamp, Validity, Verdict, VerifierReceipt,
};
use crate::deps::DependencyService;
use crate::digests::{VerificationContext, context_digest, input_manifest_digest};
use crate::state::RunState;
use crate::store::{Ledger, Store, StoreError};
use crate::verifier_host::VerifierResponse;

/// Why a response could not become a receipt.
#[derive(Debug)]
pub enum IssueError {
    /// The response is not about this attempt.
    WrongAttempt,
    /// The response's verifier is not the one the contract pinned.
    WrongVerifier,
    /// The attempt is not sealed as completed with a result.
    NotCompleted,
    /// Canonical bytes could not be produced.
    Canonical(String),
}

impl fmt::Display for IssueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IssueError::WrongAttempt => f.write_str("the response names a different attempt"),
            IssueError::WrongVerifier => {
                f.write_str("the response's verifier is not the pinned verifier")
            }
            IssueError::NotCompleted => {
                f.write_str("the attempt is not sealed as completed with a result")
            }
            IssueError::Canonical(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for IssueError {}

/// Build a receipt from a protocol response, binding it to the sealed
/// attempt and the node contract. The contract, not the response, is the
/// authority for the verifier identity; a response from any other verifier
/// is refused here.
#[allow(clippy::too_many_arguments)]
pub fn issue(
    response: &VerifierResponse,
    node: &NodeSpec,
    attempt: &Attempt,
    receipt_id: Ident,
    subject: String,
    issuer: String,
    checked_at: Timestamp,
    validity: Validity,
) -> Result<VerifierReceipt, IssueError> {
    if response.attempt != attempt.attempt_ref() {
        return Err(IssueError::WrongAttempt);
    }
    if response.verifier != node.acceptance.verifier {
        return Err(IssueError::WrongVerifier);
    }
    let result = match (&attempt.execution, &attempt.result) {
        (Some(execution), Some(result)) if execution.outcome == ExecutionOutcome::Completed => {
            result
        }
        _ => return Err(IssueError::NotCompleted),
    };
    let manifest = input_manifest_digest(&attempt.input_manifest, &attempt.dependency_receipts)
        .map_err(|e| IssueError::Canonical(e.to_string()))?;
    let context = context_digest(&VerificationContext {
        attempt: &response.attempt,
        node,
        input_manifest_digest: &manifest,
        dependency_receipts: &attempt.dependency_receipts,
        result_digest: &result.digest,
        produced_evidence: &attempt.produced_evidence,
    })
    .map_err(|e| IssueError::Canonical(e.to_string()))?;
    Ok(VerifierReceipt {
        record: RecordKind::VerifierReceipt,
        schema_version: SchemaVersion(1),
        id: receipt_id,
        attempt: response.attempt.clone(),
        subject,
        result_digest: result.digest.clone(),
        context_digest: context,
        verifier: node.acceptance.verifier.clone(),
        policy_ref: node.acceptance.policy_ref.clone(),
        verdict: response.verdict,
        reason_code: response.reason_code.clone(),
        reason_text: response.reason_text.clone(),
        retry_hint: None,
        checked_at,
        validity,
        provenance: ReceiptProvenance {
            level: crate::contracts::ProvenanceLevel::LocalController,
            issuer,
            attestation_ref: None,
        },
    })
}

/// Why a receipt was refused. Nothing was written.
#[derive(Debug)]
pub enum AdmitError {
    /// The receipt's own binding rules fail.
    Malformed(String),
    /// The receipt names a run this admission is not for.
    WrongRun,
    /// The receipt names a node or revision the graph does not have.
    WrongNode,
    /// The receipt's verifier is not the one the contract pinned.
    WrongVerifier {
        /// What the contract pins.
        pinned: String,
        /// What the receipt carries.
        offered: String,
    },
    /// The receipt's policy is not the contract's.
    WrongPolicy,
    /// No sealed attempt matches the receipt.
    UnknownAttempt,
    /// The attempt did not complete with a result; execution outcomes are
    /// never verdicts and a failed attempt cannot be accepted.
    NotCompleted,
    /// The receipt is not about the node's current checking attempt: a
    /// newer attempt exists, a receipt was already admitted, or the node is
    /// past checking.
    Stale {
        /// What the node's state is now.
        state: String,
    },
    /// A receipt with this id is already admitted.
    Duplicate,
    /// The receipt's result digest is not the sealed attempt's result.
    WrongResult,
    /// The receipt's subject is not the attempt's frozen candidate subject.
    WrongSubject {
        /// The subject frozen at dispatch.
        expected: String,
        /// The subject the receipt carries.
        offered: String,
    },
    /// The receipt's context digest does not cover the sealed attempt's
    /// exact inputs, contract, and result.
    ContextMismatch,
    /// The receipt is already past its validity.
    Expired,
    /// An accepting receipt's pinned dependencies are no longer admissible.
    DependencyBlocked(String),
    /// The store refused.
    Store(StoreError),
}

impl fmt::Display for AdmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdmitError::Malformed(e) => write!(f, "receipt is malformed: {e}"),
            AdmitError::WrongRun => f.write_str("receipt names another run"),
            AdmitError::WrongNode => f.write_str("receipt names a node the graph does not have"),
            AdmitError::WrongVerifier { pinned, offered } => {
                write!(
                    f,
                    "contract pins verifier {pinned}; receipt carries {offered}"
                )
            }
            AdmitError::WrongPolicy => f.write_str("receipt's policy is not the contract's"),
            AdmitError::UnknownAttempt => f.write_str("no sealed attempt matches the receipt"),
            AdmitError::NotCompleted => {
                f.write_str("the attempt did not complete with a result; no verdict applies")
            }
            AdmitError::Stale { state } => {
                write!(
                    f,
                    "receipt is not about the current checking attempt ({state})"
                )
            }
            AdmitError::Duplicate => f.write_str("a receipt with this id is already admitted"),
            AdmitError::WrongResult => {
                f.write_str("receipt's result digest is not the sealed result")
            }
            AdmitError::WrongSubject { expected, offered } => {
                write!(
                    f,
                    "attempt froze subject {expected}; receipt carries {offered}"
                )
            }
            AdmitError::ContextMismatch => {
                f.write_str("receipt's context digest does not cover the sealed context")
            }
            AdmitError::Expired => f.write_str("receipt is past its validity"),
            AdmitError::DependencyBlocked(reason) => {
                write!(f, "pinned dependencies are no longer admissible: {reason}")
            }
            AdmitError::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AdmitError {}

impl From<StoreError> for AdmitError {
    fn from(e: StoreError) -> Self {
        AdmitError::Store(e)
    }
}

/// Admit one receipt: validate every binding against the ledger and, in the
/// same transaction, record the receipt and its `receipt_admitted` event.
/// On any refusal nothing is written and the node's acceptance is unchanged.
pub fn admit(
    store: &mut Store,
    spec: &GraphSpec,
    run: &GraphRun,
    receipt: &VerifierReceipt,
    now: &Timestamp,
) -> Result<(), AdmitError> {
    let shape = receipt.check_bindings();
    if !shape.is_empty() {
        let text = shape
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(AdmitError::Malformed(text));
    }
    if receipt.attempt.run_id != run.run_id {
        return Err(AdmitError::WrongRun);
    }
    let node_id = &receipt.attempt.node.node_id;
    let node = match spec.nodes.iter().find(|n| &n.id == node_id) {
        Some(node) if spec.node_ref(node_id).as_ref() == Some(&receipt.attempt.node) => node,
        _ => return Err(AdmitError::WrongNode),
    };
    if receipt.verifier != node.acceptance.verifier {
        return Err(AdmitError::WrongVerifier {
            pinned: format!(
                "{}@{} {}",
                node.acceptance.verifier.id,
                node.acceptance.verifier.version,
                node.acceptance.verifier.digest
            ),
            offered: format!(
                "{}@{} {}",
                receipt.verifier.id, receipt.verifier.version, receipt.verifier.digest
            ),
        });
    }
    if receipt.policy_ref != node.acceptance.policy_ref {
        return Err(AdmitError::WrongPolicy);
    }
    if let Some(valid_until) = &receipt.validity.valid_until
        && valid_until.is_before(now)
    {
        return Err(AdmitError::Expired);
    }

    let mut failure: Option<AdmitError> = None;
    let outcome = store.transaction(|tx| {
        let mut refuse = |error: AdmitError| {
            failure = Some(error);
            Err(StoreError::Corrupt("receipt refused".into()))
        };
        if tx.receipt(&run.run_id, receipt.id.as_str())?.is_some() {
            return refuse(AdmitError::Duplicate);
        }
        let Some(attempt) = tx.attempt(&run.run_id, node_id, receipt.attempt.number.get())? else {
            return refuse(AdmitError::UnknownAttempt);
        };
        let result = match (&attempt.execution, &attempt.result) {
            (Some(execution), Some(result)) if execution.outcome == ExecutionOutcome::Completed => {
                result.clone()
            }
            _ => return refuse(AdmitError::NotCompleted),
        };
        if receipt.result_digest != result.digest {
            return refuse(AdmitError::WrongResult);
        }
        if let Some(expected) = frozen_subject(&attempt)
            && expected != receipt.subject
        {
            return refuse(AdmitError::WrongSubject {
                expected,
                offered: receipt.subject.clone(),
            });
        }
        let manifest =
            match input_manifest_digest(&attempt.input_manifest, &attempt.dependency_receipts) {
                Ok(digest) => digest,
                Err(e) => return refuse(AdmitError::Malformed(e.to_string())),
            };
        let context = context_digest(&VerificationContext {
            attempt: &receipt.attempt,
            node,
            input_manifest_digest: &manifest,
            dependency_receipts: &attempt.dependency_receipts,
            result_digest: &result.digest,
            produced_evidence: &attempt.produced_evidence,
        });
        match context {
            Ok(digest) if digest == receipt.context_digest => {}
            Ok(_) => return refuse(AdmitError::ContextMismatch),
            Err(e) => return refuse(AdmitError::Malformed(e.to_string())),
        }

        let events = tx.events(&run.run_id)?;
        let state = match RunState::replay(run.run_id.clone(), spec, &events) {
            Ok(state) => state,
            Err(e) => return refuse(AdmitError::Malformed(e.to_string())),
        };
        let node_state = state.node(node_id);
        let current_checking = node_state
            .map(|s| s.checking && s.current_attempt == Some(receipt.attempt.number))
            .unwrap_or(false);
        if !current_checking {
            let described = node_state
                .map(|s| {
                    format!(
                        "status {}, attempt {:?}, checking {}",
                        s.status.as_str(),
                        s.current_attempt.map(|n| n.get()),
                        s.checking
                    )
                })
                .unwrap_or_else(|| "no state".to_owned());
            return refuse(AdmitError::Stale { state: described });
        }

        if receipt.verdict == Verdict::Accept {
            let service = DependencyService::new(spec, run, &state, tx, now);
            if let Err(blockers) = service.recheck_refs(node_id, &attempt.dependency_receipts) {
                let text = blockers
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ");
                return refuse(AdmitError::DependencyBlocked(text));
            }
        }

        tx.insert_receipt(receipt)?;
        tx.append_event(
            &receipt.attempt.run_id,
            Some(node_id),
            "receipt_admitted",
            &serde_json::json!({
                "receipt_id": receipt.id,
                "number": receipt.attempt.number,
                "verdict": receipt.verdict,
                "result_digest": receipt.result_digest,
            }),
        )?;
        Ok(())
    });
    match outcome {
        Ok(()) => Ok(()),
        Err(e) => Err(failure.unwrap_or(AdmitError::Store(e))),
    }
}

/// The candidate subject the attempt froze, when its evidence names one.
fn frozen_subject(attempt: &Attempt) -> Option<String> {
    attempt
        .input_manifest
        .iter()
        .find(|reference| reference.subject.starts_with("patch_subject:"))
        .map(|reference| reference.subject.clone())
}
