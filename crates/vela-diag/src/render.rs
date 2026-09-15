//! The human renderer.
//!
//! Output follows the shape developers already read fluently from rustc: a headline,
//! where the problem is, the source line, a caret under the offending bytes, then any
//! notes. Matching a familiar shape is not cosmetic — it means the first error a new
//! author sees explains itself.

use unicode_width::UnicodeWidthChar;
use vela_span::{SourceMap, Span};

use crate::Diagnostic;

/// Placement details shared by every line of one rendered diagnostic.
struct Layout<'a> {
    /// Display name of the file the primary span is in.
    file: &'a str,
    /// 0-indexed line of the primary span.
    line: u32,
    /// Width of the line-number gutter.
    gutter: usize,
    /// Text of the primary span's line.
    text: &'a str,
}

/// Renders a diagnostic against the sources it came from.
///
/// A span that crosses a line break underlines only its first line: multi-line carets
/// are a refinement for a later milestone, and guessing at them would produce
/// misleading output rather than merely plain output.
#[must_use]
pub fn render(diagnostic: &Diagnostic, sources: &SourceMap) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{}[{}]: {}\n",
        diagnostic.severity().as_str(),
        diagnostic.code,
        diagnostic.message
    ));

    let span = diagnostic.primary.span;
    let Some(file) = sources.get(span.file()) else {
        out.push_str("  (source file is not available)\n");
        return out;
    };

    let pos = file.line_col(span.start());
    let layout = Layout {
        file: file.name(),
        line: pos.line,
        gutter: (pos.line + 1).to_string().len().max(1),
        text: file.line_text(pos.line).unwrap_or(""),
    };

    push_location(&mut out, &layout, pos.col + 1);
    push_source_line(&mut out, &layout);
    push_caret(
        &mut out,
        &layout,
        span,
        pos.col as usize,
        &diagnostic.primary.message,
    );
    push_secondary(&mut out, diagnostic, sources, &layout);
    push_footers(&mut out, diagnostic, &layout);
    out
}

/// The `--> file:line:col` pair, plus the blank gutter under it.
fn push_location(out: &mut String, layout: &Layout<'_>, column: u32) {
    let gutter = layout.gutter;
    out.push_str(&format!(
        "{:>gutter$}--> {}:{}:{column}\n",
        "",
        layout.file,
        layout.line + 1
    ));
    out.push_str(&format!("{:>gutter$} |\n", ""));
}

/// The source line itself, with tabs expanded so carets stay aligned.
fn push_source_line(out: &mut String, layout: &Layout<'_>) {
    let gutter = layout.gutter;
    out.push_str(&format!(
        "{:>gutter$} | {}\n",
        layout.line + 1,
        layout.text.replace('\t', "    ")
    ));
}

/// One caret line: padding, carets, then a message.
fn push_caret(out: &mut String, layout: &Layout<'_>, span: Span, col: usize, message: &str) {
    let gutter = layout.gutter;
    let (pad, carets) = caret_geometry(span, layout.text, col);
    out.push_str(&format!(
        "{:>gutter$} | {}{} {message}\n",
        "",
        " ".repeat(pad),
        "^".repeat(carets)
    ));
}

/// Secondary labels: same line gets a caret, a different line gets a location.
fn push_secondary(
    out: &mut String,
    diagnostic: &Diagnostic,
    sources: &SourceMap,
    layout: &Layout<'_>,
) {
    let primary_span = diagnostic.primary.span;
    for label in &diagnostic.secondary {
        let Some(other) = sources.get(label.span.file()) else {
            continue;
        };
        let pos = other.line_col(label.span.start());
        let same_line = label.span.file() == primary_span.file() && pos.line == layout.line;
        if same_line {
            push_caret(out, layout, label.span, pos.col as usize, &label.message);
        } else {
            let gutter = layout.gutter;
            out.push_str(&format!(
                "{:>gutter$} = at {}:{}: {}\n",
                "",
                other.name(),
                pos.line + 1,
                label.message
            ));
        }
    }
}

/// The `= help:` and `= note:` trailer.
fn push_footers(out: &mut String, diagnostic: &Diagnostic, layout: &Layout<'_>) {
    let mut footers: Vec<String> = Vec::new();
    if let Some(help) = &diagnostic.help {
        footers.push(format!("= help: {help}"));
    } else if let Some(suggestion) = &diagnostic.suggestion {
        footers.push(format!("= help: replace with `{}`", suggestion.replacement));
    }
    footers.extend(diagnostic.notes.iter().map(|n| format!("= note: {n}")));

    if footers.is_empty() {
        return;
    }
    let gutter = layout.gutter;
    out.push_str(&format!("{:>gutter$} |\n", ""));
    for footer in footers {
        out.push_str(&format!("{:>gutter$} {footer}\n", ""));
    }
}

/// Display-column offset and caret width for a span within `line_text`.
///
/// `col` is a byte offset, but carets must be placed in *display* columns, so tabs and
/// wide characters are measured rather than counted.
fn caret_geometry(span: Span, line_text: &str, col: usize) -> (usize, usize) {
    let pad = display_width(line_text.get(..col).unwrap_or(""));

    let line_end = line_text.len();
    let from = col.min(line_end);
    let to = (span.len() as usize + col).min(line_end);
    let underlined = line_text.get(from..to).unwrap_or("");

    (pad, display_width(underlined).max(1))
}

/// Width of a string in terminal cells, counting a tab as four columns.
fn display_width(text: &str) -> usize {
    text.chars()
        .map(|c| {
            if c == '\t' {
                4
            } else {
                UnicodeWidthChar::width(c).unwrap_or(0)
            }
        })
        .sum()
}
