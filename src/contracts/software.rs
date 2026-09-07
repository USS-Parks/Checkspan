//! The typed result of running the required software checks on one exact
//! candidate.
//!
//! A `software_check_result` binds the tested candidate (by subject digest
//! and commit), the pinned validation profile that defined the checks, the
//! environment, and every required check's actual exit, outcome, duration,
//! and output digests. Its conclusion restates what the checks showed; the
//! record cannot conclude `accept` while any listed check is short of
//! passed, and a required check that did not run never passes by omission.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::attempt::AttemptRef;
use super::ids::{Digest, Ident};
use super::node::VerifierRef;
use super::patch::{Candidate, CommitId, Repository};
use super::record::{Record, RecordKind, SchemaVersion};

/// Largest integer an envelope digest can carry exactly.
const MAX_EXACT_INTEGER: u64 = (1 << 53) - 1;

/// How one check actually ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoftwareCheckOutcome {
    /// The check process exited successfully.
    Passed,
    /// The check process exited unsuccessfully.
    Failed,
    /// The check exceeded its ceiling and was killed. Not a failure of the
    /// candidate's content, but never a pass.
    TimedOut,
}

impl SoftwareCheckOutcome {
    /// Every outcome, for coverage checks.
    pub const ALL: [SoftwareCheckOutcome; 3] = [
        SoftwareCheckOutcome::Passed,
        SoftwareCheckOutcome::Failed,
        SoftwareCheckOutcome::TimedOut,
    ];

    /// The wire text.
    pub fn as_str(self) -> &'static str {
        match self {
            SoftwareCheckOutcome::Passed => "passed",
            SoftwareCheckOutcome::Failed => "failed",
            SoftwareCheckOutcome::TimedOut => "timed_out",
        }
    }
}

/// What the checks collectively established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Conclusion {
    /// Every required check ran and passed.
    Accept,
    /// At least one check failed or timed out.
    Reject,
}

impl Conclusion {
    /// Every conclusion, for coverage checks.
    pub const ALL: [Conclusion; 2] = [Conclusion::Accept, Conclusion::Reject];

    /// The wire text.
    pub fn as_str(self) -> &'static str {
        match self {
            Conclusion::Accept => "accept",
            Conclusion::Reject => "reject",
        }
    }
}

/// One check as it actually ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckExecution {
    /// The check.
    pub id: Ident,
    /// How it ended.
    pub outcome: SoftwareCheckOutcome,
    /// The process exit code; absent exactly when the check timed out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Digest of the check's bounded standard output.
    pub stdout_digest: Digest,
    /// Digest of the check's bounded standard error.
    pub stderr_digest: Digest,
    /// Bounded human-readable detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Where the checks ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentInfo {
    /// Operating system, as the standard library names it.
    pub os: String,
    /// Processor architecture, as the standard library names it.
    pub arch: String,
}

/// The typed result of the required software checks on one exact candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoftwareCheckResult {
    /// Always `software_check_result`.
    pub record: RecordKind,
    /// Always `1` for this build.
    pub schema_version: SchemaVersion,
    /// The attempt the checks ran for.
    pub attempt: AttemptRef,
    /// The tested candidate's subject, `patch_subject:` and its digest.
    pub subject: String,
    /// The tested candidate.
    pub candidate: Candidate,
    /// The base the candidate was captured against.
    pub base_commit: CommitId,
    /// The repository, by its root commits.
    pub repository: Repository,
    /// The pinned validation profile that defined the checks, with the
    /// digest of its exact bytes.
    pub profile: VerifierRef,
    /// Where the checks ran.
    pub environment: EnvironmentInfo,
    /// The checks the contract required, in order.
    pub required_checks: Vec<Ident>,
    /// Every check that ran, in the order it ran.
    pub checks: Vec<CheckExecution>,
    /// What the checks collectively established.
    pub conclusion: Conclusion,
}

impl Record for SoftwareCheckResult {
    const KIND: RecordKind = RecordKind::SoftwareCheckResult;
    const VERSION: SchemaVersion = SchemaVersion(1);
    const SCHEMA_ID: &'static str =
        "https://checkspan.invalid/schemas/v1/software-check-result.schema.json";
}

