//! Byte offsets, and the positions a protocol asks for.
//!
//! The LSP counts **UTF-16 code units** within a line. Every span in this compiler counts **bytes** —
//! `vela-span`'s `LineCol` says so in as many words — and on ASCII the two agree, which is exactly why
//! this is worth a module and a test in both directions. The first accented character or emoji in a
//! string literal would otherwise put the editor's caret on the wrong character, and only for authors
//! writing in a language that is not English, which is most of them.
//!
//! Nothing here decides *what* to say; it decides where to say it.

use vela_span::{FileId, SourceMap, Span};

/// A position in a document, as a protocol means it: 0-indexed line, 0-indexed UTF-16 column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Position {
    /// 0-indexed line.
    pub line: u32,
    /// 0-indexed column, in UTF-16 code units.
    pub character: u32,
}

/// A range in a document: `start` inclusive, `end` exclusive.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Range {
    /// Where the range begins.
    pub start: Position,
    /// Where it ends.
    pub end: Position,
}

impl Range {
    /// A range covering one position, which is what a caret is.
    #[must_use]
    pub fn empty(at: Position) -> Self {
        Self { start: at, end: at }
    }
}

/// The position a byte offset is at.
///
/// Clamped to the end of the document rather than panicking, for the reason `vela-span` gives: a
/// diagnostic should never be able to crash the thing that displays it.
///
/// # Panics
///
/// Panics if `file` did not come from `sources`. Ids are produced by that map and carried by spans,
/// so an unknown one is a bug here rather than anything a document can cause.
#[must_use]
pub fn position(sources: &SourceMap, file: FileId, offset: u32) -> Position {
    let source = sources.file(file);
    let at = source.line_col(offset);
    let character = source
        .line_text(at.line)
        .map_or(at.col, |text| utf16_up_to(text, at.col));

    Position {
        line: at.line,
        character,
    }
}

/// The byte offset a position is at, or `None` when the line is past the end of the document.
///
/// A column inside a surrogate pair — which the protocol does not allow and an editor can still send —
/// rounds to the end of that character rather than to its middle: a byte offset in the middle of one
/// is not a position in the text.
#[must_use]
pub fn offset(sources: &SourceMap, file: FileId, at: Position) -> Option<u32> {
    let source = sources.file(file);
    let start = source.line_range(at.line)?.start;
    let text = source.line_text(at.line)?;

    Some(start + bytes_up_to(text, at.character))
}

/// The range a span covers.
///
/// # Panics
///
/// Panics if the span's file did not come from `sources`; see [`position`].
#[must_use]
pub fn range(sources: &SourceMap, span: Span) -> Range {
    Range {
        start: position(sources, span.file(), span.start()),
        end: position(sources, span.file(), span.end()),
    }
}

/// How many UTF-16 code units `text` takes up to `bytes`, counting whole characters only.
fn utf16_up_to(text: &str, bytes: u32) -> u32 {
    let mut units = 0;
    let mut taken = 0;

    for character in text.chars() {
        let width = character.len_utf8() as u32;
        if taken + width > bytes {
            break;
        }
        taken += width;
        units += character.len_utf16() as u32;
    }
    units
}

/// How many bytes of `text` the first `units` UTF-16 code units occupy.
fn bytes_up_to(text: &str, units: u32) -> u32 {
    let mut seen = 0;
    let mut bytes = 0;

    for character in text.chars() {
        if seen >= units {
            break;
        }
        seen += character.len_utf16() as u32;
        bytes += character.len_utf8() as u32;
    }
    bytes
}
