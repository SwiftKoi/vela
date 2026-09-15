//! Parsing: tokens to a syntax tree.

mod decl;
mod driver;
mod expr;
mod parser;
mod recovery;
mod stmt;
mod string;
mod ty;

pub use driver::{ParseResult, PathRef, parse};
