//! Primary expressions and their postfix operators.

use vela_span::Span;

use crate::error;
use crate::lex::token::{Keyword, TokenKind};
use crate::parse::parser::Parser;
use crate::tree::Expr;
impl Parser<'_> {
    pub(crate) fn parse_postfix(&mut self) -> Expr {
        let mut expr = self.parse_primary();
        loop {
            match self.peek() {
                TokenKind::Dot => {
                    self.bump();
                    let Some(name) = self.expect_name("`.`") else {
                        return expr;
                    };
                    let span = expr.span().to(self.prev_span());
                    expr = Expr::Field {
                        span,
                        base: Box::new(expr),
                        name,
                    };
                }
                TokenKind::LParen => {
                    let open = self.span();
                    self.bump();
                    let args = self.parse_arguments();
                    let close = self.expect(TokenKind::RParen, "`)`");
                    let span = expr.span().to(close.map_or(open, |token| token.span));
                    expr = Expr::Call {
                        span,
                        callee: Box::new(expr),
                        args,
                    };
                }
                TokenKind::LBracket => {
                    self.bump();
                    let index = self.parse_expr();
                    let close = self.expect(TokenKind::RBracket, "`]`");
                    let span = expr
                        .span()
                        .to(close.map_or(index.span(), |token| token.span));
                    expr = Expr::Index {
                        span,
                        base: Box::new(expr),
                        index: Box::new(index),
                    };
                }
                _ => return expr,
            }
        }
    }

    /// Parses the contents of an argument list, up to but not including `)`.
    pub(crate) fn parse_arguments(&mut self) -> Vec<Expr> {
        let mut args = Vec::new();
        while !self.at(TokenKind::RParen) && !self.at_eof() {
            let before = self.pos;
            args.push(self.parse_expr());
            if self.pos == before {
                self.bump();
                break;
            }
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        args
    }

    /// Parses a comma-separated expression list, stopping before `close`.
    /// One of the three literal words.
    ///
    /// Extracted so `parse_primary` stays inside its line budget, and named for what it does
    /// rather than for where it sits: these three are expressions in a way the other reserved
    /// words are not, which is why they are matched before the general rule below.
    fn parse_literal(&mut self, kind: TokenKind, span: vela_span::Span) -> Expr {
        self.bump();
        match kind {
            TokenKind::Keyword(Keyword::True) => Expr::Bool { span, value: true },
            TokenKind::Keyword(Keyword::False) => Expr::Bool { span, value: false },
            _ => Expr::None { span },
        }
    }

    /// A reserved word in expression position, read as a name.
    ///
    /// `LANGUAGE.md §7.0`: a keyword is special at the start of a *line*, where it opens a
    /// statement or a declaration, and is an ordinary name everywhere else. In an expression
    /// it is a value, and there is no statement for it to be — which is why the arms above
    /// catch the words that *are* expressions (`true`, `none`, a lambda) and this one catches
    /// the rest.
    pub(crate) fn keyword_name(&mut self, span: vela_span::Span) -> Expr {
        let name = self.screen_prop_name().unwrap_or_default();
        Expr::Name { span, name }
    }

    pub(crate) fn parse_arguments_until(&mut self, close: TokenKind) -> Vec<Expr> {
        let mut items = Vec::new();
        while !self.at(close) && !self.at_eof() {
            let before = self.pos;
            items.push(self.parse_expr());
            if self.pos == before {
                self.bump();
                break;
            }
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        items
    }

    pub(crate) fn parse_primary(&mut self) -> Expr {
        let token = self.current();
        let span = token.span;

        match token.kind {
            TokenKind::Int => {
                self.bump();
                Expr::Int {
                    span,
                    value: parse_int(self.text(token)),
                }
            }
            TokenKind::Float => {
                self.bump();
                Expr::Float {
                    span,
                    value: parse_float(self.text(token)),
                }
            }
            TokenKind::Path => {
                self.bump();
                Expr::Path {
                    span,
                    value: parse_path(self.text(token)),
                }
            }
            TokenKind::Str => {
                self.bump();
                self.parse_string(token)
            }
            TokenKind::Keyword(Keyword::True | Keyword::False | Keyword::None) => {
                self.parse_literal(token.kind, span)
            }
            TokenKind::Ident => {
                let name = self.expect_name("an expression").unwrap_or_default();
                Expr::Name { span, name }
            }
            TokenKind::LBracket => {
                self.bump();
                let items = self.parse_arguments_until(TokenKind::RBracket);
                let close = self.expect(TokenKind::RBracket, "`]`");
                Expr::List {
                    span: span.to(close.map_or(span, |t| t.span)),
                    items,
                }
            }
            TokenKind::LBrace => self.parse_map(),
            TokenKind::LParen => {
                self.bump();
                let inner = self.parse_expr();
                let close = self.expect(TokenKind::RParen, "`)`");
                Expr::Paren {
                    span: span.to(close.map_or(inner.span(), |t| t.span)),
                    inner: Box::new(inner),
                }
            }
            TokenKind::Keyword(Keyword::Fn) => self.parse_lambda(),
            // Any other reserved word, as a name.
            //
            // `LANGUAGE.md §7.0`: a keyword is special at the start of a *line*, where it
            // opens a statement or a declaration. In an expression it is a value, and there
            // is no statement for it to be — which is why the arms above catch the words that
            // *are* expressions (`true`, `none`, a lambda) and this one catches the rest.
            // Placed last so they all get their turn first.
            TokenKind::Keyword(_) => self.keyword_name(span),

            _ => {
                let found = self.describe(token);
                self.diagnostics
                    .push(error::expected_expression(span, &found));
                Expr::Error { span }
            }
        }
    }

    /// Parses a map literal, `{a: 1}`.
    pub(crate) fn parse_map(&mut self) -> Expr {
        let open = self.span();
        self.bump();
        let mut entries = Vec::new();

        while !self.at(TokenKind::RBrace) && !self.at_eof() {
            let before = self.pos;
            let key = self.parse_expr();
            if self.expect(TokenKind::Colon, "`:`").is_some() {
                let value = self.parse_expr();
                entries.push((key, value));
            }
            if self.pos == before {
                self.bump();
                break;
            }
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        let close = self.expect(TokenKind::RBrace, "`}`");
        Expr::Map {
            span: open.to(close.map_or(open, |t| t.span)),
            entries,
        }
    }

    /// Parses `fn(params) -> body`.
    pub(crate) fn parse_lambda(&mut self) -> Expr {
        let start = self.span();
        self.bump();
        self.expect(TokenKind::LParen, "`(`");
        let params = self.parse_params();
        self.expect(TokenKind::RParen, "`)`");
        self.expect(TokenKind::Arrow, "`->`");
        let body = self.parse_expr();
        Expr::Lambda {
            span: start.to(body.span()),
            params,
            body: Box::new(body),
        }
    }

    /// Parses a dotted path, for the places the grammar wants a *name* rather than an
    /// expression: label references, image names, and `use` targets.
    pub(crate) fn parse_path(&mut self, what: &str) -> Option<(Vec<String>, Span)> {
        let first = self.expect_name(what)?;
        let mut span = self.prev_span();
        let mut path = vec![first];

        while self.at(TokenKind::Dot) {
            self.bump();
            let Some(segment) = self.expect_name(what) else {
                break;
            };
            path.push(segment);
            span = span.to(self.prev_span());
        }

        Some((path, span))
    }
}

/// Reads an integer literal's value.
///
/// The lexer has already checked the literal's shape and reported a malformed one, so a
/// failure here is not a second diagnostic — it is the value of a literal that is
/// unusable either way, and `0` keeps lowering total.
fn parse_int(text: &str) -> i64 {
    let digits = text.replace('_', "");
    let hex = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"));
    match hex {
        Some(hex) => i64::from_str_radix(hex, 16).unwrap_or(0),
        None => digits.parse().unwrap_or(0),
    }
}

/// Reads a float literal's value.
fn parse_float(text: &str) -> f64 {
    text.replace('_', "").parse().unwrap_or(0.0)
}

/// Reads a path literal's value.
///
/// Shared with the driver, which records every path literal in a file as it is parsed: one
/// definition of what `@"a/b.png"` *means*, so the recorded value and the parsed node cannot
/// disagree.
pub(crate) fn parse_path(text: &str) -> String {
    text.trim_start_matches('@').trim_matches('"').to_string()
}
