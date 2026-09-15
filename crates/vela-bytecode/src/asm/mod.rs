//! Lowering MIR to bytecode.
//!
//! The MIR a module holds is slot-based and typed; bytecode is stack-based and untyped.
//! Everything here is that translation and nothing else — no optimization (MIR is already
//! optimized by this point) and no verification (that is a separate step, because a module
//! should be checkable without being trusted).
//!
//! # Two passes over each body
//!
//! Jump offsets are not known until the code after them is emitted, so each body is emitted
//! with placeholder targets and patched once every block has a position. Patch lists are
//! used rather than a size pre-pass: the two must agree, and the way to guarantee that is to
//! have only one of them.

mod body;
mod build;
mod stmt;
mod term;
mod value;

pub use build::compile;
