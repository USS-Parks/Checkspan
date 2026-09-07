//! The patch-capture verifier: check that a typed patch artifact exists
//! against the pinned base and describes the candidate on disk.
//!
//! A protocol child like the software verifier. Its pinned digest is the
//! digest of the bundled `patch_result` schema text: this verifier has no
//! separate validation profile, because what it enforces is exactly that
//! schema plus the base and subject bindings, and revising the schema
//! revises the verifier identity.

use std::fs;
use std::io::Read;
use std::path::Path;

use crate::adapters::code;
use crate::contracts::{Exactly, Ident, PatchResult, Record, Timestamp, Verdict};
use crate::digests::{artifact_digest, patch_subject_digest};
use crate::evidence::code_selection;
use crate::validation::{self, ValidatedRecord};
use crate::verifier_host::{CheckOutcome, CheckReport, VerifierRequest, VerifierResponse};
use crate::verifiers::software::{EXIT_CONTRACT, EXIT_PROFILE, EXIT_REQUEST};

/// The verifier's identity.
pub const VERIFIER_ID: &str = "patch_capture";
/// The verifier's version.
pub const VERIFIER_VERSION: u32 = 1;

/// The digest a contract must pin for this verifier: the bundled
/// `patch_result` schema text.
pub fn pinned_digest() -> crate::contracts::Digest {
    let schema = crate::contracts::registry::bundled(PatchResult::SCHEMA_ID)
        .expect("patch result schema is bundled");
    artifact_digest(schema.as_bytes())
}

/// Run the patch-capture verifier as a protocol child: request on standard
/// input, response on standard output, checking the artifact at `artifact`.
/// Returns the process exit code.
pub fn run_child(artifact: &Path) -> i32 {
    let mut text = String::new();
    if std::io::stdin().read_to_string(&mut text).is_err() {
        eprintln!("cannot read the request");
        return EXIT_REQUEST;
    }
    let request: VerifierRequest = match serde_json::from_str(&text) {
        Ok(request) => request,
        Err(e) => {
            eprintln!("standard input is not a request: {e}");
            return EXIT_REQUEST;
        }
    };
    if request.verifier.id.as_str() != VERIFIER_ID
        || request.verifier.version.get() != VERIFIER_VERSION
        || request.verifier.digest != pinned_digest()
    {
        eprintln!(
            "the contract pins {}@{} {}; this verifier is {VERIFIER_ID}@{VERIFIER_VERSION} {}",
            request.verifier.id,
            request.verifier.version,
            request.verifier.digest,
            pinned_digest()
        );
        return EXIT_PROFILE;
    }
    let mut candidates = request
        .evidence
        .iter()
        .filter_map(|reference| code_selection(&reference.locator).map(|c| (reference, c)));
    let Some((_, (root, frozen))) = candidates.next() else {
        eprintln!("no code evidence names a candidate");
        return EXIT_CONTRACT;
    };
    if candidates.next().is_some() {
        eprintln!("more than one code evidence entry names a candidate");
        return EXIT_CONTRACT;
    }
    for id in &request.required_checks {
        if !matches!(id.as_str(), "artifact_exists" | "base_matches") {
            eprintln!("required check {id} is not one this verifier implements");
            return EXIT_CONTRACT;
        }
    }

    // The artifact: present, schema-valid, internally consistent.
    let mut checks = Vec::new();
    let record: Option<PatchResult> = match fs::read(artifact) {
        Ok(bytes) => {
            let report = validation::validate(&bytes, &validation::Limits::default());
            match report.record {
                Some(ValidatedRecord::PatchResult(record)) if report.is_valid() => {
                    checks.push(report_for("artifact_exists", true, None));
                    Some(record)
                }
                _ => {
                    let detail = report
                        .diagnostics
                        .first()
                        .map(|d| d.to_string())
                        .unwrap_or_else(|| "not a patch_result document".to_owned());
                    checks.push(report_for("artifact_exists", false, Some(detail)));
                    None
                }
            }
        }
        Err(e) => {
            checks.push(report_for(
                "artifact_exists",
                false,
                Some(format!("cannot read {}: {e}", artifact.display())),
            ));
            None
        }
    };

    // The candidate on disk, against the frozen base, is what the request
    // and the artifact both describe.
    if request
        .required_checks
        .iter()
        .any(|c| c.as_str() == "base_matches")
    {
        let selection = code::Selection {
            candidate: code::Selector::WorkingTree,
            base: frozen.base.clone(),
        };
        let capture_time = Timestamp::new("2026-01-01T00:00:00Z").expect("a fixed instant parses");
        let outcome = match code::capture(
            &root,
            &selection,
            &code::Limits::default(),
            &capture_time,
        ) {
            Ok(captured) => {
                let live = format!("patch_subject:{}", captured.subject_digest);
                if live != request.subject {
                    (
                        false,
                        Some(format!(
                            "the checkout is {live}; the request froze {}",
                            request.subject
                        )),
                    )
                } else {
                    match &record {
                        Some(record) => match patch_subject_digest(record) {
                            Ok(digest) if captured.subject_digest == digest => (true, None),
                            Ok(digest) => (
                                false,
                                Some(format!(
                                    "the artifact describes patch_subject:{digest}, not the checkout"
                                )),
                            ),
                            Err(e) => (false, Some(e.to_string())),
                        },
                        None => (false, Some("no readable artifact to compare".to_owned())),
                    }
                }
            }
            Err(e) => (false, Some(format!("cannot capture the candidate: {e}"))),
        };
        checks.push(report_for("base_matches", outcome.0, outcome.1));
    }
    let ran: Vec<CheckReport> = checks
        .into_iter()
        .filter(|c| request.required_checks.contains(&c.id))
        .collect();
    let all_passed = !ran.is_empty() && ran.iter().all(|c| c.outcome == CheckOutcome::Passed);
    let failed: Vec<String> = ran
        .iter()
        .filter(|c| c.outcome != CheckOutcome::Passed)
        .map(|c| c.id.as_str().to_owned())
        .collect();
    let response = VerifierResponse {
        protocol: Exactly,
        verifier: request.verifier.clone(),
        attempt: request.attempt.clone(),
        verdict: if all_passed {
            Verdict::Accept
        } else {
            Verdict::Reject
        },
        checks: ran,
        reason_code: (!failed.is_empty()).then(|| Ident::new("check_failed").expect("an ident")),
        reason_text: (!failed.is_empty()).then(|| format!("not passed: {}", failed.join(", "))),
    };
    print!(
        "{}",
        serde_json::to_string(&response).expect("a response serializes")
    );
    0
}

fn report_for(id: &str, passed: bool, detail: Option<String>) -> CheckReport {
    CheckReport {
        id: Ident::new(id).expect("an ident"),
        outcome: if passed {
            CheckOutcome::Passed
        } else {
            CheckOutcome::Failed
        },
        detail,
    }
}
