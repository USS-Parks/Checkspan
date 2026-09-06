//! Resolved evidence references: exact bytes, subject, provenance, and
//! validity.
//!
//! A digest identifies the bytes that were used. Provenance says who claims
//! to have produced them. Neither is a statement that the evidence is true.

use serde::{Deserialize, Serialize};

use super::ids::{Digest, Ident, Timestamp};
use super::node::PolicyRef;

/// Who produced a piece of evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// The producer as observed or declared, such as a CI app or corpus
    /// issuer.
    pub producer: String,
    /// Reference to an attestation that binds the producer to the bytes, when
    /// one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation_ref: Option<String>,
}

/// Evidence resolved for one port of one attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    /// The port this evidence satisfies.
    pub port_name: Ident,
    /// Where the evidence was read from, in the port kind's locator form.
    pub locator: String,
    /// What the evidence is about: a commit, a run, a corpus snapshot.
    pub subject: String,
    /// Producer-side version of the evidence.
    pub version: String,
    /// Digest of the exact bytes used.
    pub content_digest: Digest,
    /// Who produced it.
    pub provenance: Provenance,
    /// Handling policy the evidence was admitted under.
    pub policy_ref: PolicyRef,
    /// Moment after which the evidence is no longer admissible for new work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<Timestamp>,
}
