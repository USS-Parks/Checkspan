//! Bounded offline document validation.
//!
//! A document passes through fixed stages in order: size, encoding, strict
//! parse (depth, duplicate keys, trailing content), record header, bundled
//! JSON Schema, typed shape, and record-level contract rules. The first
//! failing stage stops the pipeline, and every diagnostic carries the JSON
//! pointer of the value it concerns. Nothing here reads the filesystem or the
//! network.

pub mod parse;
pub mod schema;

use std::fmt;

use serde_json::Value;

use crate::contracts::registry;
use crate::contracts::{
    Attempt, ContractError, GateDecision, GatePacket, GraphRun, GraphSpec, IdentityError, NodeView,
    Record, RecordHeader, RecordKind, VerifierReceipt, parse_record_value, read_header,
};

pub use parse::{ParseError, ParseErrorKind};
pub use schema::{SUPPORTED_KEYWORDS, SchemaError};

/// Input bounds applied before any interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum document size in bytes.
    pub max_bytes: usize,
    /// Maximum container nesting depth.
    pub max_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_bytes: 4 * 1024 * 1024,
            max_depth: 64,
        }
    }
}

/// The pipeline stage a diagnostic comes from, in pipeline order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Stage {
    /// The document exceeds `max_bytes`.
    Size,
    /// The bytes are not UTF-8 JSON, or there is trailing content.
    Syntax,
    /// Containers nest deeper than `max_depth`.
    Depth,
    /// An object repeats a key.
    DuplicateKey,
    /// No supported `record` / `schema_version` header.
    Header,
    /// The bundled JSON Schema rejects the document.
    Schema,
    /// The typed record shape rejects the document.
    Shape,
    /// A record-level contract rule rejects the document.
    Contract,
}

impl Stage {
    /// The stage name used in machine output.
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Size => "size",
            Stage::Syntax => "syntax",
            Stage::Depth => "depth",
            Stage::DuplicateKey => "duplicate_key",
            Stage::Header => "header",
            Stage::Schema => "schema",
            Stage::Shape => "shape",
            Stage::Contract => "contract",
        }
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One reason a document was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Which stage produced it.
    pub stage: Stage,
    /// JSON pointer to the value concerned (`""` is the root).
    pub path: String,
    /// Human-readable detail.
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {:?}: {}", self.stage, self.path, self.message)
    }
}

/// A fully validated record of any supported kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidatedRecord {
    /// A graph revision.
    GraphSpec(GraphSpec),
    /// A graph run.
    GraphRun(GraphRun),
    /// An attempt.
    Attempt(Attempt),
    /// A verifier receipt.
    VerifierReceipt(VerifierReceipt),
    /// A gate packet.
    GatePacket(GatePacket),
    /// A gate decision.
    GateDecision(GateDecision),
    /// A node view.
    NodeView(NodeView),
}

impl ValidatedRecord {
    /// The record kind.
    pub fn kind(&self) -> RecordKind {
        match self {
            ValidatedRecord::GraphSpec(_) => RecordKind::GraphSpec,
            ValidatedRecord::GraphRun(_) => RecordKind::GraphRun,
            ValidatedRecord::Attempt(_) => RecordKind::Attempt,
            ValidatedRecord::VerifierReceipt(_) => RecordKind::VerifierReceipt,
            ValidatedRecord::GatePacket(_) => RecordKind::GatePacket,
            ValidatedRecord::GateDecision(_) => RecordKind::GateDecision,
            ValidatedRecord::NodeView(_) => RecordKind::NodeView,
        }
    }
}

/// The result of validating one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The header as declared, when one could be read.
    pub header: Option<RecordHeader>,
    /// Every diagnostic from the first failing stage; empty when valid.
    pub diagnostics: Vec<Diagnostic>,
    /// The validated record; present only when there are no diagnostics.
    pub record: Option<ValidatedRecord>,
}

