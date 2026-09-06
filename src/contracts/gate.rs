//! Gate packets and gate decisions: a bounded question for a human authority
//! and the authenticated answer bound to it.
//!
//! Recording a decision establishes that an authentic, well-formed decision
//! exists. It does not mean "approved": a denial is a valid decision, and no
//! decision of any kind is a verifier verdict.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::attempt::ProvenanceLevel;
use super::graph::NodeRef;
use super::ids::{Digest, Ident, RunId, Timestamp};
use super::node::PolicyRef;
use super::record::{Record, RecordKind, SchemaVersion};

/// Why a packet exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatePurpose {
    /// Route an undecidable or exhausted node: retry, revise, or cancel.
    ResolveWork,
    /// Record a scoped human assessment of research claims.
    AssessClaims,
    /// Approve or deny one exact privileged action.
    AuthorizeAction,
}

impl GatePurpose {
    /// Every purpose, for coverage checks.
    pub const ALL: [GatePurpose; 3] = [
        GatePurpose::ResolveWork,
        GatePurpose::AssessClaims,
        GatePurpose::AuthorizeAction,
    ];

    /// The `purpose` field text.
    pub fn as_str(self) -> &'static str {
        match self {
            GatePurpose::ResolveWork => "resolve_work",
            GatePurpose::AssessClaims => "assess_claims",
            GatePurpose::AuthorizeAction => "authorize_action",
        }
    }

    /// Decisions a packet with this purpose may offer.
    pub fn allowed_decisions(self) -> &'static [DecisionKind] {
        match self {
            GatePurpose::ResolveWork => &[
                DecisionKind::Retry,
                DecisionKind::Revise,
                DecisionKind::Cancel,
            ],
            GatePurpose::AssessClaims => &[DecisionKind::RecordAssessment],
            GatePurpose::AuthorizeAction => {
                &[DecisionKind::ApproveAction, DecisionKind::DenyAction]
            }
        }
    }
}

impl fmt::Display for GatePurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The typed decision an authority can make.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    /// Allow another attempt under the unchanged contract.
    Retry,
    /// Create a new node or graph revision.
    Revise,
    /// Cancel the node.
    Cancel,
    /// Authorize the exact action in scope.
    ApproveAction,
    /// Refuse the exact action in scope.
    DenyAction,
    /// Record a scoped semantic assessment.
    RecordAssessment,
}

impl DecisionKind {
    /// Every decision kind, for coverage checks.
    pub const ALL: [DecisionKind; 6] = [
        DecisionKind::Retry,
        DecisionKind::Revise,
        DecisionKind::Cancel,
        DecisionKind::ApproveAction,
        DecisionKind::DenyAction,
        DecisionKind::RecordAssessment,
    ];

    /// The `decision` field text.
    pub fn as_str(self) -> &'static str {
        match self {
            DecisionKind::Retry => "retry",
            DecisionKind::Revise => "revise",
            DecisionKind::Cancel => "cancel",
            DecisionKind::ApproveAction => "approve_action",
            DecisionKind::DenyAction => "deny_action",
            DecisionKind::RecordAssessment => "record_assessment",
        }
    }
}

impl fmt::Display for DecisionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Privileged actions that only a human authority can grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Send a message or notification.
    Send,
    /// Merge into a protected branch.
    Merge,
    /// Read or use a secret.
    SecretAccess,
    /// Spend money.
    Pay,
    /// Delete data.
    Delete,
}

/// One exact privileged action against one exact target.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRequest {
    /// The action.
    pub action: ActionKind,
    /// The exact target, such as a branch, recipient, or path.
    pub target: String,
}

/// What kind of sealed context a packet references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextKind {
    /// A sealed attempt record.
    Attempt,
    /// A verifier receipt.
    Receipt,
    /// A candidate or evidence artifact.
    Artifact,
    /// A claim-to-source matrix under assessment.
    ClaimMatrix,
}

/// Sealed context the authority is asked to consider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextRef {
    /// What the reference points at.
    pub kind: ContextKind,
    /// Its identity.
    pub id: String,
    /// Digest of the sealed bytes.
    pub digest: Digest,
}

