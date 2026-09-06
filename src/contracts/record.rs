//! Self-describing records and header-first parsing.

use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Kind of a top-level record, as written in its `record` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    /// One immutable graph revision.
    GraphSpec,
    /// One execution of a graph revision.
    GraphRun,
    /// One attempt of one node within one run.
    Attempt,
    /// A verifier's verdict about one exact attempt result.
    VerifierReceipt,
    /// A bounded decision request for a human authority.
    GatePacket,
    /// An authenticated decision bound to one gate packet.
    GateDecision,
    /// Derived, read-only status of one node within one run.
    NodeView,
    /// The exact local candidate a software check runs against.
    PatchResult,
}

impl RecordKind {
    /// The `record` field text.
    pub fn as_str(self) -> &'static str {
        match self {
            RecordKind::GraphSpec => "graph_spec",
            RecordKind::GraphRun => "graph_run",
            RecordKind::Attempt => "attempt",
            RecordKind::VerifierReceipt => "verifier_receipt",
            RecordKind::GatePacket => "gate_packet",
            RecordKind::GateDecision => "gate_decision",
            RecordKind::NodeView => "node_view",
            RecordKind::PatchResult => "patch_result",
        }
    }
}

impl fmt::Display for RecordKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Major version of a record schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaVersion(pub u32);

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A top-level record type with a fixed kind, version, and bundled schema.
pub trait Record: DeserializeOwned {
    /// Value the `record` field must carry.
    const KIND: RecordKind;
    /// Value the `schema_version` field must carry.
    const VERSION: SchemaVersion;
    /// `$id` of the bundled JSON Schema describing this record.
    const SCHEMA_ID: &'static str;
}

/// Why a document could not be read as the requested record.
#[derive(Debug)]
pub enum ContractError {
    /// The text is not JSON.
    Json(serde_json::Error),
    /// The document has no readable `record` / `schema_version` header.
    Header(serde_json::Error),
    /// The header names a different record kind than requested.
    WrongRecord {
        /// Kind the caller asked for.
        expected: RecordKind,
        /// Kind the document declares.
        found: String,
    },
    /// The header names a schema version this build does not support.
    UnsupportedVersion {
        /// Record kind the document declares.
        record: RecordKind,
        /// Version the document declares.
        found: SchemaVersion,
        /// The single version this build supports for that kind.
        supported: SchemaVersion,
    },
    /// The header is supported but the body does not match the record shape.
    Shape(serde_json::Error),
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContractError::Json(e) => write!(f, "document is not valid JSON: {e}"),
            ContractError::Header(e) => {
                write!(f, "document has no valid record/schema_version header: {e}")
            }
            ContractError::WrongRecord { expected, found } => {
                write!(f, "expected a {expected} record, found {found:?}")
            }
            ContractError::UnsupportedVersion {
                record,
                found,
                supported,
            } => write!(
                f,
                "{record} schema_version {found} is not supported; this build supports {supported}"
            ),
            ContractError::Shape(e) => write!(f, "document does not match the record shape: {e}"),
        }
    }
}

impl std::error::Error for ContractError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ContractError::Json(e) | ContractError::Header(e) | ContractError::Shape(e) => Some(e),
            ContractError::WrongRecord { .. } | ContractError::UnsupportedVersion { .. } => None,
        }
    }
}

/// The self-describing header every top-level record carries.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RecordHeader {
    /// The declared record kind, as written (it may be unknown to this build).
    pub record: String,
    /// The declared schema version.
    pub schema_version: SchemaVersion,
}

/// Read the header of a decoded document without interpreting its body.
pub fn read_header(value: &Value) -> Result<RecordHeader, serde_json::Error> {
    RecordHeader::deserialize(value)
}

/// Parse `text` as record `T`, checking the header before the body.
pub fn parse_record<T: Record>(text: &str) -> Result<T, ContractError> {
    let value: Value = serde_json::from_str(text).map_err(ContractError::Json)?;
    parse_record_value(value)
}

/// Parse an already-decoded JSON value as record `T`, checking the header
/// before the body.
pub fn parse_record_value<T: Record>(value: Value) -> Result<T, ContractError> {
    let header = read_header(&value).map_err(ContractError::Header)?;
    if header.record != T::KIND.as_str() {
        return Err(ContractError::WrongRecord {
            expected: T::KIND,
            found: header.record,
        });
    }
    if header.schema_version != T::VERSION {
        return Err(ContractError::UnsupportedVersion {
            record: T::KIND,
            found: header.schema_version,
            supported: T::VERSION,
        });
    }
    T::deserialize(value).map_err(ContractError::Shape)
}
