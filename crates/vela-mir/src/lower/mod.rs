//! Lowering checked syntax into MIR.
//!
//! Split by construct — declarations, expressions, statements, story — because a single
//! traversal that handled all four would be the 3k-line file this project exists to avoid.
//! The split is along the *grammar's* seams, so a construct added to `LANGUAGE.md §3` has
//! one obvious home.

mod access;
mod assign;
pub(crate) mod control;
pub(crate) mod decl;
mod expr;
mod matching;
mod state;
mod stmt;
mod story;
mod values;

pub(crate) use state::Lowerer;
pub use state::lower;
