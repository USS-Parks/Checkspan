//! Versioned contract records and the identity rules that make them
//! unambiguous.
//!
//! Every top-level record declares its kind and schema version. Records are
//! parsed header-first: an unsupported kind or version is rejected before the
//! body is examined, so a newer document never fails with a misleading shape
//! error.

pub mod evidence;
pub mod graph;
pub mod ids;
pub mod node;
pub mod record;
pub mod registry;

pub use evidence::{EvidenceRef, Provenance};
pub use graph::{GraphBudget, GraphRef, GraphRun, GraphSpec, IdentityError, NodeRef};
pub use ids::{
    Digest, Exactly, GraphId, Ident, InvalidValue, NodeId, Revision, RunId, Timestamp, Version,
};
pub use node::{
    AcceptanceSpec, Dependency, FailureClass, NodeContractError, NodeKind, NodeSpec, OnExhaustion,
    PolicyRef, PortKind, PortSpec, ResourceAccess, ResourceClaim, RetryPolicy, TypeRef,
    VerifierRef,
};
pub use record::{
    ContractError, Record, RecordKind, SchemaVersion, parse_record, parse_record_value,
};
