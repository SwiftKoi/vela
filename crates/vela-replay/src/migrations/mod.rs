//! Save migrations: the chain that brings an old world up to the current version.
//!
//! A facade. [`chain`] is the ordered steps and how they are applied, [`dsl`] is the macro a
//! new step is written with, and [`registry`] is the one table of what this build ships.

pub mod chain;
pub mod dsl;
pub mod registry;
pub mod v0002_trust_to_affection;
pub mod v0003_frame_anchor;
