//! Attempts and verifier receipts: what was executed on which exact inputs,
//! and what the designated verifier concluded about the exact result.
//!
//! Execution outcome and verifier verdict are separate vocabularies. A crash,
//! timeout, or cancellation is an execution outcome and can never be read as
//! a verdict; a rejection is a verdict and requires a completed execution.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::evidence::EvidenceRef;
use super::graph::NodeRef;
use super::ids::{AttemptNumber, Digest, Ident, RunId, Timestamp};
use super::node::{PolicyRef, TypeRef, VerifierRef};
use super::record::{Record, RecordKind, SchemaVersion};

/// Exact reference to one attempt.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptRef {
    /// The run.
    pub run_id: RunId,
    /// The node.
    pub node: NodeRef,
    /// The attempt number within that run.
    pub number: AttemptNumber,
}

impl fmt::Display for AttemptRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}#{}", self.run_id, self.node, self.number)
    }
}

/// Reference to a verifier receipt.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptRef {
    /// The receipt.
    pub id: Ident,
    /// The run the receipt was issued in.
    pub run_id: RunId,
    /// The node the receipt is about.
    pub node: NodeRef,
}

impl fmt::Display for ReceiptRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}/{})", self.id, self.run_id, self.node)
    }
}

/// How an attempt's execution ended. None of these is a verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionOutcome {
    /// Worker and verifier both ran to completion.
    Completed,
    /// The worker or verifier crashed, or a required service was unavailable.
    Failed,
    /// The attempt exceeded its time limit.
    TimedOut,
    /// The attempt was cancelled before completion.
    Cancelled,
}

impl ExecutionOutcome {
    /// Every outcome, for coverage checks.
    pub const ALL: [ExecutionOutcome; 4] = [
        ExecutionOutcome::Completed,
        ExecutionOutcome::Failed,
        ExecutionOutcome::TimedOut,
        ExecutionOutcome::Cancelled,
    ];

    /// The `outcome` field text.
    pub fn as_str(self) -> &'static str {
        match self {
            ExecutionOutcome::Completed => "completed",
            ExecutionOutcome::Failed => "failed",
            ExecutionOutcome::TimedOut => "timed_out",
            ExecutionOutcome::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for ExecutionOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Execution record of a finished attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    /// How execution ended.
    pub outcome: ExecutionOutcome,
    /// Operational error class, when execution did not complete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<Ident>,
    /// Bounded human-readable detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_text: Option<String>,
}

/// The candidate an attempt produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultRef {
    /// The declared result type.
    #[serde(rename = "type")]
    pub result_type: TypeRef,
    /// Where the artifact is stored.
    pub artifact_ref: String,
    /// Digest of the exact artifact bytes.
    pub digest: Digest,
}

/// One attempt of one node within one run.
///
/// An attempt carries no acceptance field. Its only path to acceptance is a
/// `verifier_receipt` whose receipt is bound to this exact attempt and result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    /// Always `attempt`.
    pub record: RecordKind,
    /// Always `1` for this build.
    pub schema_version: SchemaVersion,
    /// The run.
    pub run_id: RunId,
    /// The node.
    pub node: NodeRef,
    /// Attempt number within the run; every started attempt counts.
    pub number: AttemptNumber,
    /// Controller or worker identity that owns the attempt.
    pub owner: String,
    /// When the attempt was claimed.
    pub started_at: Timestamp,
    /// When execution ended; present exactly when `execution` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<Timestamp>,
    /// Narrower guidance for this attempt. It never edits the acceptance
    /// contract, which lives on the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_hint: Option<String>,
    /// Evidence frozen at dispatch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_manifest: Vec<EvidenceRef>,
    /// Accepted upstream receipts pinned at dispatch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependency_receipts: Vec<ReceiptRef>,
    /// The candidate produced, if execution got that far.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<ResultRef>,
    /// Evidence produced during the attempt, sealed before checking.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub produced_evidence: Vec<EvidenceRef>,
    /// How execution ended; present exactly when `finished_at` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<Execution>,
    /// The verifier receipt issued for this attempt, once one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier_receipt: Option<ReceiptRef>,
}

