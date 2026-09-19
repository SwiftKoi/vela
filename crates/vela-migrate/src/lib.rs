//! The Ren'Py `.rpy` to `.vela` transpiler and its compat report.
//!
//! # Owns
//!
//! The `.rpy` reader, the transpilation rules, and the report of everything that was not
//! translated.
//!
//! # Does not own
//!
//! Compiling the result (`vela-compile`), or writing it — [`project`] plans a migration and
//! [`Project::write`] performs it, so the rules can be tested without a filesystem.
//!
//! # The rule
//!
//! `TOOLING.md §8`: *"never silently mistranslate"*. A construct outside the supported set
//! produces a [`Report`] entry naming the file, the line, the original text, and what to do
//! about it — because a wrong automatic translation is worse than an explicit "port this by
//! hand": it fails later, and it fails inside someone's save file.
//!
//! ```text
//!   game/*.rpy ──rpy::read──► Node tree ──transpile──► .vela text
//!                                 │
//!                                 └──► Report (file:line + original + reason)
//! ```

mod assets;
mod config;
mod error;
mod expr;
mod gui;
mod project;
mod report;
mod rpy;
mod screens;
mod scripts;
mod testcases;
mod transpile;

#[cfg(test)]
mod tests;

pub use assets::{Asset, Image};
pub use error::MigrateError;
pub use expr::{expression, literal_type, split_assignment, without_translation_call};
pub use project::{Project, Source, project};
pub use report::{Entry, Report};
pub use rpy::{Kind, Node, read};
pub use screens::known_actions;