impl Report {
    /// Whether the document passed every stage.
    pub fn is_valid(&self) -> bool {
        self.diagnostics.is_empty() && self.record.is_some()
    }

    /// The stage that rejected the document, if any.
    pub fn failed_stage(&self) -> Option<Stage> {
        self.diagnostics.first().map(|d| d.stage)
    }
}

fn reject(header: Option<RecordHeader>, diagnostics: Vec<Diagnostic>) -> Report {
    Report {
        header,
        diagnostics,
        record: None,
    }
}

fn one(stage: Stage, path: impl Into<String>, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic {
        stage,
        path: path.into(),
        message: message.into(),
    }]
}

/// Validate `bytes` as one Checkspan record under `limits`.
pub fn validate(bytes: &[u8], limits: &Limits) -> Report {
    if bytes.len() > limits.max_bytes {
        return reject(
            None,
            one(
                Stage::Size,
                "",
                format!(
                    "document is {} bytes; the limit is {}",
                    bytes.len(),
                    limits.max_bytes
                ),
            ),
        );
    }
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) => {
            return reject(
                None,
                one(Stage::Syntax, "", format!("document is not UTF-8: {e}")),
            );
        }
    };
    let value = match parse::parse_strict(text, limits.max_depth) {
        Ok(value) => value,
        Err(e) => {
            let stage = match e.kind {
                ParseErrorKind::Syntax => Stage::Syntax,
                ParseErrorKind::Depth => Stage::Depth,
                ParseErrorKind::DuplicateKey => Stage::DuplicateKey,
            };
            return reject(None, one(stage, e.path, e.message));
        }
    };
    if !value.is_object() {
        return reject(
            None,
            one(Stage::Header, "", "document is not a JSON object"),
        );
    }
    let header = match read_header(&value) {
        Ok(header) => header,
        Err(e) => {
            return reject(
                None,
                one(
                    Stage::Header,
                    "",
                    format!("document has no valid record/schema_version header: {e}"),
                ),
            );
        }
    };
    let Some((kind, schema_id)) = registry::SUPPORTED
        .iter()
        .find(|(k, v, _)| k.as_str() == header.record && *v == header.schema_version)
        .map(|(k, _, id)| (*k, *id))
    else {
        let supported: Vec<String> = registry::SUPPORTED
            .iter()
            .map(|(k, v, _)| format!("{k}@{v}"))
            .collect();
        return reject(
            Some(header.clone()),
            one(
                Stage::Header,
                "",
                format!(
                    "record {:?} schema_version {} is not supported; supported records: {}",
                    header.record,
                    header.schema_version,
                    supported.join(", ")
                ),
            ),
        );
    };
    let validator = match schema::validator_for(schema_id) {
        Ok(validator) => validator,
        Err(e) => return reject(Some(header), one(Stage::Schema, "", e.to_string())),
    };
    let schema_diagnostics: Vec<Diagnostic> = validator
        .iter_errors(&value)
        .map(|e| Diagnostic {
            stage: Stage::Schema,
            path: e.instance_path().to_string(),
            message: e.to_string(),
        })
        .collect();
    if !schema_diagnostics.is_empty() {
        return reject(Some(header), schema_diagnostics);
    }
    match typed(kind, value) {
        Ok((record, contract_diagnostics)) if contract_diagnostics.is_empty() => Report {
            header: Some(header),
            diagnostics: Vec::new(),
            record: Some(record),
        },
        Ok((_, contract_diagnostics)) => reject(Some(header), contract_diagnostics),
        Err(e) => reject(Some(header), one(Stage::Shape, "", e.to_string())),
    }
}

fn contract(path: impl Into<String>, error: impl fmt::Display) -> Diagnostic {
    Diagnostic {
        stage: Stage::Contract,
        path: path.into(),
        message: error.to_string(),
    }
}

