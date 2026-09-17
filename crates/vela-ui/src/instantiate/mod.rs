//! Turning a `screen` body into a widget tree.
//!
//! A screen is a function (`SCREENS.md §2`): arguments in, tree out, nothing mutated. Three
//! responsibilities, split because they answer different questions and because one file that answered
//! all of them passed the size budget:
//!
//! * [`build`] — which node does this line produce, and what props does it carry.
//! * `compose` — what a `use` expands to, and where a caller's block lands (§2.1).
//! * [`bindings`] — what the screen answers and when it acts on its own (§2.3).

mod bindings;
mod build;
mod compose;
mod props;

pub use bindings::bindings;
pub use build::build;