impl Record for Attempt {
    const KIND: RecordKind = RecordKind::Attempt;
    const VERSION: SchemaVersion = SchemaVersion(1);
    const SCHEMA_ID: &'static str = "https://checkspan.invalid/schemas/v1/attempt.schema.json";
}

/// A binding rule that a structurally valid attempt breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptError {
    /// `owner` is empty.
    EmptyOwner,
    /// `finished_at` is set but there is no `execution`.
    FinishedWithoutExecution,
    /// `execution` is set but there is no `finished_at`.
    ExecutionWithoutFinish,
    /// A `result` exists for an attempt that has not finished.
    ResultWithoutExecution,
    /// A receipt is referenced but the attempt produced no result.
    ReceiptWithoutResult,
    /// A receipt is referenced but execution did not complete.
    ReceiptRequiresCompletedExecution(Option<ExecutionOutcome>),
    /// The referenced receipt is about another node.
    ReceiptForOtherNode(ReceiptRef),
    /// The referenced receipt was issued in another run.
    ReceiptForOtherRun(ReceiptRef),
    /// The same dependency receipt is pinned twice.
    DuplicateDependencyReceipt(Ident),
    /// Two input manifest entries satisfy the same port.
    DuplicateInputPort(Ident),
}

impl fmt::Display for AttemptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttemptError::EmptyOwner => f.write_str("owner is empty"),
            AttemptError::FinishedWithoutExecution => {
                f.write_str("finished_at is set without an execution record")
            }
            AttemptError::ExecutionWithoutFinish => {
                f.write_str("execution is recorded without finished_at")
            }
            AttemptError::ResultWithoutExecution => {
                f.write_str("a result exists for an attempt that has not finished")
            }
            AttemptError::ReceiptWithoutResult => {
                f.write_str("a verifier receipt is referenced but there is no result")
            }
            AttemptError::ReceiptRequiresCompletedExecution(o) => write!(
                f,
                "a verifier receipt requires completed execution, found {}",
                o.map_or("none", ExecutionOutcome::as_str)
            ),
            AttemptError::ReceiptForOtherNode(r) => {
                write!(f, "verifier receipt {r} is about another node")
            }
            AttemptError::ReceiptForOtherRun(r) => {
                write!(f, "verifier receipt {r} was issued in another run")
            }
            AttemptError::DuplicateDependencyReceipt(id) => {
                write!(f, "dependency receipt {id:?} is pinned more than once")
            }
            AttemptError::DuplicateInputPort(p) => {
                write!(f, "input manifest satisfies port {p:?} more than once")
            }
        }
    }
}

impl std::error::Error for AttemptError {}

impl Attempt {
    /// Exact reference to this attempt.
    pub fn attempt_ref(&self) -> AttemptRef {
        AttemptRef {
            run_id: self.run_id.clone(),
            node: self.node.clone(),
            number: self.number,
        }
    }

    /// Check every binding rule that concerns this record alone.
    pub fn check_bindings(&self) -> Vec<AttemptError> {
        let mut errors = Vec::new();
        if self.owner.trim().is_empty() {
            errors.push(AttemptError::EmptyOwner);
        }
        match (&self.finished_at, &self.execution) {
            (Some(_), None) => errors.push(AttemptError::FinishedWithoutExecution),
            (None, Some(_)) => errors.push(AttemptError::ExecutionWithoutFinish),
            _ => {}
        }
        if self.result.is_some() && self.execution.is_none() {
            errors.push(AttemptError::ResultWithoutExecution);
        }
        if let Some(receipt) = &self.verifier_receipt {
            if self.result.is_none() {
                errors.push(AttemptError::ReceiptWithoutResult);
            }
            let outcome = self.execution.as_ref().map(|e| e.outcome);
            if outcome != Some(ExecutionOutcome::Completed) {
                errors.push(AttemptError::ReceiptRequiresCompletedExecution(outcome));
            }
            if receipt.node != self.node {
                errors.push(AttemptError::ReceiptForOtherNode(receipt.clone()));
            }
            if receipt.run_id != self.run_id {
                errors.push(AttemptError::ReceiptForOtherRun(receipt.clone()));
            }
        }
        let mut seen = HashSet::new();
        for dep in &self.dependency_receipts {
            if !seen.insert(&dep.id) {
                errors.push(AttemptError::DuplicateDependencyReceipt(dep.id.clone()));
            }
        }
        let mut ports = HashSet::new();
        for evidence in &self.input_manifest {
            if !ports.insert(&evidence.port_name) {
                errors.push(AttemptError::DuplicateInputPort(evidence.port_name.clone()));
            }
        }
        errors
    }
}

