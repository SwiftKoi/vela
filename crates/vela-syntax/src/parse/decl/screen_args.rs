//! Parsing the words after a name, and the one ambiguity the screen grammar is built on.
//!
//! Its own file rather than part of `screen.rs` because it answers a different question: that file
//! parses *lines*, and this one parses what follows a name on one. The two lookaheads here — is this
//! word a value or a prop's name — are the whole of `SCREENS.md §2`'s "a widget and a prop look the
//! same" problem, and keeping them together is what lets a reader see that they are one question asked
//! from two sides.

use crate::lex::token::{Keyword, TokenKind};
use crate::parse::parser::Parser;
use crate::tree::{Expr, ScreenArg};

impl Parser<'_> {
    /// The words and values after a name: `at bottom, stretch_x`, `gap 8`, `style = body`.
    ///
    /// A value with no name comes first when a leaf takes content (`text "hi"`), and a bare
    /// name is a flag (`stretch_x`) — which is why a name's value is optional and why a value
    /// needs no name.
    pub(crate) fn parse_screen_args(&mut self) -> Vec<ScreenArg> {
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
        //
        // A *reserved word* is the same case, and that is `LANGUAGE.md §7.0`: a keyword is special at
        // the start of a line and an ordinary name in expression position, which is what
        // `parse_primary`'s last arm implements. Without this arm the word `jump` in `action
        // jump(start)` was read as the *next argument's name* — `jump` and a parenthesised `start` —
        // because it is not an identifier. Both halves of that are syntactically fine, so nothing
        // complained: the widget ended up holding an action *named by the screen*, which is the shape
        // a caller hands in, and the button did nothing.
        if !matches!(self.peek(), TokenKind::Ident | TokenKind::Keyword(_)) {
            return false;
        }
        let saved = self.pos;
        self.bump();
        // A dotted path is still one value, so step over the whole chain before looking.
        let mut dotted = false;
        while self.at(TokenKind::Dot) {
            dotted = true;
            self.bump();
            self.bump();
        }
        // A call or an index also begins an expression: `action quit()` is a prop whose value
        // is a call, and reading `quit` as the arg's *name* leaves `()` — an empty argument
        // list — being parsed as a parenthesised expression, which then fails at the `)`.
        //
        // And a chain settles it on its own: no prop name contains a dot, so `text option.caption`
        // is the text widget's content however the line ends. Reading `option` as a prop name leaves
        // `.caption` with nowhere to go, which is exactly what it did until a screen could walk data
        // (`SCREENS.md §2.4`) and the sample's menu wrote `textbutton i.caption action i.action`.
        let continues = dotted
            || is_operator(self.peek())
            || matches!(self.peek(), TokenKind::LParen | TokenKind::LBracket);
        self.pos = saved;
        continues
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
