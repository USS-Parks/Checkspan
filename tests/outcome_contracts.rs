//! Attempts, receipts, gate packets, gate decisions, and node views: records
//! cannot express acceptance without their receipt bindings, an authentic
//! denial remains a denial, a timeout cannot parse as a verdict, and every
//! variant has a golden fixture.

mod common;

use std::collections::HashSet;

use checkspan::contracts::{
    ActionKind, ActionRequest, AssessmentOutcome, Attempt, AttemptError, CandidateKind, ChangeKind,
    Conclusion, ContractError, DecisionKind, ExecutionOutcome, GateDecision, GateError, GatePacket,
    GatePurpose, NodeStatus, NodeView, PatchResult, ProvenanceLevel, ReceiptError, Record,
    RecordKind, SoftwareCheckOutcome, SoftwareCheckResult, Verdict, VerifierReceipt, ViewError,
    parse_record,
};
use common::{fixture, fixture_names, schema_errors};
use serde_json::Value;

fn valid(name: &str) -> String {
    fixture(&format!("outcomes/valid/{name}"))
}

fn invalid(name: &str) -> String {
    fixture(&format!("outcomes/invalid/{name}"))
}

fn record_kind(text: &str) -> &'static str {
    let value: Value = serde_json::from_str(text).unwrap();
    match value["record"].as_str() {
        Some("attempt") => "attempt",
        Some("verifier_receipt") => "verifier_receipt",
        Some("gate_packet") => "gate_packet",
        Some("gate_decision") => "gate_decision",
        Some("node_view") => "node_view",
        Some("patch_result") => "patch_result",
        Some("software_check_result") => "software_check_result",
        other => panic!("unknown record kind {other:?}"),
    }
}

fn schema_for(text: &str) -> &'static str {
    match record_kind(text) {
        "attempt" => Attempt::SCHEMA_ID,
        "verifier_receipt" => VerifierReceipt::SCHEMA_ID,
        "gate_packet" => GatePacket::SCHEMA_ID,
        "gate_decision" => GateDecision::SCHEMA_ID,
        "patch_result" => PatchResult::SCHEMA_ID,
        "software_check_result" => SoftwareCheckResult::SCHEMA_ID,
        _ => NodeView::SCHEMA_ID,
    }
}

/// Parse, check the record's own binding rules, and round-trip; returns the
/// binding errors as strings so every record kind shares one path.
fn parse_and_check(text: &str) -> Result<Vec<String>, ContractError> {
    Ok(match record_kind(text) {
        "attempt" => {
            let r: Attempt = parse_record(text)?;
            let again: Attempt = parse_record(&serde_json::to_string(&r).unwrap()).unwrap();
            assert_eq!(r, again);
            r.check_bindings().iter().map(ToString::to_string).collect()
        }
        "verifier_receipt" => {
            let r: VerifierReceipt = parse_record(text)?;
            let again: VerifierReceipt = parse_record(&serde_json::to_string(&r).unwrap()).unwrap();
            assert_eq!(r, again);
            r.check_bindings().iter().map(ToString::to_string).collect()
        }
        "gate_packet" => {
            let r: GatePacket = parse_record(text)?;
            let again: GatePacket = parse_record(&serde_json::to_string(&r).unwrap()).unwrap();
            assert_eq!(r, again);
            r.check_bindings().iter().map(ToString::to_string).collect()
        }
        "gate_decision" => {
            let r: GateDecision = parse_record(text)?;
            let again: GateDecision = parse_record(&serde_json::to_string(&r).unwrap()).unwrap();
            assert_eq!(r, again);
            r.check_bindings().iter().map(ToString::to_string).collect()
        }
        "software_check_result" => {
            let r: SoftwareCheckResult = parse_record(text)?;
            let again: SoftwareCheckResult =
                parse_record(&serde_json::to_string(&r).unwrap()).unwrap();
            assert_eq!(r, again);
            r.check_bindings().iter().map(ToString::to_string).collect()
        }
        "patch_result" => {
            let r: PatchResult = parse_record(text)?;
            let again: PatchResult = parse_record(&serde_json::to_string(&r).unwrap()).unwrap();
            assert_eq!(r, again);
            r.check_bindings().iter().map(ToString::to_string).collect()
        }
        _ => {
            let r: NodeView = parse_record(text)?;
            let again: NodeView = parse_record(&serde_json::to_string(&r).unwrap()).unwrap();
            assert_eq!(r, again);
            r.check_bindings().iter().map(ToString::to_string).collect()
        }
    })
}