/// What the verifier concluded. None of these is an execution outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The exact result satisfies the pinned acceptance contract.
    Accept,
    /// The verifier completed and the result does not satisfy the contract.
    Reject,
    /// The verifier completed but could not decide; a human gate follows.
    Undecidable,
}

impl Verdict {
    /// Every verdict, for coverage checks.
    pub const ALL: [Verdict; 3] = [Verdict::Accept, Verdict::Reject, Verdict::Undecidable];

    /// The `verdict` field text.
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Accept => "accept",
            Verdict::Reject => "reject",
            Verdict::Undecidable => "undecidable",
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How strongly a record's issuer is authenticated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceLevel {
    /// Recorded by the trusted local controller from what it observed.
    LocalController,
    /// Authenticated by an upstream service such as hosted CI.
    AuthenticatedUpstream,
    /// Signed by a registered operator key outside the worker.
    SignedOperator,
}

impl ProvenanceLevel {
    /// Every level, for coverage checks.
    pub const ALL: [ProvenanceLevel; 3] = [
        ProvenanceLevel::LocalController,
        ProvenanceLevel::AuthenticatedUpstream,
        ProvenanceLevel::SignedOperator,
    ];
}

/// Who issued a receipt and how that is authenticated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptProvenance {
    /// Authentication level of the issuer binding.
    pub level: ProvenanceLevel,
    /// The issuer identity as observed.
    pub issuer: String,
    /// Reference to the attestation that binds the issuer to the receipt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation_ref: Option<String>,
}

/// How long and under what conditions a receipt stays admissible.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Validity {
    /// Moment after which the receipt cannot admit new work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<Timestamp>,
    /// Conditions that must still hold when the receipt is reused.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<String>,
}

/// A verifier's verdict about one exact attempt result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierReceipt {
    /// Always `verifier_receipt`.
    pub record: RecordKind,
    /// Always `1` for this build.
    pub schema_version: SchemaVersion,
    /// Receipt identity.
    pub id: Ident,
    /// The exact attempt checked.
    pub attempt: AttemptRef,
    /// What was checked: a commit, tree, patch, or artifact identity.
    pub subject: String,
    /// Digest of the exact result bytes checked.
    pub result_digest: Digest,
    /// Digest of the complete verification context: contract, inputs,
    /// dependency receipts, and produced evidence.
    pub context_digest: Digest,
    /// The verifier that issued the verdict.
    pub verifier: VerifierRef,
    /// The policy the verdict was issued under.
    pub policy_ref: PolicyRef,
    /// The verdict.
    pub verdict: Verdict,
    /// Machine-readable reason; required for `reject` and `undecidable`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<Ident>,
    /// Bounded human-readable reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_text: Option<String>,
    /// Narrower guidance for a retry; never on `accept`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_hint: Option<String>,
    /// When the verdict was issued.
    pub checked_at: Timestamp,
    /// How long the verdict stays admissible.
    #[serde(default)]
    pub validity: Validity,
    /// Who issued it and how that is authenticated.
    pub provenance: ReceiptProvenance,
}

impl Record for VerifierReceipt {
    const KIND: RecordKind = RecordKind::VerifierReceipt;
    const VERSION: SchemaVersion = SchemaVersion(1);
    const SCHEMA_ID: &'static str =
        "https://checkspan.invalid/schemas/v1/verifier-receipt.schema.json";
}

