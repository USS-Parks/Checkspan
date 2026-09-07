//! Checkspan organizes AI-assisted work as a DAG of immutable contracts,
//! candidate artifacts, explicit dependencies, verifier receipts, and human
//! decisions.
//!
//! The library carries all product behavior; the `checkspan` binary is a thin
//! wrapper around [`cli::run`].

#![warn(missing_docs)]

pub mod adapters;
pub mod budget;
pub mod cli;
pub mod contracts;
pub mod controller;
pub mod deps;
pub mod digests;
pub mod evidence;
pub mod graph;
pub mod receipts;
pub mod scheduler;
pub mod state;
pub mod store;
pub mod validation;
pub mod verifier_host;
pub mod verifiers;
