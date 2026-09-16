//! Parameters, declaration bodies, and the small declarations that have none.

use crate::lex::token::{Keyword, TokenKind};
use crate::parse::parser::Parser;
use crate::tree::{ConstDecl, DefaultDecl, Param, Setting, StructField, Type, UseDecl};
impl Parser<'_> {
    /// Parses `( param, ... )` contents, up to but not including `)`.
    pub(crate) fn parse_params(&mut self) -> Vec<Param> {
        let mut params = Vec::new();

        while !self.at(TokenKind::RParen) && !self.at_eof() {
            let before = self.pos;
            let start = self.span();

            let Some(name) = self.expect_name("a parameter") else {
                break;
            };
            // The type is optional: `param = IDENT [ ":" type ]` (`LANGUAGE.md §3`). A screen's
            // parameters are usually written without one, and an absent type is the checker's
            // `Ty::Unknown` rather than a syntax error — so the colon is what decides.
            let ty = self.parse_optional_type();
            let default = if self.eat(TokenKind::Eq) {
                Some(self.parse_expr())
            } else {
                None
            };
            let end = match (&default, &ty) {
                (Some(expr), _) => expr.span(),
                (None, Some(ty)) => ty.span(),
                // Neither a type nor a default was written, so the span ends with the name —
                // the last token consumed.
                (None, None) => self.prev_span(),
            };
            params.push(Param {
                span: start.to(end),
                name,
                ty,
                default,
            });

            if self.pos == before {
                self.bump();
                break;
            }
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        params
    }

    /// Parses the indented field list of a `struct`.
    pub(crate) fn parse_fields(&mut self) -> Vec<StructField> {
        let mut fields = Vec::new();
        if !self.enter_block("`struct`") {
            return fields;
        }

        while !self.at(TokenKind::Dedent) && !self.at_eof() {
            let before = self.pos;
            let start = self.span();
            if let Some(name) = self.expect_name("a field") {
                self.expect(TokenKind::Colon, "`:`");
                let ty = self.parse_type();
                let default = if self.eat(TokenKind::Eq) {
                    Some(self.parse_expr())
                } else {
                    None
                };
                let end = default
                    .as_ref()
                    .map_or_else(|| ty.span(), |expr| expr.span());
                fields.push(StructField {
                    span: start.to(end),
                    name,
                    ty,
                    default,
                });
            }
            self.end_statement();
            if self.pos == before {
                self.bump();
            }
        }

        self.eat(TokenKind::Dedent);
        fields
    }

    /// Parses a `key = value` block, as used by characters, styles, and themes.
    pub(crate) fn parse_settings(&mut self, construct: &str) -> Vec<Setting> {
        self.parse_typed_settings(construct, false)
    }

    /// Parses a setting body, optionally allowing a leading type word.
    ///
    /// The type word is a *theme* thing: `color bg = 0x10121a` names a colour token, while a
    /// style's `color = theme.fg` names the colour property. Both are `word word = value` and
    /// `word = value` shapes, so the only place that can tell them apart is the construct
    /// being parsed — which is here.
    pub(crate) fn parse_typed_settings(&mut self, construct: &str, typed: bool) -> Vec<Setting> {
        let mut settings = Vec::new();
        if !self.enter_block(construct) {
            return settings;
        }

        while !self.at(TokenKind::Dedent) && !self.at_eof() {
            let before = self.pos;
            let start = self.span();
            if let Some(first) = self.expect_name("a setting") {
                // `key = value`, or `type key = value`. The spec writes tokens with a type
                // word (`SCREENS.md §5`: `color bg = 0x10121a`), and that word is the only
                // thing distinguishing a colour from a length once the value is just a
                // number — the parser does not otherwise record how one was written.
                let (ty, key) = if self.at(TokenKind::Eq) || !typed {
                    (None, first)
                } else if let Some(key) = self.expect_name("a setting name") {
                    (Some(first), key)
                } else {
                    self.end_statement();
                    continue;
                };
                self.expect(TokenKind::Eq, "`=`");
                let value = self.parse_expr();
                settings.push(Setting {
                    span: start.to(value.span()),
                    ty,
                    key,
                    value,
                });
            }
            self.end_statement();
            if self.pos == before {
                self.bump();
            }
        }

        self.eat(TokenKind::Dedent);
        settings
    }

    /// Parses a `: Type`, if one is written.
    pub(crate) fn parse_optional_type(&mut self) -> Option<Type> {
        if self.at(TokenKind::Colon) {
            self.bump();
            return Some(self.parse_type());
        }
        None
    }

    /// Parses `use a.b.c [as name]`.
    pub(crate) fn parse_use(&mut self) -> UseDecl {
        let start = self.span();
        self.bump();
        let (path, path_span) = self
            .parse_path("a module path")
            .unwrap_or_else(|| (Vec::new(), start));
        let alias = if self.eat_keyword(Keyword::As) {
            self.expect_name("`as`")
        } else {
            None
        };
        let end = alias.as_ref().map_or(path_span, |_| self.prev_span());
        self.end_statement();
        UseDecl {
            span: start.to(end),
            path,
            alias,
        }
    }

    /// Parses `const NAME [: T] = expr`.
    pub(crate) fn parse_constant(&mut self) -> ConstDecl {
        let start = self.span();
        self.bump();
        let name = self.expect_name("`const`").unwrap_or_default();
        let ty = self.parse_optional_type();
        self.expect(TokenKind::Eq, "`=`");
        let value = self.parse_expr();
        let span = start.to(value.span());
        self.end_statement();
        ConstDecl {
            span,
            name,
            ty,
            value,
        }
    }

    /// Parses `default NAME [: T] = expr`.
    pub(crate) fn parse_default(&mut self) -> DefaultDecl {
        let start = self.span();
        self.bump();
        let name = self.expect_name("`default`").unwrap_or_default();
        let ty = self.parse_optional_type();
        self.expect(TokenKind::Eq, "`=`");
        let value = self.parse_expr();
        let span = start.to(value.span());
        self.end_statement();
        DefaultDecl {
            span,
            name,
            ty,
            value,
        }
    }

    /// Consumes a `:` and an indented block that may legitimately be empty.
    ///
    /// Only an enum uses this. Every other declaration needs a body, but "this enum has no
    /// variants" is a *type* error (`E3001`) rather than a syntax one — reporting a missing
    /// block would name the wrong problem, and would make `E3001` unreachable.
    pub(crate) fn enter_optional_block(&mut self) -> bool {
        if self.expect(TokenKind::Colon, "`:`").is_none() {
            return false;
        }
        self.eat(TokenKind::Newline);
        self.eat(TokenKind::Indent)
    }
}
