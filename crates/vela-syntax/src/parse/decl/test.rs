//! Parsing a `test` item.

use crate::error;
use crate::lex::token::{Keyword, TokenKind};
use crate::parse::parser::Parser;
use crate::tree::{CoverMode, Directive, DirectiveKind, TestDecl};

impl Parser<'_> {
    /// Parses a `test` item: a name, and a block of directives.
    pub(crate) fn parse_test(&mut self) -> TestDecl {
        let start = self.span();
        self.bump();

        let name = self.parse_test_name();
        let directives = self.parse_directives();

        TestDecl {
            span: start.to(self.prev_span()),
            name,
            directives,
        }
    }

    /// The test's name, which is a string.
    ///
    /// A sentence rather than an identifier, because it is what a report prints. Kept as text rather
    /// than an expression: a name that depended on the world would be a name a report could not print
    /// before anything had run.
    fn parse_test_name(&mut self) -> String {
        let Some(token) = self.expect(TokenKind::Str, "a name for the test, in quotes") else {
            return String::new();
        };

        self.text(token).trim_matches('"').to_string()
    }

    /// The block of directives, one per line.
    fn parse_directives(&mut self) -> Vec<Directive> {
        let mut directives = Vec::new();
        if !self.enter_block("`test`") {
            return directives;
        }

        while !self.at(TokenKind::Dedent) && !self.at_eof() {
            let before = self.pos;
            if let Some(directive) = self.parse_directive() {
                directives.push(directive);
            }
            // The same guarantee `parse_block` makes: a directive that cannot make progress must not
            // spin here forever.
            if self.pos == before {
                self.bump();
            }
        }
        self.eat(TokenKind::Dedent);

        directives
    }

    /// One directive: a name in the first position, and its argument.
    fn parse_directive(&mut self) -> Option<Directive> {
        let start = self.span();
        let name = self.expect_name("a directive")?;

        let kind = match name.as_str() {
            // `run from <label>`, and `run` alone — which starts from the entry label, the same place
            // the game does.
            "run" => {
                let (target, target_span) = if self.eat_keyword(Keyword::From) {
                    match self.parse_path("a label to run from") {
                        Some((path, span)) => (path, span),
                        None => (Vec::new(), self.prev_span()),
                    }
                } else {
                    (Vec::new(), start)
                };
                DirectiveKind::Run {
                    target,
                    target_span,
                }
            }
            "advance" => DirectiveKind::Advance {
                count: self.parse_count(),
            },
            "choose" => DirectiveKind::Choose {
                text: self.parse_expr(),
            },
            "expect" => DirectiveKind::Expect {
                expr: self.parse_expr(),
            },
            "cover" => DirectiveKind::Cover {
                mode: self.parse_cover_mode(),
            },
            // An unknown directive is an error rather than a line to skip quietly: a line inside a test
            // that the runner does not understand would be a line that checks nothing, and a test that
            // silently checks less than it says is worse than one that does not parse.
            _ => {
                self.diagnostics.push(error::unexpected(
                    start.to(self.prev_span()),
                    &format!("`{name}`"),
                    "one of `run`, `advance`, `choose`, `expect`, or `cover`",
                ));
                self.skip_line();
                self.end_statement();
                return None;
            }
        };

        // The line ends where the directive does; the loop above is what reads the next one.
        self.end_statement();

        Some(Directive {
            span: start.to(self.prev_span()),
            kind,
        })
    }

    /// The count an `advance` carries.
    fn parse_count(&mut self) -> u32 {
        let Some(token) = self.expect(TokenKind::Int, "how many commands to advance") else {
            return 1;
        };

        self.text(token).parse().unwrap_or(1)
    }

    /// A `cover` directive's argument.
    fn parse_cover_mode(&mut self) -> CoverMode {
        let start = self.span();
        let name = self
            .expect_name("`labels` or `variants`")
            .unwrap_or_default();

        match CoverMode::lookup(&name) {
            Some(mode) => mode,
            None => {
                self.diagnostics.push(error::unexpected(
                    start.to(self.prev_span()),
                    &format!("`{name}`"),
                    "`labels` or `variants`",
                ));
                CoverMode::Labels
            }
        }
    }

    /// Skips the rest of the line, so one bad directive does not become a cascade of them.
    fn skip_line(&mut self) {
        while !self.at(TokenKind::Newline) && !self.at(TokenKind::Dedent) && !self.at_eof() {
            self.bump();
        }
    }
}
