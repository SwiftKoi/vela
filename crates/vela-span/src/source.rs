//! Source files and the map that owns them.

use std::ops::Range;

use crate::{FileId, Span};

/// A position within a source file.
///
/// `line` and `col` are **0-indexed**, and `col` is a **byte** offset into the line,
/// not a display column. Renderers convert it to a display width (via
/// `unicode-width`) because a tab or a CJK character occupies more than one cell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineCol {
    /// 0-indexed line number.
    pub line: u32,
    /// 0-indexed byte offset within the line.
    pub col: u32,
}

/// One source file: its display name and its full text, plus a line index for cheap
/// offset-to-position lookups.
#[derive(Clone, Debug)]
pub struct SourceFile {
    name: String,
    text: String,
    /// Byte offset of the first character of each line. Always starts with 0.
    line_starts: Vec<u32>,
}

impl SourceFile {
    /// Creates a source file, indexing its line starts.
    #[must_use]
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let line_starts = compute_line_starts(&text);
        Self {
            name: name.into(),
            text,
            line_starts,
        }
    }

    /// The file's display path, exactly as it should appear in a diagnostic.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The full source text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replaces the text, re-indexing the lines.
    ///
    /// A session that keeps its files open (an editor, a build server) needs this: a
    /// source file is not immutable for the life of a process, and the map must stay
    /// consistent with whatever the current text is.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.line_starts = compute_line_starts(&self.text);
    }

    /// Number of lines. A file with no trailing newline has `text.lines().count()`
    /// lines; a file ending in a newline has one extra empty line.
    #[must_use]
    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    /// Converts a byte offset into a [`LineCol`].
    ///
    /// Offsets past the end of the text are clamped to the final line rather than
    /// panicking: a diagnostic should never be able to crash the compiler.
    #[must_use]
    pub fn line_col(&self, offset: u32) -> LineCol {
        let line = self.line_index(offset);
        let start = self.line_starts[line];
        LineCol {
            line: line as u32,
            col: offset.saturating_sub(start),
        }
    }

    /// The byte range of a line, excluding its terminator.
    #[must_use]
    pub fn line_range(&self, line: u32) -> Option<Range<u32>> {
        let start = *self.line_starts.get(line as usize)?;
        let end = match self.line_starts.get(line as usize + 1) {
            Some(&next) => next,
            None => self.text.len() as u32,
        };
        Some(start..trim_terminator(&self.text, start, end))
    }

    /// The text of a line, without its `\n` or `\r\n` terminator.
    #[must_use]
    pub fn line_text(&self, line: u32) -> Option<&str> {
        let range = self.line_range(line)?;
        self.text.get(range.start as usize..range.end as usize)
    }

    /// Index of the line containing `offset`.
    fn line_index(&self, offset: u32) -> usize {
        // `partition_point` counts the starts that are <= offset; the last of those
        // is the line we are on. The first entry is always 0, so the result is >= 1
        // for every offset, which makes the subtraction safe.
        self.line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1)
    }
}

/// Owns every file loaded in a session and hands out dense [`FileId`]s.
///
/// Cloning one copies the text of every file. A caller that needs the map to outlive the
/// session that built it — a debugger holding the sources its spans point into — clones once
/// and keeps it, rather than reaching back into a session it cannot borrow from.
#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    /// Creates an empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file and returns its id.
    pub fn add(&mut self, name: impl Into<String>, text: impl Into<String>) -> FileId {
        let id = FileId::from_raw(self.files.len() as u32);
        self.files.push(SourceFile::new(name, text));
        id
    }

    /// Returns the file for an id.
    ///
    /// # Panics
    ///
    /// Panics if the id did not come from this map. Ids are only ever produced by
    /// `add`, so this is a bug in the caller, not a user-input error.
    #[must_use]
    pub fn file(&self, id: FileId) -> &SourceFile {
        &self.files[id.as_raw() as usize]
    }

    /// Returns the file for an id, or `None` if it is not in this map.
    #[must_use]
    pub fn get(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(id.as_raw() as usize)
    }

    /// Replaces the text of a file already in the map.
    ///
    /// # Panics
    ///
    /// Panics if the id is not in this map. Ids are only produced by `add`, so an
    /// unknown id is a bug in the caller rather than bad input.
    pub fn set_text(&mut self, id: FileId, text: impl Into<String>) {
        let file = self
            .files
            .get_mut(id.as_raw() as usize)
            .expect("file id was not produced by this map");
        file.set_text(text);
    }

    /// The source text covered by a span.
    ///
    /// Returns an empty string if the span is out of range or does not fall on a
    /// character boundary, so a malformed span degrades instead of panicking.
    #[must_use]
    pub fn text(&self, span: Span) -> &str {
        let Some(file) = self.get(span.file()) else {
            return "";
        };
        file.text()
            .get(span.start() as usize..span.end() as usize)
            .unwrap_or("")
    }

    /// The line/column of a span's start.
    #[must_use]
    pub fn line_col(&self, span: Span) -> Option<LineCol> {
        Some(self.get(span.file())?.line_col(span.start()))
    }
}

/// Byte offsets of each line start. The first entry is always 0.
fn compute_line_starts(text: &str) -> Vec<u32> {
    let mut starts = vec![0];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i as u32 + 1);
        }
    }
    starts
}

/// Moves `end` back over a `\r\n` or `\n` terminator so a line's text excludes it.
fn trim_terminator(text: &str, start: u32, end: u32) -> u32 {
    let bytes = text.as_bytes();
    let mut e = end;
    if e > start && bytes.get(e as usize - 1) == Some(&b'\n') {
        e -= 1;
    }
    if e > start && bytes.get(e as usize - 1) == Some(&b'\r') {
        e -= 1;
    }
    e
}
