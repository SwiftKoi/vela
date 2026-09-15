//! Lexing: source text to tokens.

pub mod comment;
pub mod cursor;
pub mod indent;
pub mod lexer;
pub mod token;

mod lexeme;

pub use comment::{Comment, Pragma};
