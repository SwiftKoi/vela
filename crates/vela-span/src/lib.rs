//! Byte offsets, spans, file ids, and the source map that resolves offsets to line/column
//! positions.
//!
//! # Owns
//!
//! Span arithmetic, FileId allocation, SourceMap line indexing.
//!
//! # Does not own
//!
//! Diagnostics or rendering (vela-diag); lexing (vela-syntax).

mod source;
mod span;

#[cfg(test)]
mod tests;

pub use source::{LineCol, SourceFile, SourceMap};
pub use span::{FileId, Span};
