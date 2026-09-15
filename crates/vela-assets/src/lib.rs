//! Asset import, transformation, and the content-addressed manifest.
//!
//! # Owns
//!
//! Importer/Transformer registries, Manifest, Digests, font subsetting.
//!
//! # Does not own
//!
//! Packing or shipping (vela-cli); runtime asset reads (vela-host).
//!
//! # Shape of a build
//!
//! ```text
//!   assets/**  ──import──►  artifacts + manifest.json
//!                 |
//!          ImporterRegistry: magic bytes first, extension second
//! ```
//!
//! `import_tree` is the whole of it: an assets directory in, a [`Manifest`] and the artifact
//! bytes out. Where those bytes are written is the caller's question — `vela build` answers it
//! one way, a test answers it another — which is what keeps this crate free of the layout
//! decisions that would make two builds of one source disagree.

mod build;
mod digest;
mod error;
mod importers;
mod manifest;
mod patch;
#[cfg(test)]
mod tests;

pub use build::{Built, import_tree};
pub use digest::Digest;
pub use error::AssetError;
pub use importers::{ImportRequest, Importer, ImporterRegistry, Output};
pub use manifest::{Artifact, Asset, MANIFEST_VERSION, Manifest, Variant};
pub use patch::{PATCH_VERSION, Patch, identity_of};
