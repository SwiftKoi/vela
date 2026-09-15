//! Checking a module's bodies.

mod coverage;
mod expr;
mod run;
mod stmt;

pub use run::{check, type_of};
