//! Versioned contract records and the identity rules that make them
//! unambiguous.
//!
//! Every top-level record declares its kind and schema version. Records are
//! parsed header-first: an unsupported kind or version is rejected before the
//! body is examined, so a newer document never fails with a misleading shape
//! error.

pub mod graph;
pub mod ids;
pub mod record;
pub mod registry;

pub use graph::{GraphBudget, GraphRef, GraphRun, GraphSpec, IdentityError, NodeRef, NodeSpec};
pub use ids::{GraphId, InvalidValue, NodeId, Revision, RunId, Timestamp};
pub use record::{
    ContractError, Record, RecordKind, SchemaVersion, parse_record, parse_record_value,
};
