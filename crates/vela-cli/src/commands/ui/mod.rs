//! The screens a project declares, and the watcher that notices them changing.
//!
//! Splitting a screen into a tree is `vela-ui`'s job; this is the CLI's side of it: which file a
//! screen came from, whether a bundle ships packs instead of sources, and what a caller does with
//! the result — lay it out, draw it, or edit the source and reload.
//!
//! Two files, by responsibility: [`screens`] is the sets a project has and what can be asked of
//! them, and [`watch`] is how an edit is noticed. The screens a story has *open* is not this
//! crate's: a stack is what `vela run` and `vela test` have in common, so it lives in `vela-ui`
//! beside the layout (`SCREENS.md §13`, `stack.rs`).

mod screens;
mod watch;

pub use screens::{Screens, schema};
pub use watch::Watcher;
