//! String literals and their interpolated parts.
//!
//! `"Score: [score]"` is not one string with magic inside it: it is a literal part, then an
//! expression, then a literal part. Splitting that here — rather than leaving it to the type
//! checker to re-parse — means every interpolated expression gets real spans in the real file, so
//! an error inside `[...]` points at the right characters.
//!
//! # Two sigils, and why they are not the same one
//!
//! `[` interpolates a value and `{` is reserved for text tags, which is the split Ren'Py arrived
//! at and for the same reason: both are things a VN puts *inside* dialogue, and one sigil cannot
//! be both. `"{b}Hi{/b}"` must mean bold, `"Score: [score]"` must mean the number, and with one
//! sigil the first reads as "interpolate `b`". A brace is therefore an error today rather than a
//! literal (`E0010`), because the two readings differ in meaning and accepting the wrong one now
//! would change what already-written dialogue says.

use vela_diag::{Diagnostic, Label, Suggestion};
use vela_span::Span;

use crate::error;
use crate::lex::lexer::lex;
use crate::lex::token::Token;
use crate::parse::parser::Parser;
use crate::tree::{Expr, StrPart};

impl Parser<'_> {
    /// Splits a string literal into literal and interpolated parts.
    ///
    /// An interpolated expression is parsed by re-lexing its source and handing the
    /// tokens to a sub-parser whose spans are pre-offset, so the resulting expressions
    /// carry absolute positions in the real file and need no post-hoc shifting.
    pub(crate) fn parse_string(&mut self, token: Token) -> Expr {
        let raw = self.text(token);
        let inner = raw
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(raw);
        let inner_start = token.span.start() as usize + 1;

        let mut parts = Vec::new();
        let mut literal = String::new();
        let mut literal_start = inner_start;
        let mut i = 0usize;

        while i < inner.len() {
            match inner.as_bytes()[i] {
                b'\\' => {
                    if let Some(escaped) = inner[i + 1..].chars().next() {
                        literal.push(unescape(escaped));
                        i += 1 + escaped.len_utf8();
                    } else {
                        i += 1;
                    }
                }
                // `[[` is one literal `[`, which is how a string says "not an interpolation".
                b'[' if inner[i + 1..].starts_with('[') => {
                    literal.push('[');
                    i += 2;
                }
                b'[' => {
                    i = self.interpolation(
                        inner,
                        i,
                        inner_start,
                        literal_start,
                        &mut literal,
                        &mut parts,
                    );
                    literal_start = inner_start + i;
                }
                // `{{` is one literal `{`; a bare brace is a text tag, which does not exist yet.
                b'{' if inner[i + 1..].starts_with('{') => {
                    literal.push('{');
                    i += 2;
                }
                b'{' => {
                    let end = tag_end(inner, i);
                    self.diagnostics.push(error::reserved_text_tag(
                        self.file,
                        (inner_start + i) as u32,
                        (inner_start + end) as u32,
                    ));
                    // Recovered as literal text: the string still has a shape, and the reader gets
                    // one diagnostic rather than a cascade from whatever the braces confused.
                    literal.push('{');
                    i += 1;
                }
                _ => {
                    let ch = inner[i..].chars().next().unwrap_or('\u{fffd}');
                    literal.push(ch);
                    i += ch.len_utf8();
                }
            }
        }

        if !literal.is_empty() || parts.is_empty() {
            parts.push(StrPart::Literal {
                span: self.span_of(literal_start, inner_start + inner.len()),
                text: literal,
            });
        }

        Expr::Str {
            span: token.span,
            parts,
        }
    }

    /// Emits the interpolation that starts at `at`, and returns the offset past its `]`.
    ///
    /// Split out of the scan so that the scan reads as what it is — literal, interpolation, literal
    /// — rather than as the bookkeeping of one of the three.
    fn interpolation(
        &mut self,
        inner: &str,
        at: usize,
        inner_start: usize,
        literal_start: usize,
        literal: &mut String,
        parts: &mut Vec<StrPart>,
    ) -> usize {
        if !literal.is_empty() {
            parts.push(StrPart::Literal {
                span: self.span_of(literal_start, inner_start + at),
                text: std::mem::take(literal),
            });
        }

        let (body, end) = read_bracketed(inner, at + 1);
        let expr = self.parse_embedded(inner_start + at + 1, body);
        parts.push(StrPart::Interpolation {
            span: self.span_of(inner_start + at, inner_start + end),
            expr: Box::new(expr),
        });
        end
    }

    /// Parses the expression inside `[...]`, with spans in the enclosing file.
    pub(crate) fn parse_embedded(&mut self, body_start: usize, body: &str) -> Expr {
        let lexed = lex(self.file, body);
        let shifted: Vec<Token> = lexed
            .tokens
            .iter()
            .map(|token| Token::new(token.kind, token.span.offset(body_start as u32)))
            .collect();

        let mut sub = Parser::new(self.file, self.src, &shifted);
        let expr = sub.parse_expr();

        // Lexical diagnostics carry offsets into `body`, so they are shifted here; the
        // sub-parser's own diagnostics already point into the file, because its tokens
        // do.
        let mut diagnostics: Vec<Diagnostic> = lexed
            .diagnostics
            .into_iter()
            .map(|diagnostic| shift(diagnostic, body_start as u32))
            .collect();
        diagnostics.extend(sub.take_diagnostics());
        self.diagnostics.extend(diagnostics);

        expr
    }

    /// A span in the enclosing file from two absolute byte offsets.
    fn span_of(&self, start: usize, end: usize) -> Span {
        Span::new(self.file, start as u32, end as u32)
    }
}

