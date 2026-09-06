//! Content binding: raw digests of artifact bytes and canonical digests of
//! JSON envelopes, so that contracts, manifests, and results have stable
//! identities independent of formatting.
//!
//! Envelope digests use RFC 8785 (the JSON Canonicalization Scheme) followed
//! by SHA-256. Before hashing, an envelope may contain only null, booleans,
//! strings, arrays, objects, and integers within ±2^53. Floats and larger
//! integers are rejected: JCS would reformat them as IEEE doubles and could
//! silently change their meaning. Duplicate keys are rejected by the strict
//! parser before canonicalization. Every typed digest carries the algorithm
//! and an envelope kind, so two different record kinds with the same body
//! never share a digest.

use std::fmt;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use crate::contracts::{
    AttemptRef, Digest, EvidenceRef, GraphSpec, NodeSpec, PatchResult, ReceiptRef,
};
use crate::validation::parse::{ParseError, parse_strict, pointer_push};

/// The raw digest algorithm this build produces.
pub const ALGORITHM: &str = "sha256";

/// The envelope construction this build produces: RFC 8785 canonical bytes
/// hashed with SHA-256, wrapped with the envelope kind.
pub const ENVELOPE_ALGORITHM: &str = "jcs-sha256@1";

/// Largest integer magnitude an envelope may carry: 2^53, the last integer
/// an IEEE double represents exactly.
pub const MAX_SAFE_INTEGER: u64 = 1 << 53;

/// Envelope kind of a graph revision.
pub const GRAPH_SPEC_KIND: &str = "checkspan.graph_spec@1";
/// Envelope kind of a node contract.
pub const NODE_SPEC_KIND: &str = "checkspan.node_spec@1";
/// Envelope kind of an attempt's frozen inputs.
pub const INPUT_MANIFEST_KIND: &str = "checkspan.input_manifest@1";
/// Envelope kind of a complete verification context.
pub const VERIFICATION_CONTEXT_KIND: &str = "checkspan.verification_context@1";
/// Envelope kind of a patch subject digest.
pub const PATCH_SUBJECT_KIND: &str = "checkspan.patch_subject@1";

/// Why a value could not be canonicalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalError {
    /// A number outside the supported envelope subset.
    UnsupportedNumber {
        /// JSON pointer to the number.
        path: String,
        /// The number as written.
        value: String,
    },
    /// The text could not be parsed strictly (syntax, depth, duplicate key).
    Parse(ParseError),
    /// The value could not be serialized.
    Serialize(String),
}

impl fmt::Display for CanonicalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CanonicalError::UnsupportedNumber { path, value } => write!(
                f,
                "number {value} at {path:?} is not an integer within ±2^53; envelopes cannot carry it"
            ),
            CanonicalError::Parse(e) => write!(f, "{e}"),
            CanonicalError::Serialize(e) => write!(f, "cannot serialize envelope: {e}"),
        }
    }
}

impl std::error::Error for CanonicalError {}

/// SHA-256 of exact bytes: the identity of an artifact.
pub fn artifact_digest(bytes: &[u8]) -> Digest {
    let hash = Sha256::digest(bytes);
    Digest::from_sha256(&hash.into())
}

/// RFC 8785 canonical bytes of any JSON value, floats included. Use
/// [`envelope_bytes`] for Checkspan envelopes, which restrict numbers.
pub fn jcs_bytes(value: &Value) -> Result<Vec<u8>, CanonicalError> {
    serde_json_canonicalizer::to_vec(value).map_err(|e| CanonicalError::Serialize(e.to_string()))
}

/// Reject any number outside the envelope subset, reporting the first one
/// found in document order.
pub fn check_envelope_numbers(value: &Value) -> Result<(), CanonicalError> {
    fn walk(value: &Value, path: String) -> Result<(), CanonicalError> {
        match value {
            Value::Number(n) => {
                let safe = n
                    .as_i64()
                    .map(|i| i.unsigned_abs() <= MAX_SAFE_INTEGER)
                    .or_else(|| n.as_u64().map(|u| u <= MAX_SAFE_INTEGER))
                    .unwrap_or(false);
                if safe {
                    Ok(())
                } else {
                    Err(CanonicalError::UnsupportedNumber {
                        path,
                        value: n.to_string(),
                    })
                }
            }
            Value::Array(items) => items
                .iter()
                .enumerate()
                .try_for_each(|(i, item)| walk(item, pointer_push(&path, &i.to_string()))),
            Value::Object(map) => map
                .iter()
                .try_for_each(|(k, v)| walk(v, pointer_push(&path, k))),
            _ => Ok(()),
        }
    }
    walk(value, String::new())
}

