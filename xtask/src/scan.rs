//! Shared source-scanning helpers.
//!
//! Several checks need to look at Rust source without being fooled by comments or
//! string literals, and one of them (`check-exemptions`) needs to see comments and
//! nothing else. Both needs are served by a single lexer here, rather than a regex per
//! check.

use std::fs;
use std::path::{Path, PathBuf};

/// Directory names that are never descended into.
const SKIP_DIRS: &[&str] = &["target", ".git", "node_modules", ".commandcode"];

/// Recursively collects files under `root` with one of `exts`, **sorted** so every
/// check produces identical output on identical input.
#[must_use]
pub fn walk(root: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(root, exts, &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) || name.starts_with('.') {
                continue;
            }
            collect(&path, exts, out);
        } else if path
            .extension()
            .is_some_and(|e| exts.contains(&e.to_string_lossy().as_ref()))
        {
            out.push(path);
        }
    }
}

/// Reads a file, returning `None` (rather than failing a check) if it is not UTF-8.
#[must_use]
pub fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

/// Number of lines, matching what a text editor reports: a trailing newline does not
/// create an extra counted line.
#[must_use]
pub fn line_count(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    text.lines().count()
}

/// A non-doc line comment (`// ...`), with doc comments (`///`, `//!`) excluded.
#[derive(Debug)]
pub struct LineComment {
    /// 1-indexed line the comment starts on.
    pub line: usize,
    /// Comment text following the `//`.
    pub body: String,
}

/// A scanned source file: the original with comments blanked, plus its line comments.
#[derive(Debug)]
pub struct Scanned {
    /// Source with every comment replaced by spaces. Byte length is preserved, so
    /// offsets and line numbers still line up with the original.
    pub blanked: String,
    /// Non-doc line comments, in source order.
    pub comments: Vec<LineComment>,
}

/// Scans a source file once, producing a comment-blanked copy and the line comments.
///
/// Strings and char literals are preserved in `blanked` so callers can still match
/// code; comments are recorded separately so callers can match prose. Doc comments are
/// deliberately *not* recorded: they describe the code, so treating them as directives
/// would make documentation self-triggering.
#[must_use]
pub fn scan_source(src: &str) -> Scanned {
    let bytes = src.as_bytes();
    let mut blanked = bytes.to_vec();
    let mut comments = Vec::new();
    let mut i = 0;
    let mut line = 1usize;

    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                let (end, is_doc) = line_comment(bytes, i);
                if !is_doc {
                    comments.push(LineComment {
                        line,
                        body: String::from_utf8_lossy(&bytes[i + 2..end]).into_owned(),
                    });
                }
                blanked[i..end].fill(b' ');
                i = end;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                let (end, crossed) = block_comment(bytes, i, &mut blanked);
                line += crossed;
                i = end;
            }
            b'"' => {
                let (end, crossed) = string_literal(bytes, i + 1);
                line += crossed;
                i = end;
            }
            b'\'' => i = char_literal(bytes, i),
            _ => i += 1,
        }
    }

    Scanned {
        blanked: String::from_utf8(blanked).unwrap_or_else(|_| src.to_string()),
        comments,
    }
}

/// Convenience wrapper for callers that only need the blanked copy.
#[must_use]
pub fn blank_comments(src: &str) -> String {
    scan_source(src).blanked
}

/// End offset of the line comment starting at `start`, and whether it is a doc comment.
fn line_comment(bytes: &[u8], start: usize) -> (usize, bool) {
    let is_doc = matches!(bytes.get(start + 2), Some(b'/' | b'!'));
    let mut end = start + 2;
    while end < bytes.len() && bytes[end] != b'\n' {
        end += 1;
    }
    (end, is_doc)
}

/// Blanks a block comment, returning its end offset and how many newlines it crossed.
///
/// Rust block comments nest, so this tracks depth rather than scanning for the first
/// `*/`.
fn block_comment(bytes: &[u8], start: usize, blanked: &mut [u8]) -> (usize, usize) {
    blanked[start..start + 2].fill(b' ');

    let mut i = start + 2;
    let mut depth = 1u32;
    let mut crossed = 0usize;

    while i < bytes.len() && depth > 0 {
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            blanked[i..i + 2].fill(b' ');
            depth += 1;
            i += 2;
        } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
            blanked[i..i + 2].fill(b' ');
            depth -= 1;
            i += 2;
        } else {
            if bytes[i] == b'\n' {
                crossed += 1;
            } else {
                blanked[i] = b' ';
            }
            i += 1;
        }
    }

    (i, crossed)
}

