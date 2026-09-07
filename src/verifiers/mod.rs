//! Verifier implementations this binary ships.
//!
//! A verifier here is still a separate process at run time: the controller
//! spawns this same binary as a protocol child under a pinned profile. Being
//! compiled into one executable changes distribution, not the trust
//! boundary.

pub mod patch;
pub mod software;
