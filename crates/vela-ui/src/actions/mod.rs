//! The action registry.
//!
//! `SCREENS.md §7`: *"the action set is a registry. Adding an action is a registry entry, not a
//! UI-core edit."* The same shape as widgets, and for the same reason — an action set grows with what
//! a project wants to do, and a `match` on an action name would be a core file every new action edits.
//!
//! Interaction produces **typed** actions and a screen never mutates state directly, which is what
//! keeps rendering one-directional: `paint` reads, actions write, and nothing does both.
//! `set(trust, trust + 1)` is a description of a mutation the runtime performs, not a mutation a
//! button performs.
//!
//! Two files, because the vocabulary is a table and the types are not: `decl.rs` is what an action
//! *is*, and `builtin.rs` is which ones exist.

mod builtin;
mod decl;

pub use builtin::{
    OPEN_SCREEN, PREFERENCE, REPLACE_SCREEN, SET_SCREEN_VARIABLE, TOGGLE_PREFERENCE,
};
pub use decl::{Action, ActionDecl, ActionRegistry};
