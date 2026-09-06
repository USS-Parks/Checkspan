//! Bundled JSON Schemas and the record kinds this build supports.
//!
//! Schema `$id`s live under `https://checkspan.invalid/`, a reserved host that
//! never resolves. They are identifiers, not locations: every `$ref` is served
//! from this registry, and no validator in Checkspan fetches schemas from the
//! network or the filesystem.

use super::record::{RecordKind, SchemaVersion};

/// One schema compiled into the binary.
#[derive(Debug, Clone, Copy)]
pub struct BundledSchema {
    /// The schema's `$id`.
    pub id: &'static str,
    /// The schema document text.
    pub source: &'static str,
}

/// Base URI under which every bundled schema `$id` lives.
pub const BASE_URI: &str = "https://checkspan.invalid/schemas/v1/";

macro_rules! bundled {
    ($($file:literal),* $(,)?) => {
        &[$(BundledSchema {
            id: concat!("https://checkspan.invalid/schemas/v1/", $file),
            source: include_str!(concat!("../../schemas/v1/", $file)),
        }),*]
    };
}

/// Every schema this build ships, keyed by `$id`.
pub const SCHEMAS: &[BundledSchema] = bundled![
    "common.schema.json",
    "node-ref.schema.json",
    "graph-ref.schema.json",
    "node-spec.schema.json",
    "evidence-ref.schema.json",
    "graph-spec.schema.json",
    "graph-run.schema.json",
    "attempt.schema.json",
    "verifier-receipt.schema.json",
    "gate-packet.schema.json",
    "gate-decision.schema.json",
    "node-view.schema.json",
    "patch-result.schema.json",
];

/// Record kinds and versions this build can parse, with their schema `$id`.
pub const SUPPORTED: &[(RecordKind, SchemaVersion, &str)] = &[
    (
        RecordKind::GraphSpec,
        SchemaVersion(1),
        "https://checkspan.invalid/schemas/v1/graph-spec.schema.json",
    ),
    (
        RecordKind::GraphRun,
        SchemaVersion(1),
        "https://checkspan.invalid/schemas/v1/graph-run.schema.json",
    ),
    (
        RecordKind::Attempt,
        SchemaVersion(1),
        "https://checkspan.invalid/schemas/v1/attempt.schema.json",
    ),
    (
        RecordKind::VerifierReceipt,
        SchemaVersion(1),
        "https://checkspan.invalid/schemas/v1/verifier-receipt.schema.json",
    ),
    (
        RecordKind::GatePacket,
        SchemaVersion(1),
        "https://checkspan.invalid/schemas/v1/gate-packet.schema.json",
    ),
    (
        RecordKind::GateDecision,
        SchemaVersion(1),
        "https://checkspan.invalid/schemas/v1/gate-decision.schema.json",
    ),
    (
        RecordKind::NodeView,
        SchemaVersion(1),
        "https://checkspan.invalid/schemas/v1/node-view.schema.json",
    ),
    (
        RecordKind::PatchResult,
        SchemaVersion(1),
        "https://checkspan.invalid/schemas/v1/patch-result.schema.json",
    ),
];

/// Look up a bundled schema by exact `$id` (any fragment must already be
/// removed).
pub fn bundled(id: &str) -> Option<&'static str> {
    SCHEMAS.iter().find(|s| s.id == id).map(|s| s.source)
}

/// The schema `$id` for a supported record kind and version.
pub fn schema_for(kind: RecordKind, version: SchemaVersion) -> Option<&'static str> {
    SUPPORTED
        .iter()
        .find(|(k, v, _)| *k == kind && *v == version)
        .map(|(_, _, id)| *id)
}
