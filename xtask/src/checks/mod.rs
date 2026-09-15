//! The architecture and policy checks, and the registry that lists them.
//!
//! This module is a facade: the checks live in sibling files and the registry's data
//! lives in `registry.rs`. Adding a check is a table entry plus one new file.

pub mod determinism;
pub mod diag_codes;
pub mod exemptions;
pub mod facade;
pub mod file_size;
pub mod layers;
pub mod registries;

mod registry;

pub use registry::{CHECKS, find};
