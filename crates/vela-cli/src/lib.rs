//! The `vela` command line interface: argument dispatch over a command registry.
//!
//! # Owns
//!
//! Argument dispatch, the Command trait, exit codes, project scaffolding.
//!
//! # Does not own
//!
//! Engine behavior; commands are thin adapters over the engine crates.

mod command;
mod commands;
mod driver;
mod format;
mod manifest;
mod registry;

#[cfg(test)]
mod tests;

pub use command::{Command, Error};
pub use driver::{VERSION, help_text, run, run_code};
pub use registry::{Registry, builtin};