/// Canonical bytes of an envelope: numbers checked, then RFC 8785.
pub fn envelope_bytes(value: &Value) -> Result<Vec<u8>, CanonicalError> {
    check_envelope_numbers(value)?;
    jcs_bytes(value)
}

/// SHA-256 of an envelope's canonical bytes.
pub fn envelope_digest(value: &Value) -> Result<Digest, CanonicalError> {
    Ok(artifact_digest(&envelope_bytes(value)?))
}

/// Parse `text` strictly (rejecting duplicate keys, excess depth, and
/// trailing content) and digest it as an envelope.
pub fn envelope_digest_from_text(text: &str, max_depth: usize) -> Result<Digest, CanonicalError> {
    let value = parse_strict(text, max_depth).map_err(CanonicalError::Parse)?;
    envelope_digest(&value)
}

#[derive(Serialize)]
struct Envelope<'a, T: Serialize> {
    algorithm: &'static str,
    kind: &'a str,
    body: &'a T,
}

/// Digest of `body` inside a named envelope. The kind is part of the hashed
/// bytes, so the same body under two kinds has two digests.
pub fn digest_of<T: Serialize>(kind: &str, body: &T) -> Result<Digest, CanonicalError> {
    let envelope = Envelope {
        algorithm: ENVELOPE_ALGORITHM,
        kind,
        body,
    };
    let value =
        serde_json::to_value(&envelope).map_err(|e| CanonicalError::Serialize(e.to_string()))?;
    envelope_digest(&value)
}

/// Identity of the exact candidate a patch result describes: repository,
/// base, candidate, and every changed path with its content digest. Capture
/// time and tool version are not part of it, so recapturing an unchanged
/// candidate reproduces the digest and any change to the candidate does not.
pub fn patch_subject_digest(result: &PatchResult) -> Result<Digest, CanonicalError> {
    digest_of(PATCH_SUBJECT_KIND, &result.subject())
}

/// Content digest of a graph revision. Two documents that differ only in
/// formatting or key order share it; any semantic change does not.
pub fn graph_spec_digest(spec: &GraphSpec) -> Result<Digest, CanonicalError> {
    digest_of(GRAPH_SPEC_KIND, spec)
}

/// Content digest of one node contract.
pub fn node_spec_digest(node: &NodeSpec) -> Result<Digest, CanonicalError> {
    digest_of(NODE_SPEC_KIND, node)
}

#[derive(Serialize)]
struct InputManifest<'a> {
    evidence: Vec<&'a EvidenceRef>,
    dependency_receipts: Vec<&'a ReceiptRef>,
}

/// Digest of an attempt's frozen inputs. Entries are ordered by port name
/// and receipt id, so the digest does not depend on assembly order.
pub fn input_manifest_digest(
    evidence: &[EvidenceRef],
    dependency_receipts: &[ReceiptRef],
) -> Result<Digest, CanonicalError> {
    let mut evidence: Vec<&EvidenceRef> = evidence.iter().collect();
    evidence.sort_by(|a, b| a.port_name.cmp(&b.port_name));
    let mut dependency_receipts: Vec<&ReceiptRef> = dependency_receipts.iter().collect();
    dependency_receipts.sort_by(|a, b| a.id.cmp(&b.id));
    digest_of(
        INPUT_MANIFEST_KIND,
        &InputManifest {
            evidence,
            dependency_receipts,
        },
    )
}

/// Everything a verifier verdict is bound to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerificationContext<'a> {
    /// The exact attempt.
    pub attempt: &'a AttemptRef,
    /// The node contract in force, in full.
    pub node: &'a NodeSpec,
    /// Digest of the frozen inputs.
    pub input_manifest_digest: &'a Digest,
    /// The receipts the attempt was dispatched against.
    pub dependency_receipts: &'a [ReceiptRef],
    /// Digest of the exact result checked.
    pub result_digest: &'a Digest,
    /// Evidence sealed after execution.
    pub produced_evidence: &'a [EvidenceRef],
}

/// Digest of a complete verification context.
pub fn context_digest(context: &VerificationContext<'_>) -> Result<Digest, CanonicalError> {
    digest_of(VERIFICATION_CONTEXT_KIND, context)
}