/// Resolves a backslash escape to the character it stands for.
///
/// An unrecognised escape yields the character itself. That is what makes `\[` and `\{` spell a
/// literal bracket and brace — they are *accepted*, and the formatter writes them as `[[` and `{{`
/// because those are the escapes the language documents, which is the same "a redundant escape is
/// dropped" rule `TOOLING.md §3` states.
fn unescape(escaped: char) -> char {
    match escaped {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '"' => '"',
        '\\' => '\\',
        other => other,
    }
}

/// Finds the `]` matching the `[` just before `from`.
///
/// Returns the body and the offset one past the closing bracket. An unterminated interpolation takes
/// the rest of the string rather than failing, so the enclosing expression still has a shape.
///
/// Nested brackets are counted and a string literal inside the body is stepped over: `["a]b"]` is
/// one interpolation whose body is a list, and a scan that stopped at the first `]` would cut the
/// body in half and then re-lex the remains as source.
fn read_bracketed(inner: &str, from: usize) -> (&str, usize) {
    let bytes = inner.as_bytes();
    let mut depth = 0i32;
    let mut i = from;

    while i < bytes.len() {
        match bytes[i] {
            b'[' => depth += 1,
            b']' if depth == 0 => return (&inner[from..i], i + 1),
            b']' => depth -= 1,
            b'"' => i = end_of_string(inner, i),
            _ => {}
        }
        i += 1;
    }

    (&inner[from..], bytes.len())
}

/// The offset of the closing quote of the string literal that starts at `from`.
///
/// The end of the text when there is none, so a malformed body degrades instead of panicking.
fn end_of_string(inner: &str, from: usize) -> usize {
    let bytes = inner.as_bytes();
    let mut i = from + 1;

    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'"' => return i,
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

/// One past the `}` that closes the tag starting at `from`, or the end of the text.
///
/// The tags themselves are not parsed: the language has not decided what they say yet, and a parser
/// for a syntax that does not exist would be a guess to unlearn later. Only the extent is needed, to
/// point at what the reader wrote.
fn tag_end(inner: &str, from: usize) -> usize {
    match inner[from + 1..].find('}') {
        Some(offset) => from + offset + 2,
        None => inner.len(),
    }
}

/// Moves every span in a diagnostic forward, for diagnostics produced by a sub-parser.
fn shift(diagnostic: Diagnostic, by: u32) -> Diagnostic {
    Diagnostic {
        code: diagnostic.code,
        message: diagnostic.message,
        primary: Label {
            span: diagnostic.primary.span.offset(by),
            message: diagnostic.primary.message,
        },
        secondary: diagnostic
            .secondary
            .into_iter()
            .map(|label| Label {
                span: label.span.offset(by),
                message: label.message,
            })
            .collect(),
        notes: diagnostic.notes,
        help: diagnostic.help,
        suggestion: diagnostic.suggestion.map(|suggestion| Suggestion {
            span: suggestion.span.offset(by),
            replacement: suggestion.replacement,
        }),
    }
}