/// What the packet asks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestedDecision {
    /// The decisions the authority may choose from.
    pub options: Vec<DecisionKind>,
    /// The exact action in question; only for `authorize_action`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ActionRequest>,
}

/// A bounded decision request for a human authority.
///
/// The packet references sealed context; it never waits for the paused
/// node's acceptance, and nothing in it can carry a verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatePacket {
    /// Always `gate_packet`.
    pub record: RecordKind,
    /// Always `1` for this build.
    pub schema_version: SchemaVersion,
    /// Packet identity.
    pub id: Ident,
    /// The run.
    pub run_id: RunId,
    /// The node whose work or action is in question.
    pub node: NodeRef,
    /// Why the packet exists.
    pub purpose: GatePurpose,
    /// Sealed context to consider.
    pub context_refs: Vec<ContextRef>,
    /// Digest over the sealed packet content; a decision binds to it.
    pub subject_digest: Digest,
    /// The decisions offered.
    pub requested_decision: RequestedDecision,
    /// The policy that says who may decide.
    pub authority_policy_ref: PolicyRef,
    /// When the packet was created.
    pub created_at: Timestamp,
    /// When the packet stops accepting decisions.
    pub expires_at: Timestamp,
}

impl Record for GatePacket {
    const KIND: RecordKind = RecordKind::GatePacket;
    const VERSION: SchemaVersion = SchemaVersion(1);
    const SCHEMA_ID: &'static str = "https://checkspan.invalid/schemas/v1/gate-packet.schema.json";
}

/// Outcome of a scoped human assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentOutcome {
    /// The reviewed claims are adequately supported by their cited sources.
    AdequateSupport,
    /// The reviewed claims are not adequately supported.
    InadequateSupport,
    /// The reviewer could not determine support.
    CannotDetermine,
}

impl AssessmentOutcome {
    /// Every outcome, for coverage checks.
    pub const ALL: [AssessmentOutcome; 3] = [
        AssessmentOutcome::AdequateSupport,
        AssessmentOutcome::InadequateSupport,
        AssessmentOutcome::CannotDetermine,
    ];
}

/// A scoped semantic assessment. It is human evidence, never a machine pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assessment {
    /// Digest of the exact claim matrix reviewed.
    pub claim_matrix_digest: Digest,
    /// The claims the reviewer examined.
    pub reviewed_claim_ids: Vec<Ident>,
    /// The reviewer's conclusion.
    pub outcome: AssessmentOutcome,
    /// Reference to the reviewer's rationale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale_ref: Option<String>,
}

/// Exactly what a decision covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionScope {
    /// The run.
    pub run_id: RunId,
    /// The node.
    pub node: NodeRef,
    /// The exact action, for action decisions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ActionRequest>,
}

/// Who decided and how that is authenticated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authority {
    /// The authority identity as authenticated.
    pub identity: String,
    /// Authentication level of that identity binding.
    pub level: ProvenanceLevel,
}

/// An authenticated decision bound to one gate packet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateDecision {
    /// Always `gate_decision`.
    pub record: RecordKind,
    /// Always `1` for this build.
    pub schema_version: SchemaVersion,
    /// The packet decided.
    pub gate_packet_id: Ident,
    /// The packet's `subject_digest` at decision time.
    pub gate_packet_digest: Digest,
    /// Who decided.
    pub authority: Authority,
    /// When.
    pub decided_at: Timestamp,
    /// The decision.
    pub decision: DecisionKind,
    /// The assessment payload; present exactly for `record_assessment`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assessment: Option<Assessment>,
    /// Exactly what the decision covers.
    pub scope: DecisionScope,
    /// When the decision stops being usable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Timestamp>,
    /// Reference to the external signature or authorization record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_ref: Option<String>,
}

impl Record for GateDecision {
    const KIND: RecordKind = RecordKind::GateDecision;
    const VERSION: SchemaVersion = SchemaVersion(1);
    const SCHEMA_ID: &'static str =
        "https://checkspan.invalid/schemas/v1/gate-decision.schema.json";
}

