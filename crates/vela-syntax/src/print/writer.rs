//! Writing lines: indentation, the comments an author wrote, and the blank lines between them.
//!
//! The printer's other modules decide *what* a line says; this one decides where it goes. Two
//! things make that more than a `String` push: a comment has to be put back where it was (which is
//! why the lexer keeps them), and a blank line has to be put back too — a formatter that closes up
//! the paragraphs of a long story file has destroyed something a reader relies on.

use vela_span::Span;

use crate::lex::Comment;

/// One indentation level, per `LANGUAGE.md §1`.
const INDENT: &str = "    ";

/// The column the formatter wraps a right-hand side at (`TOOLING.md §3`).
pub(crate) const WIDTH: usize = 88;

/// A line-oriented buffer over the source being rewritten.
pub(crate) struct Writer<'a> {
    /// The file as written, for comments, blank lines, and `# fmt: off` regions.
    src: &'a str,
    /// Every comment in the file, in source order.
    comments: &'a [Comment],
    /// How many have been written.
    written: usize,
    /// The `# fmt: off` regions, as byte offsets.
    regions: Vec<(usize, usize)>,
    /// What has been produced.
    out: String,
    /// The current indentation level, in units of four spaces.
    level: usize,
    /// Where the last piece of *code* ended, for telling a trailing comment from a leading one.
    last_end: usize,
    /// Whether the line in progress is empty, so a trailing comment has somewhere to go.
    open_line: bool,
    /// Whether the next line should be preceded by a blank one.
    blank: bool,
}

impl<'a> Writer<'a> {
    /// A writer for one file.
    pub(crate) fn new(src: &'a str, comments: &'a [Comment]) -> Self {
        Self {
            src,
            comments,
            written: 0,
            regions: regions(src, comments),
            out: String::new(),
            level: 0,
            last_end: 0,
            open_line: false,
            blank: false,
        }
    }

    /// Indents by one level.
    pub(crate) fn level_up(&mut self) {
        self.level += 1;
    }

    /// Outdents by one level.
    pub(crate) fn level_down(&mut self) {
        self.level = self.level.saturating_sub(1);
    }

    /// The column the next text would start at.
    pub(crate) fn cursor(&self) -> usize {
        match self.out.rfind('\n') {
            Some(index) if self.open_line => self.out.len() - index - 1,
            _ => self.level * INDENT.len(),
        }
    }

    /// Writes one line at the current indentation.
    pub(crate) fn line(&mut self, text: &str) {
        self.break_line();
        for _ in 0..self.level {
            self.out.push_str(INDENT);
        }
        self.out.push_str(text.trim_end());
        self.open_line = true;
    }

    /// Writes one line with no indentation, for text reproduced as written.
    pub(crate) fn raw(&mut self, text: &str) {
        self.break_line();
        self.out.push_str(text.trim_end());
        self.open_line = true;
    }

    /// Writes a blank line, if one is due.
    fn break_line(&mut self) {
        if self.out.is_empty() {
            self.blank = false;
            return;
        }
        self.out.push('\n');
        if self.blank {
            self.out.push('\n');
        }
        self.blank = false;
    }

    /// Records that code up to `end` has been written.
    pub(crate) fn note(&mut self, end: u32) {
        self.last_end = self.last_end.max(end as usize);
    }

    /// Asks for a blank line before `offset` if the author left one there.
    ///
    /// Never the first line of the file: source that starts with blank lines gets a canonical
    /// form that does not.
    pub(crate) fn blank_before(&mut self, offset: u32) {
        if self.out.is_empty() || self.last_end >= offset as usize {
            return;
        }
        if has_blank_line(self.src, self.last_end, offset as usize) {
            self.blank = true;
        }
    }

    /// Writes every comment that starts before `offset`.
    pub(crate) fn comments_until(&mut self, offset: u32) {
        while let Some(comment) = self.comments.get(self.written) {
            if comment.span.start() >= offset {
                break;
            }
            self.written += 1;
            self.blank_before(comment.span.start());

            if self.open_line && comment.is_trailing(self.src, self.last_end) {
                // Belongs to the line just written: after a statement, or after a declaration's
                // header — the two places an author puts a note about what they just read.
                self.out.push_str("  ");
                self.out.push_str(&comment.rendered());
            } else {
                self.line(&comment.rendered());
            }
            self.note(comment.span.end());
        }
    }

    /// Reproduces the source a statement or item covers, as written.
    ///
    /// What `# fmt: off` asks for: the author has said this region is not the formatter's to lay
    /// out, and the only faithful answer is the text itself.
    pub(crate) fn verbatim(&mut self, span: Span) {
        let text = self
            .src
            .get(span.start() as usize..span.end() as usize)
            .unwrap_or("");
        let mut lines = text.lines();

        if let Some(first) = lines.next() {
            let first = first.to_string();
            self.line(&first);
        }
        for line in lines {
            let line = line.to_string();
            self.raw(&line);
        }
        self.note(span.end());
    }

    /// Whether `offset` falls inside a `# fmt: off` region.
    pub(crate) fn is_off(&self, offset: u32) -> bool {
        let offset = offset as usize;
        self.regions
            .iter()
            .any(|(start, end)| offset >= *start && offset < *end)
    }

    /// The finished file: one trailing newline, no trailing whitespace.
    pub(crate) fn finish(mut self) -> String {
        self.break_line();
        while self.out.ends_with('\n') {
            self.out.pop();
        }
        self.out.push('\n');
        self.out
    }
}

/// The regions a `# fmt: off` / `# fmt: on` pair covers.
///
/// An unterminated `off` runs to the end of the file, which is the reading that loses nothing: a
/// formatter that ignored the marker for want of a closing one would rewrite exactly what the
/// author asked it to leave alone.
fn regions(src: &str, comments: &[Comment]) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    let mut open: Option<usize> = None;

    for comment in comments {
        match comment.text.trim() {
            "fmt: off" if open.is_none() => open = Some(comment.span.end() as usize),
            "fmt: on" => {
                if let Some(start) = open.take() {
                    regions.push((start, comment.span.start() as usize));
                }
            }
            _ => {}
        }
    }
    if let Some(start) = open {
        regions.push((start, src.len()));
    }
    regions
}

/// Whether two offsets are separated by a blank line.
///
/// A line that holds only whitespace, with a line break on either side of it — which is what an
/// author means by a blank line, and what a comment on its own line is *not*.
fn has_blank_line(src: &str, from: usize, to: usize) -> bool {
    let Some(between) = src.get(from..to) else {
        return false;
    };

    let mut break_seen = false;
    for character in between.chars() {
        match character {
            '\n' => {
                if break_seen {
                    return true;
                }
                break_seen = true;
            }
            '\r' | ' ' | '\t' => {}
            _ => break_seen = false,
        }
    }
    false
}
