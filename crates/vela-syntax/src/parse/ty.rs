//! Parsing types and patterns.

use crate::error;
use crate::lex::token::TokenKind;
use crate::parse::parser::Parser;
use crate::tree::{Pattern, Type};

impl Parser<'_> {
    /// Parses a type, including any trailing `?`.
    pub(crate) fn parse_type(&mut self) -> Type {
        let mut ty = self.parse_type_atom();
        while self.at(TokenKind::Question) {
            let span = ty.span().to(self.span());
            self.bump();
            ty = Type::Optional {
                span,
                inner: Box::new(ty),
            };
        }
        ty
    }

    /// Parses a type with no trailing `?`.
    pub(crate) fn parse_type_atom(&mut self) -> Type {
        let token = self.current();
        let start = token.span;

        if self.at(TokenKind::LParen) {
            self.bump();
            let mut elements = Vec::new();
            while !self.at(TokenKind::RParen) && !self.at_eof() {
                let before = self.pos;
                elements.push(self.parse_type());
                if self.pos == before {
                    self.bump();
                    break;
                }
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            let close = self.expect(TokenKind::RParen, "`)`");
            return Type::Tuple {
                span: start.to(close.map_or(start, |t| t.span)),
                elements,
            };
        }

        if !self.at(TokenKind::Ident) {
            let found = self.describe(token);
            self.diagnostics.push(error::expected_type(start, &found));
            return Type::Error { span: start };
        }

        let Some((path, span)) = self.parse_path("a type") else {
            return Type::Error { span: start };
        };

        // Only `list` and `map` take type arguments; the language has no user generics
        // (`LANGUAGE.md §10`), so anything else here is just a name.
        if !self.at(TokenKind::Lt) {
            return Type::Named { span, path };
        }

        let name = path.first().map(String::as_str).unwrap_or("");
        if path.len() != 1 || (name != "list" && name != "map") {
            return Type::Named { span, path };
        }

        self.bump();
        let key = self.parse_type();
        let close_span = self.span();
        if name == "map" {
            self.expect(TokenKind::Comma, "`,`");
            let value = self.parse_type();
            let close = self.expect(TokenKind::Gt, "`>`");
            return Type::Map {
                span: span.to(close.map_or(close_span, |t| t.span)),
                key: Box::new(key),
                value: Box::new(value),
            };
        }

        let close = self.expect(TokenKind::Gt, "`>`");
        Type::List {
            span: span.to(close.map_or(close_span, |t| t.span)),
            element: Box::new(key),
        }
    }

    /// Parses a `when` pattern: a variant path with bindings, or `_`.
    pub(crate) fn parse_pattern(&mut self) -> Pattern {
        let start = self.span();

        if self.at(TokenKind::Ident) && self.text(self.current()) == "_" {
            self.bump();
            return Pattern {
                span: start,
                path: Vec::new(),
                bindings: Vec::new(),
            };
        }

        let Some((path, span)) = self.parse_path("a pattern") else {
            return Pattern {
                span: start,
                path: Vec::new(),
                bindings: Vec::new(),
            };
        };

        let mut bindings = Vec::new();
        if self.at(TokenKind::LParen) {
            self.bump();
            while !self.at(TokenKind::RParen) && !self.at_eof() {
                let before = self.pos;
                if let Some(name) = self.expect_name("a binding") {
                    bindings.push(name);
                }
                if self.pos == before {
                    self.bump();
                    break;
                }
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RParen, "`)`");
        }

        Pattern {
            span,
            path,
            bindings,
        }
    }
}