/// A rule that a structurally valid packet or decision breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateError {
    /// The packet offers no decision.
    NoOptions,
    /// The same option is offered twice.
    DuplicateOption(DecisionKind),
    /// The packet offers a decision its purpose does not allow.
    OptionNotAllowedForPurpose {
        /// The packet purpose.
        purpose: GatePurpose,
        /// The offending option.
        option: DecisionKind,
    },
    /// An `authorize_action` packet names no action.
    ActionRequired,
    /// A packet that is not `authorize_action` names an action.
    ActionNotAllowed(GatePurpose),
    /// The packet references no sealed context.
    EmptyContext,
    /// The same context reference appears twice.
    DuplicateContextRef(String),
    /// The authority identity is empty.
    EmptyIdentity,
    /// `record_assessment` has no assessment payload.
    AssessmentRequired,
    /// A decision other than `record_assessment` carries an assessment.
    AssessmentNotAllowed(DecisionKind),
    /// An assessment reviews no claims.
    EmptyReviewedClaims,
    /// An action decision has no action in scope.
    ActionScopeRequired(DecisionKind),
    /// A non-action decision names an action in scope.
    ActionScopeNotAllowed(DecisionKind),
    /// The decision names a different packet.
    PacketIdMismatch {
        /// The packet offered for binding.
        expected: Ident,
        /// The packet the decision names.
        found: Ident,
    },
    /// The packet content changed since the decision was made.
    PacketDigestMismatch {
        /// The packet's current digest.
        expected: Digest,
        /// The digest the decision was bound to.
        found: Digest,
    },
    /// The packet did not offer this decision.
    DecisionNotOffered(DecisionKind),
    /// The decision scope names a different run.
    ScopeRunMismatch,
    /// The decision scope names a different node.
    ScopeNodeMismatch,
    /// The decision scope names a different action than the packet.
    ScopeActionMismatch,
}

impl fmt::Display for GateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GateError::NoOptions => f.write_str("packet offers no decision"),
            GateError::DuplicateOption(d) => write!(f, "option {d} is offered more than once"),
            GateError::OptionNotAllowedForPurpose { purpose, option } => {
                write!(f, "a {purpose} packet cannot offer {option}")
            }
            GateError::ActionRequired => {
                f.write_str("an authorize_action packet must name the exact action")
            }
            GateError::ActionNotAllowed(p) => write!(f, "a {p} packet cannot name an action"),
            GateError::EmptyContext => f.write_str("packet references no sealed context"),
            GateError::DuplicateContextRef(id) => {
                write!(f, "context reference {id:?} appears more than once")
            }
            GateError::EmptyIdentity => f.write_str("authority identity is empty"),
            GateError::AssessmentRequired => {
                f.write_str("record_assessment requires an assessment payload")
            }
            GateError::AssessmentNotAllowed(d) => {
                write!(f, "a {d} decision cannot carry an assessment")
            }
            GateError::EmptyReviewedClaims => f.write_str("assessment reviews no claims"),
            GateError::ActionScopeRequired(d) => {
                write!(f, "a {d} decision must name the exact action in scope")
            }
            GateError::ActionScopeNotAllowed(d) => {
                write!(f, "a {d} decision cannot name an action in scope")
            }
            GateError::PacketIdMismatch { expected, found } => {
                write!(f, "decision is about packet {found}, not {expected}")
            }
            GateError::PacketDigestMismatch { expected, found } => write!(
                f,
                "decision was bound to packet digest {found} but the packet is now {expected}"
            ),
            GateError::DecisionNotOffered(d) => write!(f, "the packet did not offer {d}"),
            GateError::ScopeRunMismatch => f.write_str("decision scope names another run"),
            GateError::ScopeNodeMismatch => f.write_str("decision scope names another node"),
            GateError::ScopeActionMismatch => {
                f.write_str("decision scope names a different action than the packet")
            }
        }
    }
}

impl std::error::Error for GateError {}