fn identity_path(error: &IdentityError) -> &'static str {
    match error {
        IdentityError::DuplicateNodeId(_) => "/nodes",
        IdentityError::NoTargets
        | IdentityError::DuplicateTarget(_)
        | IdentityError::ForeignTarget(_)
        | IdentityError::UnknownTarget(_)
        | IdentityError::TargetRevisionMismatch { .. } => "/targets",
        IdentityError::SupersedesForeignGraph(_) | IdentityError::SupersedesNotEarlier(_) => {
            "/supersedes"
        }
        IdentityError::ZeroAttemptBudget => "/budget/total_attempts",
        IdentityError::SelfLineage(_) => "/budget_lineage_ref",
    }
}

fn typed(
    kind: RecordKind,
    value: Value,
) -> Result<(ValidatedRecord, Vec<Diagnostic>), ContractError> {
    Ok(match kind {
        RecordKind::GraphSpec => {
            let spec: GraphSpec = parse_record_value(value)?;
            let mut diagnostics: Vec<Diagnostic> = spec
                .check_identity()
                .iter()
                .map(|e| contract(identity_path(e), e))
                .collect();
            for (index, node) in spec.nodes.iter().enumerate() {
                let path = format!("/nodes/{index}");
                diagnostics.extend(
                    node.check_contract()
                        .iter()
                        .map(|e| contract(path.clone(), e)),
                );
            }
            (ValidatedRecord::GraphSpec(spec), diagnostics)
        }
        RecordKind::GraphRun => {
            let run: GraphRun = parse_record_value(value)?;
            let diagnostics = run
                .check_identity()
                .iter()
                .map(|e| contract(identity_path(e), e))
                .collect();
            (ValidatedRecord::GraphRun(run), diagnostics)
        }
        RecordKind::Attempt => {
            let attempt: Attempt = parse_record_value(value)?;
            let diagnostics = attempt
                .check_bindings()
                .iter()
                .map(|e| contract("", e))
                .collect();
            (ValidatedRecord::Attempt(attempt), diagnostics)
        }
        RecordKind::VerifierReceipt => {
            let receipt: VerifierReceipt = parse_record_value(value)?;
            let diagnostics = receipt
                .check_bindings()
                .iter()
                .map(|e| contract("", e))
                .collect();
            (ValidatedRecord::VerifierReceipt(receipt), diagnostics)
        }
        RecordKind::GatePacket => {
            let packet: GatePacket = parse_record_value(value)?;
            let diagnostics = packet
                .check_bindings()
                .iter()
                .map(|e| contract("/requested_decision", e))
                .collect();
            (ValidatedRecord::GatePacket(packet), diagnostics)
        }
        RecordKind::GateDecision => {
            let decision: GateDecision = parse_record_value(value)?;
            let diagnostics = decision
                .check_bindings()
                .iter()
                .map(|e| contract("", e))
                .collect();
            (ValidatedRecord::GateDecision(decision), diagnostics)
        }
        RecordKind::NodeView => {
            let view: NodeView = parse_record_value(value)?;
            let diagnostics = view
                .check_bindings()
                .iter()
                .map(|e| contract("/status", e))
                .collect();
            (ValidatedRecord::NodeView(view), diagnostics)
        }
    })
}

/// The schema `$id` a validated record was checked against.
pub fn schema_id_for(kind: RecordKind) -> &'static str {
    match kind {
        RecordKind::GraphSpec => GraphSpec::SCHEMA_ID,
        RecordKind::GraphRun => GraphRun::SCHEMA_ID,
        RecordKind::Attempt => Attempt::SCHEMA_ID,
        RecordKind::VerifierReceipt => VerifierReceipt::SCHEMA_ID,
        RecordKind::GatePacket => GatePacket::SCHEMA_ID,
        RecordKind::GateDecision => GateDecision::SCHEMA_ID,
        RecordKind::NodeView => NodeView::SCHEMA_ID,
    }
}
