//! Staging, audio, waiting, and simple statements.

use crate::lex::token::{Keyword, TokenKind};
use crate::parse::parser::Parser;
use crate::tree::{
    AssignOp, AssignStmt, AudioKind, AudioStmt, Expr, ExprStmt, StageKind, StageStmt, Stmt,
    VarStmt, WaitEvent, WaitStmt, WithStmt,
};
impl Parser<'_> {
    /// Parses `scene`, `show`, or `hide`.
    pub(crate) fn parse_stage(&mut self, kind: StageKind) -> Stmt {
        let start = self.span();
        self.bump();
        let (image, _) = self
            .parse_path("an image")
            .unwrap_or_else(|| (Vec::new(), start));

        // `hide` takes neither attributes nor transforms; the grammar allows the others.
        let (attributes, transforms) = if kind == StageKind::Hide {
            (Vec::new(), Vec::new())
        } else {
            (self.parse_attributes(), self.parse_at_clause())
        };
        let transition = self.parse_with_clause();

        let span = start.to(self.prev_span());
        self.end_statement();
        Stmt::Stage(StageStmt {
            span,
            kind,
            image,
            attributes,
            transforms,
            transition,
        })
    }

    /// Parses an `at a, b` clause, if present.
    pub(crate) fn parse_at_clause(&mut self) -> Vec<String> {
        let mut transforms = Vec::new();
        if !self.eat_keyword(Keyword::At) {
            return transforms;
        }

        loop {
            let before = self.pos;
            match self.expect_name("a transform") {
                Some(name) => transforms.push(name),
                None => break,
            }
            if self.pos == before || !self.eat(TokenKind::Comma) {
                break;
            }
        }
        transforms
    }

    /// Parses a standalone `with transition`.
    pub(crate) fn parse_with(&mut self) -> Stmt {
        let start = self.span();
        self.bump();
        let transition = self.expect_name("a transition").unwrap_or_default();
        let span = start.to(self.prev_span());
        self.end_statement();
        Stmt::With(WithStmt { span, transition })
    }

    /// Parses `play`, `stop`, or `queue`.
    pub(crate) fn parse_audio(&mut self, kind: AudioKind) -> Stmt {
        let start = self.span();
        self.bump();
        let channel = self.expect_name("a channel").unwrap_or_default();
        let source = if kind == AudioKind::Stop {
            None
        } else {
            Some(self.parse_expr())
        };

        let mut looping = false;
        let mut fade = None;
        loop {
            if self.at_word("loop") {
                self.bump();
                looping = true;
            } else if self.at_word("fade") {
                self.bump();
                fade = Some(self.parse_expr());
            } else {
                break;
            }
        }

        let span = start.to(self.prev_span());
        self.end_statement();
        Stmt::Audio(AudioStmt {
            span,
            kind,
            channel,
            source,
            looping,
            fade,
        })
    }

    /// Parses `pause [duration]`.
    pub(crate) fn parse_pause(&mut self) -> Stmt {
        let start = self.span();
        self.bump();
        let event = if self.ends_statement() {
            WaitEvent::Click
        } else {
            WaitEvent::Duration(self.parse_expr())
        };
        let span = start.to(self.prev_span());
        self.end_statement();
        Stmt::Wait(WaitStmt { span, event })
    }

    /// Parses `wait click` or `wait duration`.
    pub(crate) fn parse_wait(&mut self) -> Stmt {
        let start = self.span();
        self.bump();
        let event = if self.at_word("click") {
            self.bump();
            WaitEvent::Click
        } else {
            WaitEvent::Duration(self.parse_expr())
        };
        let span = start.to(self.prev_span());
        self.end_statement();
        Stmt::Wait(WaitStmt { span, event })
    }

    /// Parses `var name [: T] = expr`.
    pub(crate) fn parse_var(&mut self) -> Stmt {
        let start = self.span();
        self.bump();
        let name = self.expect_name("`var`").unwrap_or_default();
        let ty = if self.at(TokenKind::Colon) {
            self.bump();
            Some(self.parse_type())
        } else {
            None
        };
        self.expect(TokenKind::Eq, "`=`");
        let value = self.parse_expr();
        let span = start.to(value.span());
        self.end_statement();
        Stmt::Var(VarStmt {
            span,
            name,
            ty,
            value,
        })
    }

    /// Parses an assignment, or an expression evaluated for its effect.
    pub(crate) fn parse_expression_statement(&mut self) -> Stmt {
        let start = self.span();
        let target = self.parse_expr();

        // If not even a primary expression could be parsed, the rest of the line is
        // noise. Skipping to the statement boundary is what keeps one bad line from
        // producing a diagnostic per token.
        if matches!(target, Expr::Error { .. }) {
            self.recover_statement();
            return Stmt::Error { span: start };
        }

        let op = match self.peek() {
            TokenKind::Eq => Some(AssignOp::Assign),
            TokenKind::PlusEq => Some(AssignOp::Add),
            TokenKind::MinusEq => Some(AssignOp::Sub),
            TokenKind::StarEq => Some(AssignOp::Mul),
            TokenKind::SlashEq => Some(AssignOp::Div),
            _ => None,
        };

        let stmt = match op {
            Some(op) => {
                self.bump();
                let value = self.parse_expr();
                let span = start.to(value.span());
                Stmt::Assign(AssignStmt {
                    span,
                    target,
                    op,
                    value,
                })
            }
            None => Stmt::Expr(ExprStmt {
                span: target.span(),
                expr: target,
            }),
        };

        self.end_statement();
        stmt
    }
}
