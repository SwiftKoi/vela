//! File identifiers and byte spans.

/// Identifies a source file within a [`SourceMap`](crate::SourceMap).
///
/// Ids are dense and assigned in insertion order, so they are cheap to use as
/// array indices and stable for the lifetime of a session.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct FileId(u32);

impl FileId {
    /// Creates an id from its raw index.
    ///
    /// Only a [`SourceMap`](crate::SourceMap) should construct ids; this exists so
    /// other crates can round-trip a id through serialized test data.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw index.
    #[must_use]
    pub const fn as_raw(self) -> u32 {
        self.0
    }
}

/// A half-open byte range `[start, end)` within a single file.
///
/// Spans are the currency between every phase of the compiler: the lexer produces
/// them, diagnostics point at them, and the debugger maps them back to source.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Span {
    file: FileId,
    start: u32,
    end: u32,
}

impl Span {
    /// Creates a span from a file and a byte range.
    ///
    /// The range is not validated against the file's length; a span whose `end`
    /// exceeds the text simply yields empty text when resolved.
    #[must_use]
    pub const fn new(file: FileId, start: u32, end: u32) -> Self {
        Self { file, start, end }
    }

    /// The file this span belongs to.
    #[must_use]
    pub const fn file(self) -> FileId {
        self.file
    }

    /// Byte offset of the first byte.
    #[must_use]
    pub const fn start(self) -> u32 {
        self.start
    }

    /// Byte offset one past the last byte.
    #[must_use]
    pub const fn end(self) -> u32 {
        self.end
    }

    /// Length in bytes.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// Whether the span covers no bytes.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }

    /// The smallest span covering both `self` and `other`.
    ///
    /// Used to grow a span as a construct is parsed. Both spans must be in the same
    /// file; this is a cross-file bug, so it is a real assertion.
    #[must_use]
    pub fn to(self, other: Self) -> Self {
        assert_eq!(
            self.file, other.file,
            "cannot join spans across files: {:?} and {:?}",
            self.file, other.file
        );
        Self::new(
            self.file,
            self.start.min(other.start),
            self.end.max(other.end),
        )
    }

    /// Returns a copy of this span shifted forward by `by` bytes.
    ///
    /// Used when a construct is embedded in a larger buffer and its spans need to be
    /// re-based.
    #[must_use]
    pub const fn offset(self, by: u32) -> Self {
        Self::new(self.file, self.start + by, self.end + by)
    }
}