/// Schema rejects and the typed path fails to parse with a message naming
/// `needle`.
fn assert_shape_rejects(name: &str, needle: &str) {
    let text = invalid(name);
    assert!(
        !schema_errors(schema_for(&text), &text).is_empty(),
        "{name}: schema must reject"
    );
    let err = parse_and_check(&text).expect_err(name).to_string();
    assert!(err.contains(needle), "{name}: {err}");
}

/// Schema rejects, typed parse succeeds, and the binding check names the
/// rule.
fn assert_binding_rejects(name: &str, needle: &str) {
    let text = invalid(name);
    assert!(
        !schema_errors(schema_for(&text), &text).is_empty(),
        "{name}: schema must reject"
    );
    let errors = parse_and_check(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert!(
        errors.iter().any(|e| e.contains(needle)),
        "{name}: expected {needle:?} in {errors:?}"
    );
}

/// Schema accepts (the rule is cross-field beyond JSON Schema), typed parse
/// succeeds, and the binding check names the rule.
fn assert_binding_only_rejects(name: &str, needle: &str) {
    let text = invalid(name);
    assert_eq!(
        schema_errors(schema_for(&text), &text),
        Vec::<String>::new(),
        "{name} is schema-valid by construction"
    );
    let errors = parse_and_check(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert!(
        errors.iter().any(|e| e.contains(needle)),
        "{name}: expected {needle:?} in {errors:?}"
    );
}

#[test]
fn golden_fixtures_cover_every_variant() {
    let mut outcomes = HashSet::new();
    let mut verdicts = HashSet::new();
    let mut levels = HashSet::new();
    let mut purposes = HashSet::new();
    let mut decisions = HashSet::new();
    let mut assessments = HashSet::new();
    let mut statuses = HashSet::new();
    let mut candidates = HashSet::new();
    let mut changes = HashSet::new();
    let mut conclusions = HashSet::new();
    let mut software_outcomes = HashSet::new();
    let names = fixture_names("outcomes/valid");
    for name in &names {
        let text = valid(name);
        assert_eq!(
            schema_errors(schema_for(&text), &text),
            Vec::<String>::new(),
            "{name}"
        );
        let errors = parse_and_check(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(errors, Vec::<String>::new(), "{name}");
        match record_kind(&text) {
            "attempt" => {
                let a: Attempt = parse_record(&text).unwrap();
                if let Some(e) = a.execution {
                    outcomes.insert(e.outcome);
                }
            }
            "verifier_receipt" => {
                let r: VerifierReceipt = parse_record(&text).unwrap();
                verdicts.insert(r.verdict);
                levels.insert(r.provenance.level);
            }
            "gate_packet" => {
                let p: GatePacket = parse_record(&text).unwrap();
                purposes.insert(p.purpose);
            }
            "gate_decision" => {
                let d: GateDecision = parse_record(&text).unwrap();
                decisions.insert(d.decision);
                levels.insert(d.authority.level);
                if let Some(a) = d.assessment {
                    assessments.insert(a.outcome);
                }
            }
            "patch_result" => {
                let p: PatchResult = parse_record(&text).unwrap();
                candidates.insert(p.candidate.kind);
                changes.extend(p.changes.iter().map(|c| c.change));
            }
            "software_check_result" => {
                let r: SoftwareCheckResult = parse_record(&text).unwrap();
                conclusions.insert(r.conclusion);
                software_outcomes.extend(r.checks.iter().map(|c| c.outcome));
            }
            _ => {
                let v: NodeView = parse_record(&text).unwrap();
                statuses.insert(v.status);
            }
        }
    }
    assert_eq!(
        candidates,
        [CandidateKind::Commit, CandidateKind::WorkingTree]
            .into_iter()
            .collect()
    );
    assert_eq!(
        changes,
        [
            ChangeKind::Added,
            ChangeKind::Modified,
            ChangeKind::Deleted,
            ChangeKind::Untracked,
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(conclusions, Conclusion::ALL.into_iter().collect());
    assert_eq!(
        software_outcomes,
        SoftwareCheckOutcome::ALL.into_iter().collect()
    );
    assert_eq!(outcomes, ExecutionOutcome::ALL.into_iter().collect());
    assert_eq!(verdicts, Verdict::ALL.into_iter().collect());
    assert_eq!(levels, ProvenanceLevel::ALL.into_iter().collect());
    assert_eq!(purposes, GatePurpose::ALL.into_iter().collect());
    assert_eq!(decisions, DecisionKind::ALL.into_iter().collect());
    assert_eq!(assessments, AssessmentOutcome::ALL.into_iter().collect());
    assert_eq!(statuses, NodeStatus::ALL.into_iter().collect());
    assert_eq!(names.len(), 32);
}

#[test]
fn execution_outcomes_and_verdicts_are_disjoint_vocabularies() {
    let outcomes: HashSet<&str> = ExecutionOutcome::ALL.iter().map(|o| o.as_str()).collect();
    let verdicts: HashSet<&str> = Verdict::ALL.iter().map(|v| v.as_str()).collect();
    assert!(outcomes.is_disjoint(&verdicts));
    for (name, variant) in [
        ("receipt-verdict-timed-out.json", "timed_out"),
        ("receipt-verdict-timeout.json", "timeout"),
        ("receipt-verdict-failed.json", "failed"),
        ("receipt-verdict-completed.json", "completed"),
    ] {
        assert_shape_rejects(name, variant);
    }
    assert_shape_rejects("attempt-outcome-is-verdict.json", "reject");
    assert_shape_rejects("attempt-outcome-accept.json", "accept");
    assert_shape_rejects("receipt-with-outcome.json", "outcome");
    assert_shape_rejects("attempt-with-verdict.json", "verdict");
    assert_shape_rejects("packet-with-verdict.json", "verdict");
    assert_shape_rejects("decision-with-verdict.json", "verdict");
    assert_shape_rejects("view-with-verdict.json", "verdict");
}

#[test]
fn failure_and_rejection_are_distinct_records() {
    let failed: Attempt = parse_record(&valid("attempt-failed.json")).unwrap();
    assert_eq!(
        failed.execution.as_ref().unwrap().outcome,
        ExecutionOutcome::Failed
    );
    assert!(failed.result.is_none());
    assert!(failed.verifier_receipt.is_none());
    assert_eq!(
        failed
            .execution
            .as_ref()
            .unwrap()
            .error_code
            .as_ref()
            .unwrap()
            .as_str(),
        "verifier_unavailable"
    );

    let rejected: VerifierReceipt = parse_record(&valid("receipt-reject.json")).unwrap();
    assert_eq!(rejected.verdict, Verdict::Reject);
    assert_eq!(
        rejected.reason_code.as_ref().unwrap().as_str(),
        "check_failed"
    );
    assert!(rejected.retry_hint.is_some());
    assert_eq!(
        rejected.provenance.level,
        ProvenanceLevel::AuthenticatedUpstream
    );

    let completed: Attempt = parse_record(&valid("attempt-completed-with-receipt.json")).unwrap();
    assert!(rejected.binds(&completed).is_empty());
    assert!(matches!(
        rejected.binds(&failed).as_slice(),
        [
            ReceiptError::AttemptNotCompleted(Some(ExecutionOutcome::Failed)),
            ReceiptError::AttemptHasNoResult
        ]
    ));
}

#[test]
fn acceptance_requires_its_receipt_bindings() {
    assert_binding_rejects("attempt-receipt-without-result.json", "no result");
    assert_binding_rejects("attempt-receipt-on-failed.json", "completed execution");
    assert_binding_rejects("attempt-receipt-on-timed-out.json", "completed execution");
    assert_binding_only_rejects("attempt-receipt-other-node.json", "another node");
    assert_binding_only_rejects("attempt-receipt-other-run.json", "another run");
    assert_binding_rejects("attempt-finished-without-execution.json", "finished_at");
    assert_binding_rejects("attempt-execution-without-finish.json", "finished_at");
    assert_binding_rejects("attempt-result-without-execution.json", "not finished");

    assert_binding_rejects("view-accepted-without-receipt.json", "requires the receipt");
    assert_binding_rejects("view-rejected-without-receipt.json", "requires the receipt");
    assert_binding_rejects("view-accepted-without-attempt.json", "current attempt");
    assert_binding_only_rejects("view-accepted-receipt-other-node.json", "another node");
    assert_binding_only_rejects("view-accepted-receipt-other-run.json", "another run");
    assert_binding_rejects("view-open-with-receipt.json", "cannot carry a receipt");
    assert_binding_rejects("view-running-with-receipt.json", "cannot carry a receipt");

    let accepted: NodeView = parse_record(&valid("view-accepted.json")).unwrap();
    assert_eq!(accepted.status, NodeStatus::Accepted);
    let receipt: VerifierReceipt = parse_record(&valid("receipt-accept.json")).unwrap();
    assert_eq!(accepted.receipt.unwrap(), receipt.receipt_ref());
    let attempt: Attempt = parse_record(&valid("attempt-completed-with-receipt.json")).unwrap();
    assert_eq!(receipt.binds(&attempt), vec![]);
    assert_eq!(attempt.verifier_receipt.unwrap(), receipt.receipt_ref());
}

#[test]
fn a_receipt_binds_only_its_exact_attempt_and_result() {
    let receipt: VerifierReceipt = parse_record(&valid("receipt-accept.json")).unwrap();
    let attempt: Attempt = parse_record(&valid("attempt-completed-with-receipt.json")).unwrap();
    assert!(receipt.binds(&attempt).is_empty());

    let mut other_attempt = attempt.clone();
    other_attempt.number = checkspan::contracts::AttemptNumber::new(2).unwrap();
    assert!(matches!(
        receipt.binds(&other_attempt).as_slice(),
        [ReceiptError::AttemptMismatch { .. }]
    ));

    let mut changed_result = attempt.clone();
    changed_result.result.as_mut().unwrap().digest =
        checkspan::contracts::Digest::new(format!("sha256:{}", "ab".repeat(32))).unwrap();
    assert!(matches!(
        receipt.binds(&changed_result).as_slice(),
        [ReceiptError::ResultDigestMismatch { .. }]
    ));

    let unchecked: Attempt = parse_record(&valid("attempt-completed-unchecked.json")).unwrap();
    assert!(
        receipt.binds(&unchecked).is_empty(),
        "same result, receipt not yet recorded"
    );
    let cancelled: Attempt = parse_record(&valid("attempt-cancelled.json")).unwrap();
    assert!(!receipt.binds(&cancelled).is_empty());
}

#[test]
fn reject_and_undecidable_need_a_reason_and_accept_takes_no_retry_hint() {
    assert_binding_rejects("receipt-reject-without-reason.json", "reason_code");
    assert_binding_rejects("receipt-undecidable-without-reason.json", "reason_code");
    assert_binding_rejects("receipt-accept-with-retry-hint.json", "retry_hint");
    assert_shape_rejects("receipt-missing-provenance.json", "provenance");
    assert_shape_rejects("receipt-missing-context-digest.json", "context_digest");
    assert_shape_rejects("receipt-missing-result-digest.json", "result_digest");
    assert_shape_rejects("receipt-unknown-provenance-level.json", "trusted");
    assert_shape_rejects("receipt-attempt-number-zero.json", "attempt number");
}

#[test]
fn attempt_shape_rules() {
    assert_shape_rejects("attempt-number-zero.json", "attempt number");
    assert_shape_rejects("attempt-with-acceptance.json", "accepted");
    assert_binding_only_rejects(
        "attempt-duplicate-dependency-receipt.json",
        "more than once",
    );
    assert_binding_only_rejects("attempt-duplicate-input-port.json", "more than once");
    assert_binding_rejects("attempt-empty-owner.json", "owner");
    let retry: Attempt = parse_record(&valid("attempt-retry-with-hint.json")).unwrap();
    assert_eq!(retry.number.get(), 2);
    assert!(retry.repair_hint.is_some());
    let json = serde_json::to_value(&retry).unwrap();
    assert!(
        json.get("acceptance").is_none(),
        "an attempt never carries the contract"
    );
}

#[test]
fn gate_packets_offer_only_what_their_purpose_allows() {
    assert_binding_rejects(
        "packet-option-not-for-purpose.json",
        "cannot offer approve_action",
    );
    assert_binding_rejects("packet-assess-with-retry.json", "cannot offer retry");
    assert_binding_rejects(
        "packet-authorize-without-action.json",
        "must name the exact action",
    );
    assert_binding_rejects("packet-resolve-with-action.json", "cannot name an action");
    assert_binding_rejects("packet-no-options.json", "no decision");
    assert_binding_only_rejects("packet-duplicate-option.json", "more than once");
    assert_binding_rejects("packet-empty-context.json", "no sealed context");
    assert_binding_only_rejects("packet-duplicate-context.json", "more than once");
    assert_shape_rejects("packet-unknown-purpose.json", "approve_work");
    assert_shape_rejects("packet-unknown-action.json", "deploy");
    assert_shape_rejects("packet-missing-expiry.json", "expires_at");
    for purpose in GatePurpose::ALL {
        assert!(!purpose.allowed_decisions().is_empty());
    }
}

#[test]
fn an_authentic_denial_remains_a_denial() {
    let packet: GatePacket = parse_record(&valid("packet-authorize-action.json")).unwrap();
    let deny: GateDecision = parse_record(&valid("decision-deny-action.json")).unwrap();
    assert_eq!(deny.check_bindings(), vec![]);
    assert_eq!(
        deny.binds(&packet),
        vec![],
        "a denial is a well-formed, bound decision"
    );
    assert!(deny.is_denial());
    let merge = ActionRequest {
        action: ActionKind::Merge,
        target: "refs/heads/main".into(),
    };
    assert!(!deny.authorizes(&merge));
    assert_eq!(deny.authority.level, ProvenanceLevel::SignedOperator);

    let approve: GateDecision = parse_record(&valid("decision-approve-action.json")).unwrap();
    assert_eq!(approve.binds(&packet), vec![]);
    assert!(approve.authorizes(&merge));
    assert!(!approve.authorizes(&ActionRequest {
        action: ActionKind::Merge,
        target: "refs/heads/release".into(),
    }));
    assert!(!approve.authorizes(&ActionRequest {
        action: ActionKind::Delete,
        target: "refs/heads/main".into(),
    }));

    let cancel: GateDecision = parse_record(&valid("decision-cancel.json")).unwrap();
    assert!(cancel.is_denial());
    assert!(!cancel.authorizes(&merge));
}

#[test]
fn a_decision_binds_only_the_exact_packet_it_answered() {
    let packet: GatePacket = parse_record(&valid("packet-authorize-action.json")).unwrap();
    let approve: GateDecision = parse_record(&valid("decision-approve-action.json")).unwrap();
    assert_eq!(approve.binds(&packet), vec![]);

    let mut changed = packet.clone();
    changed.subject_digest =
        checkspan::contracts::Digest::new(format!("sha256:{}", "cd".repeat(32))).unwrap();
    assert!(matches!(
        approve.binds(&changed).as_slice(),
        [GateError::PacketDigestMismatch { .. }]
    ));

    let mut retargeted = packet.clone();
    retargeted.requested_decision.action = Some(ActionRequest {
        action: ActionKind::Merge,
        target: "refs/heads/release".into(),
    });
    assert!(
        approve
            .binds(&retargeted)
            .contains(&GateError::ScopeActionMismatch)
    );

    let resolve: GatePacket = parse_record(&valid("packet-resolve-work.json")).unwrap();
    let errors = approve.binds(&resolve);
    assert!(errors.contains(&GateError::DecisionNotOffered(DecisionKind::ApproveAction)));
    assert!(errors.contains(&GateError::ScopeNodeMismatch));
    assert!(matches!(
        errors.first(),
        Some(GateError::PacketIdMismatch { .. })
    ));

    let retry: GateDecision = parse_record(&valid("decision-retry.json")).unwrap();
    assert_eq!(retry.binds(&resolve), vec![]);
    assert!(!retry.is_denial());
    assert!(!retry.authorizes(&ActionRequest {
        action: ActionKind::Merge,
        target: "refs/heads/main".into(),
    }));
}

#[test]
fn decision_payloads_are_typed() {
    assert_binding_rejects(
        "decision-assessment-without-payload.json",
        "assessment payload",
    );
    assert_binding_rejects(
        "decision-retry-with-assessment.json",
        "cannot carry an assessment",
    );
    assert_binding_rejects("decision-assessment-no-claims.json", "no claims");
    assert_binding_rejects(
        "decision-approve-without-action-scope.json",
        "exact action in scope",
    );
    assert_binding_rejects(
        "decision-retry-with-action-scope.json",
        "cannot name an action",
    );
    assert_binding_rejects("decision-empty-identity.json", "identity is empty");
    assert_shape_rejects("decision-unknown-kind.json", "approve");
    assert_shape_rejects("decision-unknown-level.json", "trusted");
    assert_shape_rejects("decision-assessment-unknown-outcome.json", "pass");
    assert_shape_rejects("decision-missing-packet-digest.json", "gate_packet_digest");

    let assess: GatePacket = parse_record(&valid("packet-assess-claims.json")).unwrap();
    for name in [
        "decision-assessment-adequate.json",
        "decision-assessment-inadequate.json",
        "decision-assessment-cannot-determine.json",
    ] {
        let d: GateDecision = parse_record(&valid(name)).unwrap();
        assert_eq!(d.binds(&assess), vec![], "{name}");
        assert_eq!(d.decision, DecisionKind::RecordAssessment);
        assert_eq!(d.assessment.as_ref().unwrap().reviewed_claim_ids.len(), 2);
    }
}

#[test]
fn approval_cannot_manufacture_a_machine_pass() {
    // A decision record is not a receipt: it cannot be read as one.
    let text = valid("decision-approve-action.json");
    assert!(matches!(
        parse_record::<VerifierReceipt>(&text),
        Err(ContractError::WrongRecord {
            expected: RecordKind::VerifierReceipt,
            ..
        })
    ));
    // An accepted view needs a receipt about this node in this run; a
    // receipt about the gate node cannot be borrowed.
    let mut view: NodeView = parse_record(&valid("view-accepted.json")).unwrap();
    let gate_receipt = checkspan::contracts::ReceiptRef {
        id: checkspan::contracts::Ident::new("rcpt-gate-1").unwrap(),
        run_id: view.run_id.clone(),
        node: checkspan::contracts::NodeRef {
            graph_id: view.node.graph_id.clone(),
            node_id: checkspan::contracts::NodeId::new("cs_gate").unwrap(),
            revision: checkspan::contracts::Revision::new(1).unwrap(),
        },
    };
    view.receipt = Some(gate_receipt.clone());
    assert_eq!(
        view.check_bindings(),
        vec![ViewError::ReceiptForOtherNode(gate_receipt)]
    );
    // And no status vocabulary includes an approval word.
    assert_shape_rejects("view-status-approved.json", "approved");
    assert_shape_rejects("view-unknown-status.json", "proved");
}

#[test]
fn node_view_status_rules() {
    assert_binding_rejects("view-gated-without-gate.json", "waiting_on_gate");
    assert_binding_rejects("view-running-with-gate.json", "cannot wait on a gate");
    assert_binding_rejects("view-running-without-attempt.json", "current attempt");
    assert_binding_rejects("view-failed-without-attempt.json", "current attempt");
    assert_binding_rejects("view-accepted-with-blocked-reason.json", "blocked_reason");
    let open: NodeView = parse_record(&valid("view-open.json")).unwrap();
    assert!(open.blocked_reason.is_some());
    assert!(open.current_attempt.is_none());
    let gated: NodeView = parse_record(&valid("view-gated.json")).unwrap();
    assert_eq!(gated.waiting_on_gate.unwrap().as_str(), "gate-0001");
    let _: Vec<AttemptError> = Vec::new();
}

#[test]
fn a_patch_result_deletion_cannot_carry_content() {
    assert_binding_only_rejects("patch-deleted-with-content.json", "carries content");
}

#[test]
fn a_software_result_cannot_conclude_accept_over_a_failed_check() {
    assert_binding_only_rejects("software-accept-with-failure.json", "is not passed");
}
