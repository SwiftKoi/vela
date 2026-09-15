//! The compile driver: the incremental query database that ties the front end together.
//!
//! # Owns
//!
//! Session, tracked queries, caching, lint passes, CompileResult.
//!
//! # Does not own
//!
//! Language semantics; each phase crate owns its own.
//!
//! # The one invariant
//!
//! A tracked query's result is reusable only while every input it read still has the
//! revision it had when the query ran. Everything below follows from that, and every
//! query added here has to uphold it — see `session.rs`.

mod assets;
mod memo;
mod session;

#[cfg(test)]
mod tests;

pub use session::{Compiled, Query, Session};