/// Skips a string literal whose contents begin at `start`.
///
/// Returns its end offset and how many newlines it crossed (a multi-line raw string is
/// not modelled, but an escaped line continuation is).
fn string_literal(bytes: &[u8], start: usize) -> (usize, usize) {
    let mut i = start;
    let mut crossed = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => {
                i += 1;
                break;
            }
            b'\n' => {
                crossed += 1;
                i += 1;
            }
            _ => i += 1,
        }
    }

    (i, crossed)
}

/// End offset of a char literal or a lifetime starting at `start`.
///
/// A lifetime (`'a`) looks like the start of a char literal, so only a well-formed
/// `'x'` or `'\x'` is consumed; anything else is ordinary code.
fn char_literal(bytes: &[u8], start: usize) -> usize {
    let n = bytes.len();
    if start + 3 < n && bytes[start + 1] == b'\\' && bytes[start + 3] == b'\'' {
        start + 4
    } else if start + 2 < n && bytes[start + 2] == b'\'' {
        start + 3
    } else {
        start + 1
    }
}

/// Replaces the *contents* of string and char literals with spaces, preserving byte
/// length.
///
/// `blank_comments` deliberately keeps literals intact, because some checks need to
/// match code that lives in them. Brace counting is the opposite case: a test containing
/// `"{a} {b}"` would otherwise appear to open two blocks and run to the end of the file.
#[must_use]
pub fn blank_literals(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let mut j = i + 1;
                while j < bytes.len() {
                    if bytes[j] == b'\\' {
                        j += 2;
                        continue;
                    }
                    if bytes[j] == b'"' {
                        break;
                    }
                    out[j] = b' ';
                    j += 1;
                }
                i = j.saturating_add(1);
            }
            b'\'' => {
                if i + 3 < bytes.len() && bytes[i + 1] == b'\\' && bytes[i + 3] == b'\'' {
                    out[i + 1..i + 3].fill(b' ');
                    i += 4;
                } else if i + 2 < bytes.len() && bytes[i + 2] == b'\'' {
                    out[i + 1] = b' ';
                    i += 3;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }

    String::from_utf8(out).unwrap_or_else(|_| src.to_string())
}

/// What kind of block a brace scan found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// A function body.
    Fn,
    /// An `impl` block, which contains its own functions.
    Impl,
}

/// A braced block and how many lines it spans.
#[derive(Debug)]
pub struct Block {
    /// Whether this is a function or an `impl`.
    pub kind: BlockKind,
    /// 1-indexed line of the signature.
    pub start_line: usize,
    /// Total lines from the signature through the closing brace.
    pub lines: usize,
}

/// Finds every `fn`/`impl` block in comment-blanked source.
///
/// Signatures are recognised at the start of a trimmed line, which matches rustfmt
/// output and avoids matching mentions inside expressions.
#[must_use]
pub fn find_blocks(blanked: &str) -> Vec<Block> {
    let lines: Vec<&str> = blanked.lines().collect();
    let mut blocks = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let Some(kind) = block_kind(line) else {
            continue;
        };
        let (started, end) = measure(&lines, idx);
        if started {
            blocks.push(Block {
                kind,
                start_line: idx + 1,
                lines: end - idx + 1,
            });
        }
    }

    blocks
}

/// Scans forward from `start` until the braces opened there balance.
///
/// Returns `(found_body, last_line_index)`. A `;` before any `{` means this was a
/// declaration with no body.
fn measure(lines: &[&str], start: usize) -> (bool, usize) {
    let mut depth: i32 = 0;
    let mut opened = false;

    for (j, line) in lines.iter().enumerate().skip(start) {
        for ch in line.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        if opened && depth <= 0 {
            return (true, j);
        }
        if !opened && line.contains(';') {
            return (false, j);
        }
    }

    (opened, lines.len().saturating_sub(1))
}

/// Classifies a line as the start of an `fn` or `impl` block, if it is one.
fn block_kind(line: &str) -> Option<BlockKind> {
    let mut rest = line.trim_start();
    // Strip the leading modifiers rustfmt may put before the keyword.
    loop {
        let stripped = [
            "pub(crate)",
            "pub(super)",
            "pub",
            "async",
            "const",
            "unsafe",
            "default",
        ]
        .iter()
        .find_map(|kw| rest.strip_prefix(*kw))
        .map(str::trim_start);
        match stripped {
            Some(next) if next.len() < rest.len() => rest = next,
            _ => break,
        }
    }

    if rest.starts_with("fn ") || rest.starts_with("fn<") {
        return Some(BlockKind::Fn);
    }
    if rest.starts_with("impl ") || rest.starts_with("impl<") {
        return Some(BlockKind::Impl);
    }
    None
}