/// A binding rule that a structurally valid receipt breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiptError {
    /// `subject` is empty.
    EmptySubject,
    /// `provenance.issuer` is empty.
    EmptyIssuer,
    /// A `reject` or `undecidable` verdict has no `reason_code`.
    MissingReasonCode(Verdict),
    /// An `accept` verdict carries a `retry_hint`.
    RetryHintOnAccept,
    /// The receipt names a different attempt than the one offered.
    AttemptMismatch {
        /// The attempt offered for binding.
        expected: AttemptRef,
        /// The attempt the receipt names.
        found: AttemptRef,
    },
    /// The attempt has no result to check.
    AttemptHasNoResult,
    /// The attempt's result digest differs from the one checked.
    ResultDigestMismatch {
        /// The attempt's result digest.
        expected: Digest,
        /// The digest the receipt checked.
        found: Digest,
    },
    /// The attempt did not complete, so no verdict can apply to it.
    AttemptNotCompleted(Option<ExecutionOutcome>),
}

impl fmt::Display for ReceiptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReceiptError::EmptySubject => f.write_str("subject is empty"),
            ReceiptError::EmptyIssuer => f.write_str("provenance.issuer is empty"),
            ReceiptError::MissingReasonCode(v) => {
                write!(f, "a {v} verdict requires a reason_code")
            }
            ReceiptError::RetryHintOnAccept => {
                f.write_str("an accept verdict cannot carry a retry_hint")
            }
            ReceiptError::AttemptMismatch { expected, found } => {
                write!(f, "receipt is about attempt {found}, not {expected}")
            }
            ReceiptError::AttemptHasNoResult => f.write_str("the attempt produced no result"),
            ReceiptError::ResultDigestMismatch { expected, found } => write!(
                f,
                "receipt checked result {found} but the attempt produced {expected}"
            ),
            ReceiptError::AttemptNotCompleted(o) => write!(
                f,
                "a verdict requires completed execution, found {}",
                o.map_or("none", ExecutionOutcome::as_str)
            ),
        }
    }
}

impl std::error::Error for ReceiptError {}

impl VerifierReceipt {
    /// Reference to this receipt.
    pub fn receipt_ref(&self) -> ReceiptRef {
        ReceiptRef {
            id: self.id.clone(),
            run_id: self.attempt.run_id.clone(),
            node: self.attempt.node.clone(),
        }
    }

    /// Check every rule that concerns this record alone.
    pub fn check_bindings(&self) -> Vec<ReceiptError> {
        let mut errors = Vec::new();
        if self.subject.trim().is_empty() {
            errors.push(ReceiptError::EmptySubject);
        }
        if self.provenance.issuer.trim().is_empty() {
            errors.push(ReceiptError::EmptyIssuer);
        }
        match self.verdict {
            Verdict::Accept => {
                if self.retry_hint.is_some() {
                    errors.push(ReceiptError::RetryHintOnAccept);
                }
            }
            Verdict::Reject | Verdict::Undecidable => {
                if self.reason_code.is_none() {
                    errors.push(ReceiptError::MissingReasonCode(self.verdict));
                }
            }
        }
        errors
    }

    /// Check that this receipt is about exactly `attempt` and its exact
    /// result. An empty list means the receipt binds.
    pub fn binds(&self, attempt: &Attempt) -> Vec<ReceiptError> {
        let mut errors = Vec::new();
        let expected = attempt.attempt_ref();
        if self.attempt != expected {
            errors.push(ReceiptError::AttemptMismatch {
                expected,
                found: self.attempt.clone(),
            });
        }
        let outcome = attempt.execution.as_ref().map(|e| e.outcome);
        if outcome != Some(ExecutionOutcome::Completed) {
            errors.push(ReceiptError::AttemptNotCompleted(outcome));
        }
        match &attempt.result {
            None => errors.push(ReceiptError::AttemptHasNoResult),
            Some(result) if result.digest != self.result_digest => {
                errors.push(ReceiptError::ResultDigestMismatch {
                    expected: result.digest.clone(),
                    found: self.result_digest.clone(),
                });
            }
            Some(_) => {}
        }
        errors
    }
}