impl GatePacket {
    /// Check every rule that concerns this record alone.
    pub fn check_bindings(&self) -> Vec<GateError> {
        let mut errors = Vec::new();
        let options = &self.requested_decision.options;
        if options.is_empty() {
            errors.push(GateError::NoOptions);
        }
        let mut seen = HashSet::new();
        for option in options {
            if !seen.insert(option) {
                errors.push(GateError::DuplicateOption(*option));
            } else if !self.purpose.allowed_decisions().contains(option) {
                errors.push(GateError::OptionNotAllowedForPurpose {
                    purpose: self.purpose,
                    option: *option,
                });
            }
        }
        match (self.purpose, &self.requested_decision.action) {
            (GatePurpose::AuthorizeAction, None) => errors.push(GateError::ActionRequired),
            (GatePurpose::ResolveWork | GatePurpose::AssessClaims, Some(_)) => {
                errors.push(GateError::ActionNotAllowed(self.purpose));
            }
            _ => {}
        }
        if self.context_refs.is_empty() {
            errors.push(GateError::EmptyContext);
        }
        let mut seen_context = HashSet::new();
        for context in &self.context_refs {
            if !seen_context.insert((&context.kind, &context.id)) {
                errors.push(GateError::DuplicateContextRef(context.id.clone()));
            }
        }
        errors
    }
}

impl GateDecision {
    /// Check every rule that concerns this record alone.
    pub fn check_bindings(&self) -> Vec<GateError> {
        let mut errors = Vec::new();
        if self.authority.identity.trim().is_empty() {
            errors.push(GateError::EmptyIdentity);
        }
        match (self.decision, &self.assessment) {
            (DecisionKind::RecordAssessment, None) => errors.push(GateError::AssessmentRequired),
            (DecisionKind::RecordAssessment, Some(a)) if a.reviewed_claim_ids.is_empty() => {
                errors.push(GateError::EmptyReviewedClaims);
            }
            (other, Some(_)) if other != DecisionKind::RecordAssessment => {
                errors.push(GateError::AssessmentNotAllowed(other));
            }
            _ => {}
        }
        let is_action = matches!(
            self.decision,
            DecisionKind::ApproveAction | DecisionKind::DenyAction
        );
        match (is_action, &self.scope.action) {
            (true, None) => errors.push(GateError::ActionScopeRequired(self.decision)),
            (false, Some(_)) => errors.push(GateError::ActionScopeNotAllowed(self.decision)),
            _ => {}
        }
        errors
    }

    /// Check that this decision answers exactly `packet` as it currently
    /// stands. An empty list means the decision binds.
    pub fn binds(&self, packet: &GatePacket) -> Vec<GateError> {
        let mut errors = Vec::new();
        if self.gate_packet_id != packet.id {
            errors.push(GateError::PacketIdMismatch {
                expected: packet.id.clone(),
                found: self.gate_packet_id.clone(),
            });
        }
        if self.gate_packet_digest != packet.subject_digest {
            errors.push(GateError::PacketDigestMismatch {
                expected: packet.subject_digest.clone(),
                found: self.gate_packet_digest.clone(),
            });
        }
        if !packet.requested_decision.options.contains(&self.decision) {
            errors.push(GateError::DecisionNotOffered(self.decision));
        }
        if self.scope.run_id != packet.run_id {
            errors.push(GateError::ScopeRunMismatch);
        }
        if self.scope.node != packet.node {
            errors.push(GateError::ScopeNodeMismatch);
        }
        if self.scope.action != packet.requested_decision.action {
            errors.push(GateError::ScopeActionMismatch);
        }
        errors
    }

    /// Whether this decision refuses the action or work in question.
    pub fn is_denial(&self) -> bool {
        matches!(
            self.decision,
            DecisionKind::DenyAction | DecisionKind::Cancel
        )
    }

    /// Whether this decision authorizes exactly `action`. Only an
    /// `approve_action` decision whose scope names that exact action does.
    pub fn authorizes(&self, action: &ActionRequest) -> bool {
        self.decision == DecisionKind::ApproveAction && self.scope.action.as_ref() == Some(action)
    }
}
