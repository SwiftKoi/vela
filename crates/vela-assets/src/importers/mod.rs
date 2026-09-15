//! Asset importers: one file per source format, chosen by magic bytes then extension.
//!
//! A facade. The trait and the selection rule live in [`registry`], and each format lives
//! beside it — adding one is the checklist in `CONVENTIONS.md §4.4`, which is a new file here
//! plus one line in the registry.

pub mod data;
pub mod registry;
pub mod texture;

pub use registry::{ImportRequest, Importer, ImporterRegistry, Output};