/// Why a software check result is not internally consistent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoftwareError {
    /// The subject is empty.
    EmptySubject,
    /// No check ran.
    NoChecks,
    /// A check id appears twice.
    DuplicateCheck(String),
    /// No check is required.
    NoRequiredChecks,
    /// A required check id appears twice.
    DuplicateRequired(String),
    /// The conclusion is `accept` but a required check is absent.
    AcceptWithoutCheck(String),
    /// The conclusion is `accept` but a listed check did not pass.
    AcceptWithFailure(String),
    /// The conclusion is `reject` but every listed check passed.
    RejectWithoutFailure,
    /// A timed-out check carries an exit code.
    TimedOutWithExit(String),
    /// A completed check lacks its exit code.
    CompletedWithoutExit(String),
    /// A duration exceeds what an envelope digest can carry exactly.
    DurationNotExact(String),
}

impl fmt::Display for SoftwareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SoftwareError::EmptySubject => f.write_str("subject is empty"),
            SoftwareError::NoChecks => f.write_str("no check ran"),
            SoftwareError::DuplicateCheck(id) => write!(f, "check {id:?} appears twice"),
            SoftwareError::NoRequiredChecks => f.write_str("no check is required"),
            SoftwareError::DuplicateRequired(id) => {
                write!(f, "required check {id:?} appears twice")
            }
            SoftwareError::AcceptWithoutCheck(id) => {
                write!(
                    f,
                    "conclusion is accept but required check {id:?} did not run"
                )
            }
            SoftwareError::AcceptWithFailure(id) => {
                write!(f, "conclusion is accept but check {id:?} is not passed")
            }
            SoftwareError::RejectWithoutFailure => {
                f.write_str("conclusion is reject but every check passed")
            }
            SoftwareError::TimedOutWithExit(id) => {
                write!(f, "timed-out check {id:?} carries an exit code")
            }
            SoftwareError::CompletedWithoutExit(id) => {
                write!(f, "completed check {id:?} lacks its exit code")
            }
            SoftwareError::DurationNotExact(id) => {
                write!(f, "duration of check {id:?} exceeds 2^53 - 1")
            }
        }
    }
}

impl std::error::Error for SoftwareError {}

impl SoftwareCheckResult {
    /// Every internal inconsistency, in document order.
    pub fn check_bindings(&self) -> Vec<SoftwareError> {
        let mut errors = Vec::new();
        if self.subject.trim().is_empty() {
            errors.push(SoftwareError::EmptySubject);
        }
        if self.required_checks.is_empty() {
            errors.push(SoftwareError::NoRequiredChecks);
        }
        let mut seen = std::collections::BTreeSet::new();
        for id in &self.required_checks {
            if !seen.insert(id.as_str()) {
                errors.push(SoftwareError::DuplicateRequired(id.as_str().to_owned()));
            }
        }
        if self.checks.is_empty() {
            errors.push(SoftwareError::NoChecks);
        }
        let mut ran = std::collections::BTreeSet::new();
        for check in &self.checks {
            if !ran.insert(check.id.as_str()) {
                errors.push(SoftwareError::DuplicateCheck(check.id.as_str().to_owned()));
            }
            match (check.outcome, check.exit_code) {
                (SoftwareCheckOutcome::TimedOut, Some(_)) => {
                    errors.push(SoftwareError::TimedOutWithExit(
                        check.id.as_str().to_owned(),
                    ));
                }
                (SoftwareCheckOutcome::Passed | SoftwareCheckOutcome::Failed, None) => {
                    errors.push(SoftwareError::CompletedWithoutExit(
                        check.id.as_str().to_owned(),
                    ));
                }
                _ => {}
            }
            if check.duration_ms > MAX_EXACT_INTEGER {
                errors.push(SoftwareError::DurationNotExact(
                    check.id.as_str().to_owned(),
                ));
            }
        }
        match self.conclusion {
            Conclusion::Accept => {
                for id in &self.required_checks {
                    if !ran.contains(id.as_str()) {
                        errors.push(SoftwareError::AcceptWithoutCheck(id.as_str().to_owned()));
                    }
                }
                for check in &self.checks {
                    if check.outcome != SoftwareCheckOutcome::Passed {
                        errors.push(SoftwareError::AcceptWithFailure(
                            check.id.as_str().to_owned(),
                        ));
                    }
                }
            }
            Conclusion::Reject => {
                if self
                    .checks
                    .iter()
                    .all(|c| c.outcome == SoftwareCheckOutcome::Passed)
                {
                    errors.push(SoftwareError::RejectWithoutFailure);
                }
            }
        }
        errors
    }
}
