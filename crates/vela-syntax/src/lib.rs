//! The lexer and parser: `.vela` source text to a concrete syntax tree.
//!
//! # Owns
//!
//! Tokens, comments, indentation handling, syntax tree node types, parse error recovery, the
//! lexical (`E0xxx`) and syntactic (`E1xxx`) diagnostics, and the formatter — which is a function
//! of the tree, so it lives with the tree (`TOOLING.md §3`).
//!
//! # Does not own
//!
//! Name resolution (vela-hir); types (vela-types); diagnostic rendering (vela-diag).
//!
//! # Shape
//!
//! Lexing runs first and completely, producing a flat token vector that already
//! contains `Indent`/`Dedent` tokens. Parsing is then ordinary recursive descent over
//! that vector — the parser never looks at whitespace.
//!
//! Errors are recovered from locally: a mistake inside a statement skips to the next
//! statement boundary, and one at the top level skips to the next item. The tree keeps
//! an `Error` node wherever something was expected, so later phases still see the shape
//! of what was written.

mod error;
mod lex;
mod parse;
mod print;
mod tree;

#[cfg(test)]
mod tests;

pub use lex::Comment;
pub use lex::cursor::Cursor;
pub use lex::indent::{IndentAction, IndentError, IndentStack};
pub use lex::lexer::{LexResult, lex};
pub use lex::token::{Keyword, Token, TokenKind};
pub use parse::{ParseResult, PathRef, parse};
pub use print::{NotFormatted, format};
pub use tree::*;
