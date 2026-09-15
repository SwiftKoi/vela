//! The optimization pass pipeline.
//!
//! Passes live with MIR because they are a property of the IR, not of the driver
//! (`CONVENTIONS.md §4.5`). `vela-compile` only decides *when* to run them.
//!
//! Every pass must be observably semantics-preserving (`BYTECODE.md §2.1`), and
//! "observably" is checked by the differential harness rather than argued: the corpus runs
//! at `-O0` and at `-O2`, and the command streams and final worlds have to match.

mod branch_simplify;
mod cmd_fuse;
mod const_fold;
mod dead_block;
mod inline_small;
mod pass;
mod registry;

pub use pass::{Pass, optimize};
pub use registry::{OptLevel, Pipeline, passes};
