//! Parsing a `screen` body into a widget tree.
//!
//! `SCREENS.md §2`. The body is a tree by indentation, like every other block in the language,
//! with one wrinkle: a line is a *name followed by words*, and whether it names a widget or a
//! prop is not a question the parser can answer. See `tree/screen.rs` for why that is fine.

use crate::lex::token::{Keyword, TokenKind};
use crate::parse::parser::Parser;
use crate::tree::{Expr, ScreenArg, ScreenElif, ScreenLine, ScreenNode};

impl Parser<'_> {
    /// Parses the indented body of a `screen`.
    pub(crate) fn parse_screen_body(&mut self, construct: &str) -> Vec<ScreenLine> {
        let mut lines = Vec::new();
        if !self.enter_block(construct) {
            return lines;
        }

        while !self.at(TokenKind::Dedent) && !self.at_eof() {
            let before = self.pos;
            if let Some(line) = self.parse_screen_line() {
                lines.push(line);
            }
            // Same guarantee as every other block loop: a parser that cannot make progress
            // would spin here forever, so forward motion does not depend on its correctness.
            if self.pos == before {
                self.bump();
            }
        }
        self.eat(TokenKind::Dedent);
        lines
    }

    /// Parses one line of a screen body.
    fn parse_screen_line(&mut self) -> Option<ScreenLine> {
        let start = self.span();

        if self.at(TokenKind::Keyword(Keyword::If)) {
            return self.parse_screen_if(start);
        }
        // `use` is a keyword, and the only position where it is not a module import is here.
        if self.at(TokenKind::Keyword(Keyword::Use)) {
            return self.parse_screen_use(start);
        }
        if self.at_keyword_word("layer") {
            return self.parse_screen_layer(start);
        }
        // Contextual, like `layer`: a word only special in this position.
        if self.at_keyword_word("style_prefix") {
            return self.parse_style_prefix(start);
        }
        // Composition's other half: where a caller's block lands. Contextual like `layer` — the
        // word is only special in this position, so a project keeps it as a name everywhere else.
        if self.at_keyword_word("transclude") {
            self.bump();
            self.end_statement();
            return Some(ScreenLine::Transclude { span: start });
        }
        // Contextual too, and for the same reason: `key` and `timer` are ordinary words a widget
        // could be called, and only a line that starts with one is an input binding.
        if self.at_keyword_word("key") {
            return self.parse_screen_key(start);
        }
        if self.at_keyword_word("timer") {
            return self.parse_screen_timer(start);
        }
        // `pass` is the language's own empty statement, and a screen needs *something* indented
        // to have a body. Read as a widget it would be a line named `pass` and reported as an
        // unknown widget — an error about a screen that is doing exactly the right thing.
        if self.at_keyword_word("pass") {
            self.bump();
            self.end_statement();
            return None;
        }
        self.parse_screen_node(start).map(ScreenLine::Node)
    }

    /// `if <condition>:` and its body, then any `elif` arms and an `else`.
    ///
    /// The chain is parsed into one line rather than into sibling lines, which is the shape the
    /// statement form already has (`LANGUAGE.md §3`). `elif` and `else` are reserved words, so this
    /// loop is the only place a screen body consumes them: written anywhere else they are a name the
    /// widget vocabulary does not know, which is the honest report for a conditional with no `if`.
    fn parse_screen_if(&mut self, start: vela_span::Span) -> Option<ScreenLine> {
        self.bump();
        let condition = self.parse_expr();
        let body = self.parse_screen_body("`if`");

        let mut elifs = Vec::new();
        while self.at(TokenKind::Keyword(Keyword::Elif)) {
            let elif_start = self.span();
            self.bump();
            let elif_condition = self.parse_expr();
            let elif_body = self.parse_screen_body("`elif`");
            elifs.push(ScreenElif {
                span: elif_start.to(self.prev_span()),
                condition: elif_condition,
                body: elif_body,
            });
        }

        let else_body = if self.at(TokenKind::Keyword(Keyword::Else)) {
            self.bump();
            Some(self.parse_screen_body("`else`"))
        } else {
            None
        };

        Some(ScreenLine::If {
            span: start.to(self.prev_span()),
            condition,
            body,
            elifs,
            else_body,
        })
    }

    /// `style_prefix <name>`.
    ///
    /// A *name*, not a string: Vela names a style the way it names anything else (`style = body`), and
    /// the migrator's job is to drop Ren'Py's quotes — the same trade every other line here makes.
    fn parse_style_prefix(&mut self, start: vela_span::Span) -> Option<ScreenLine> {
        self.bump();
        let name = self.expect_name("a style prefix").unwrap_or_default();
        self.end_statement();
        Some(ScreenLine::StylePrefix {
            span: start.to(self.prev_span()),
            name,
        })
    }

    /// `layer <name>`.
    ///
    /// `layer` is not a keyword — it is only special in this position, and reserving it would
    /// stop a project calling a variable `layer` for no gain.
    fn parse_screen_layer(&mut self, start: vela_span::Span) -> Option<ScreenLine> {
        self.bump();
        let name = self.expect_name("a layer name").unwrap_or_default();
        self.end_statement();
        Some(ScreenLine::Layer { span: start, name })
    }

    /// A widget or a bare prop: `name [props] [:]`.
    fn parse_screen_node(&mut self, start: vela_span::Span) -> Option<ScreenNode> {
        // A widget name may be a reserved word: `image`, `bar`, and `input` are all in the
        // registry and all keywords elsewhere. Refusing them would make the registry's own
        // default set unwritable — the same problem `audio.play` had as an effect name.
        let name = self.screen_prop_name()?;
        let args = self.parse_screen_args();

        let children = if self.at(TokenKind::Colon) {
            self.parse_screen_body(&format!("`{name}`"))
        } else {
            self.end_statement();
            Vec::new()
        };

        Some(ScreenNode {
            span: start.to(self.prev_span()),
            name,
            args,
            children,
        })
    }

    /// The words and values after a name: `at bottom, stretch_x`, `gap 8`, `style = body`.
    ///
    /// A value with no name comes first when a leaf takes content (`text "hi"`), and a bare
    /// name is a flag (`stretch_x`) — which is why a name's value is optional and why a value
    /// needs no name.
    fn parse_screen_args(&mut self) -> Vec<ScreenArg> {
        let mut args = Vec::new();

        while !self.at(TokenKind::Newline)
            && !self.at(TokenKind::Colon)
            && !self.at(TokenKind::Dedent)
            && !self.at_eof()
        {
            let before = self.pos;
            let start = self.span();

            // A bare value: a string, a number, a parenthesised expression.
            if self.starts_bare_value() {
                let value = self.parse_expr();
                args.push(ScreenArg::Value(value));
            } else {
                let Some(name) = self.screen_prop_name() else {
                    break;
                };
                // `= value` is explicit; a following value is implicit. Either way the value
                // is the same parse, so they are one condition rather than two identical
                // branches — and the short circuit matters: `eat` consumes the `=`.
                let value = if self.eat(TokenKind::Eq)
                    || (self.starts_expression() && !self.next_is_another_name())
                {
                    Some(self.parse_expr())
                } else {
                    None
                };
                let end = value.as_ref().map_or(self.prev_span(), Expr::span);
                args.push(ScreenArg::Named {
                    span: start.to(end),
                    name,
                    value,
                });
            }

            // A comma is an optional separator, not a terminator: `box at bottom,
            // stretch_x` uses one and `text line style = body` does not. Requiring it made
            // the second shape parse its first arg and then stall on the rest.
            self.eat(TokenKind::Comma);
            if self.pos == before {
                self.bump();
                break;
            }
        }
        args
    }

    /// Whether the token after the next one is `=`, which makes the next one a name.
    ///
    /// `text line style = body` is the case that forces this. `line` is the text widget's
    /// content and `style = body` is a named prop, so taking `style` as `line`'s value would
    /// leave `= body` unparsed. Looking one token further is enough to tell them apart: a name
    /// is followed by `=`, and a value is not.
    pub(crate) fn next_is_another_name(&mut self) -> bool {
        let saved = self.pos;
        self.bump();
        let followed_by_eq = self.at(TokenKind::Eq);
        self.pos = saved;
        followed_by_eq
    }

    /// Whether the next token is plainly a value rather than a name.
    ///
    /// Deliberately narrow: a bare identifier is a *name* here, because `at bottom` and
    /// `text name` are indistinguishable without the widget's schema, and the checker is the
    /// party that has it.
    fn starts_bare_value(&mut self) -> bool {
        if matches!(
            self.peek(),
            TokenKind::Str
                | TokenKind::Int
                | TokenKind::Float
                | TokenKind::Minus
                | TokenKind::LParen
        ) {
            return true;
        }
        // An identifier followed by an operator begins an expression, not a name.
        //
        // `enable_if trust > 3` is one prop with one value, and reading `trust` as an arg's
        // *name* leaves `> 3` with nowhere to go. This is the same one-token lookahead as
        // `next_is_another_name`, pointing the other way: a name is followed by `=`, a value
        // is followed by an operator.
        if !matches!(self.peek(), TokenKind::Ident) {
            return false;
        }
        let saved = self.pos;
        self.bump();
        // A dotted path is still one value, so step over the whole chain before looking.
        while self.at(TokenKind::Dot) {
            self.bump();
            self.bump();
        }
        // A call or an index also begins an expression: `action quit()` is a prop whose value
        // is a call, and reading `quit` as the arg's *name* leaves `()` — an empty argument
        // list — being parsed as a parenthesised expression, which then fails at the `)`.
        let continues = is_operator(self.peek())
            || matches!(self.peek(), TokenKind::LParen | TokenKind::LBracket);
        self.pos = saved;
        continues
    }

    /// A prop's name, which may be a reserved word — `at`, `size`, and `min` are all props and
    /// several of them read as keywords in other positions.
    pub(crate) fn screen_prop_name(&mut self) -> Option<String> {
        if matches!(self.peek(), TokenKind::Ident) {
            return self.expect_name("a prop");
        }
        let token = self.current();
        if matches!(token.kind, TokenKind::Keyword(_)) {
            let text = self.text(token).to_string();
            self.bump();
            return Some(text);
        }
        self.expect_name("a prop")
    }

    /// Whether the next token could begin a prop's value.
    fn starts_expression(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Ident
                | TokenKind::Int
                | TokenKind::Float
                | TokenKind::Str
                | TokenKind::Minus
                | TokenKind::LParen
                | TokenKind::Keyword(Keyword::True | Keyword::False | Keyword::None)
        )
    }

    /// Whether the current token is the bare word `word`.
    fn at_keyword_word(&self, word: &str) -> bool {
        matches!(self.peek(), TokenKind::Ident) && self.text(self.current()) == word
    }
}

/// Whether a token continues an expression rather than starting a new word.
fn is_operator(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::EqEq
            | TokenKind::BangEq
            | TokenKind::Lt
            | TokenKind::Le
            | TokenKind::Gt
            | TokenKind::Ge
            | TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::Question
    )
}
