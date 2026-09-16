//! The canonical form of a file (`TOOLING.md §3`).
//!
//! A printer over the syntax tree, not a pretty-printer: it reads the tree and nothing else, so two
//! files with the same tree come out the same, and a file that is already canonical is left exactly
//! as it is. That second property is the one a formatter is judged by, and it is a *test* —
//! `tests/format_roundtrip.rs` formats every fixture in the repository twice and re-parses in
//! between, which is what makes "canonical form" a fact about this code rather than a claim in a
//! document.
//!
//! # What it will not do
//!
//! Reformat a file that does not parse. The tree has gaps where a syntax error was, and printing
//! the gaps would replace the unparsable part with nothing — a formatter must never quietly delete
//! code. Such a file is reported, and the caller decides what to say about it.
//!
//! # Where the canonical form is decided
//!
//! `TOOLING.md §3` states the rules; where the tree cannot distinguish two spellings of one
//! construct, the choice is made here and written down there. Both spellings parse either way, so
//! the choice costs nothing but a diff, and leaving it undecided would make the formatter
//! non-deterministic in practice — two files written differently would never converge.

mod block;
mod decl;
mod expr;
mod item;
mod screen;
mod stmt;
mod test;
mod writer;

pub use item::{NotFormatted, format};
