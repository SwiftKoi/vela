//! The language server: a thin adapter over the vela-compile query database.
//!
//! # Owns
//!
//! LSP protocol handling and capability mapping, and the one answer to "what does the compiler and
//! the widget vocabulary say about this project" that `vela check` and the editor both publish
//! (`diagnostics`).
//!
//! # Does not own
//!
//! Analysis; `vela-compile` owns every query, and `vela-ui` owns what a screen says. This crate
//! assembles the two and hands them to a protocol — it decides nothing about what is wrong with a
//! program, which is the only way an editor and CI can be guaranteed to agree.

pub mod diagnostics;
pub mod position;
mod server;

#[cfg(test)]
mod tests;

pub use server::Server;
