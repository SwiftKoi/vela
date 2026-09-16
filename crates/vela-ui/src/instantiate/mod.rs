//! Turning a `screen` body into a widget tree.
//!
//! A screen is a function (`SCREENS.md §2`): arguments in, tree out, nothing mutated. Two
//! responsibilities, split because they answer different questions and because one file that answered
//! both passed the size budget:
//!
//! * [`build`] — which node does this line produce, and what props does it carry.
//! * `compose` — what a `use` expands to, and where a caller's block lands (§2.1).

mod build;
mod compose;

pub use build::build;
