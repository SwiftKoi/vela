//! The screens a project declares, and the watcher that notices them changing.
//!
//! Splitting a screen into a tree is `vela-ui`'s job; this is the CLI's side of it: which file a
//! screen came from, whether a bundle ships packs instead of sources, and what a caller does with
//! the result — lay it out, draw it, or edit the source and reload.
//!
//! Three files, by responsibility: [`screens`] is the sets a project has and what can be asked of
//! them, [`stack`] is the screens a story has *open*, and [`watch`] is how an edit is noticed.

mod screens;
mod stack;
mod watch;

pub use screens::{Screens, schema};
pub use stack::Stack;
pub use watch::Watcher;
