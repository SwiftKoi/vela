//! A unified diff, for `vela fmt --diff`.
//!
//! The formatter's output is a whole file, so "what would change" has to be computed here rather
//! than reported by the formatter: it knows the canonical text, not the edit that reaches it. The
//! algorithm is the ordinary one — longest common subsequence over lines, then group the differences
//! into hunks with three lines of context — and it is hand-written because a diff is thirty lines of
//! code and a dependency is a supply-chain decision.
//!
//! Deterministic, and total: two identical texts produce nothing, and any pair of texts produces a
//! patch that `patch -p0` would accept.

use std::ops::Range;

/// Lines of context on each side of a change, as every patch tool prints.
const CONTEXT: usize = 3;

/// A unified diff between two texts, or an empty string when they are equal.
#[must_use]
pub fn unified(path: &str, before: &str, after: &str) -> String {
    let before: Vec<&str> = before.lines().collect();
    let after: Vec<&str> = after.lines().collect();
    let lines = edit_script(&before, &after);
    let ranges = hunks(&lines);

    if ranges.is_empty() {
        return String::new();
    }

    let mut out = format!("--- {path}\n+++ {path}\n");
    for range in ranges {
        let head = lines[..range.start]
            .iter()
            .fold((1usize, 1usize), |(old, new), line| advance(line, old, new));
        let mut old = head.0;
        let mut new = head.1;

        let (old_len, new_len) =
            range
                .clone()
                .fold((0usize, 0usize), |(old_len, new_len), index| {
                    let (o, n) = lengths(&lines[index]);
                    (old_len + o, new_len + n)
                });

        out.push_str(&format!(
            "@@ -{} +{} @@\n",
            span(old, old_len),
            span(new, new_len)
        ));

        for index in range {
            let line = &lines[index];
            out.push_str(prefix(line));
            out.push_str(text(line));
            out.push('\n');
            (old, new) = advance(line, old, new);
        }
    }
    out
}

/// One line of the edit script.
#[derive(Debug)]
enum Line<'a> {
    /// Present in both.
    Keep(&'a str),
    /// Only in `before`.
    Remove(&'a str),
    /// Only in `after`.
    Add(&'a str),
}

/// The text of a line.
fn text<'a>(line: &Line<'a>) -> &'a str {
    match line {
        Line::Keep(text) | Line::Remove(text) | Line::Add(text) => text,
    }
}

/// The character a unified diff marks a line with.
fn prefix(line: &Line<'_>) -> &'static str {
    match line {
        Line::Keep(_) => " ",
        Line::Remove(_) => "-",
        Line::Add(_) => "+",
    }
}

/// How many lines of `before` and of `after` a line accounts for.
fn lengths(line: &Line<'_>) -> (usize, usize) {
    match line {
        Line::Keep(_) => (1, 1),
        Line::Remove(_) => (1, 0),
        Line::Add(_) => (0, 1),
    }
}

/// The line numbers after consuming one line.
fn advance(line: &Line<'_>, old: usize, new: usize) -> (usize, usize) {
    let (o, n) = lengths(line);
    (old + o, new + n)
}

/// A hunk header's range: `start,count`, with the count omitted when it is one.
///
/// A zero-length range names the line *before* it, which is what makes a pure insertion at the top
/// of a file `-0,0` rather than `-1,0` — the convention every patch tool reads.
fn span(start: usize, count: usize) -> String {
    if count == 0 {
        return format!("{},0", start.saturating_sub(1));
    }
    if count == 1 {
        return format!("{start}");
    }
    format!("{start},{count}")
}

/// The longest common subsequence of two lists, as an edit script.
fn edit_script<'a>(before: &[&'a str], after: &[&'a str]) -> Vec<Line<'a>> {
    let table = suffix_lengths(before, after);
    let mut script = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);

    while i < before.len() && j < after.len() {
        if before[i] == after[j] {
            script.push(Line::Keep(before[i]));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            script.push(Line::Remove(before[i]));
            i += 1;
        } else {
            script.push(Line::Add(after[j]));
            j += 1;
        }
    }
    for line in &before[i..] {
        script.push(Line::Remove(line));
    }
    for line in &after[j..] {
        script.push(Line::Add(line));
    }
    script
}

/// `table[i][j]` is the length of the longest common subsequence of `before[i..]` and `after[j..]`.
///
/// Filled from the end so the walk above can read it forwards, which is what makes the script
/// greedy *and* optimal rather than merely plausible.
fn suffix_lengths(before: &[&str], after: &[&str]) -> Vec<Vec<usize>> {
    let mut table = vec![vec![0usize; after.len() + 1]; before.len() + 1];

    for i in (0..before.len()).rev() {
        for j in (0..after.len()).rev() {
            table[i][j] = if before[i] == after[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    table
}

/// The ranges of the script worth printing: every change, with context, merged when they overlap.
fn hunks(lines: &[Line<'_>]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        if matches!(line, Line::Keep(_)) {
            continue;
        }
        let start = index.saturating_sub(CONTEXT);
        let end = (index + CONTEXT + 1).min(lines.len());

        match ranges.last_mut() {
            // `<=` rather than `<`: two hunks whose context touches would otherwise print the same
            // line twice, which a patch tool reads as a conflict.
            Some(last) if start <= last.end => last.end = last.end.max(end),
            _ => ranges.push(start..end),
        }
    }
    ranges
}
