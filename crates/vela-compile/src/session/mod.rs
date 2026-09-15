//! The compile session: the files in play, their revisions, and memoized queries.
//!
//! The whole design rests on one rule: **a cached result is reused only while every input
//! it read still has the revision it had when the query ran.** That is what makes an edit
//! re-check the module it touched and nothing else, which is the property the language
//! server needs and the reason this exists before the checks that use it.
//!
//! Dependency tracking is built in from the start rather than retrofitted, because it is
//! the one part of a compiler that cannot be added afterwards without rewriting every
//! query: a query that does not record what it read cannot be invalidated correctly, and
//! a query that is invalidated too eagerly is merely slow — so the bug is easy to miss and
//! expensive to find.

mod queries;
mod state;

pub use state::{Compiled, Query, Session};
