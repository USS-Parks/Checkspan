//! Checkspan organizes AI-assisted work as a DAG of immutable contracts,
//! candidate artifacts, explicit dependencies, verifier receipts, and human
//! decisions.
//!
//! The library carries all product behavior; the `checkspan` binary is a thin
//! wrapper around [`cli::run`].

#![warn(missing_docs)]

pub mod cli;
pub mod contracts;
pub mod digests;
pub mod graph;
pub mod store;
pub mod validation;
