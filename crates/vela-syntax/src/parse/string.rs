//! String literals and their interpolated parts.
//!
//! `"Score: {score}"` is not one string with magic inside it: it is a literal part, then
//! an expression, then a literal part. Splitting that here — rather than leaving it to
//! the type checker to re-parse — means every interpolated expression gets real spans in
//! the real file, so an error inside `{...}` points at the right characters.

use vela_diag::{Diagnostic, Label, Suggestion};
use vela_span::Span;

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
                b'{' => {
                    if !literal.is_empty() {
                        parts.push(StrPart::Literal {
                            span: self.span_of(literal_start, inner_start + i),
                            text: std::mem::take(&mut literal),
                        });
                    }
                    let (body, end) = read_interpolation(inner, i + 1);
                    let body_start = inner_start + i + 1;
                    let expr = self.parse_embedded(body_start, body);
                    parts.push(StrPart::Interpolation {
                        span: self.span_of(inner_start + i, inner_start + end),
                        expr: Box::new(expr),
                    });
                    i = end;
                    literal_start = inner_start + end;
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

    /// Parses the expression inside `{...}`, with spans in the enclosing file.
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
/// An unrecognised escape yields the character itself; reporting it would need a code
/// the language does not have, and the round trip through a string is not lossy enough
/// to be worth one.
fn unescape(escaped: char) -> char {
    match escaped {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '"' => '"',
        '\\' => '\\',
        '{' => '{',
        '}' => '}',
        other => other,
    }
}

/// Finds the `}` matching the `{` just before `from`.
///
/// Returns the body and the offset one past the closing brace. An unterminated
/// interpolation takes the rest of the string rather than failing, so the enclosing
/// expression still has a shape.
fn read_interpolation(inner: &str, from: usize) -> (&str, usize) {
    let bytes = inner.as_bytes();
    let mut depth = 0i32;
    let mut i = from;

    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' if depth == 0 => return (&inner[from..i], i + 1),
            b'}' => depth -= 1,
            _ => {}
        }
        i += 1;
    }

    (&inner[from..], bytes.len())
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
