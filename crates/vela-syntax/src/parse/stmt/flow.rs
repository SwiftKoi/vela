//! Control flow within a label or function body.

use crate::lex::token::{Keyword, TokenKind};
use crate::parse::parser::Parser;
use crate::tree::{
    CallStmt, ElifClause, ForStmt, IfStmt, JumpStmt, MatchArm, MatchStmt, ReturnStmt, Stmt,
    WhileStmt,
};
impl Parser<'_> {
    /// Parses `jump target`.
    pub(crate) fn parse_jump(&mut self) -> Stmt {
        let start = self.span();
        self.bump();
        let (target, target_span) = self
            .parse_path("a label")
            .unwrap_or_else(|| (Vec::new(), start));
        let span = start.to(self.prev_span());
        self.end_statement();
        Stmt::Jump(JumpStmt {
            span,
            target,
            target_span,
        })
    }

    /// Parses `call target [with transition]`.
    pub(crate) fn parse_call(&mut self) -> Stmt {
        let start = self.span();
        self.bump();
        let (target, target_span) = self
            .parse_path("a label")
            .unwrap_or_else(|| (Vec::new(), start));
        let transition = self.parse_with_clause();
        let span = start.to(self.prev_span());
        self.end_statement();
        Stmt::Call(CallStmt {
            span,
            target,
            target_span,
            transition,
        })
    }

    /// Parses `return [expr]`.
    pub(crate) fn parse_return(&mut self) -> Stmt {
        let start = self.span();
        self.bump();
        let value = if self.ends_statement() {
            None
        } else {
            Some(self.parse_expr())
        };
        let span = start.to(self.prev_span());
        self.end_statement();
        Stmt::Return(ReturnStmt { span, value })
    }

    /// Parses `if`/`elif`/`else`.
    pub(crate) fn parse_if(&mut self) -> Stmt {
        let start = self.span();
        self.bump();
        let condition = self.parse_expr();
        let then_body = self.parse_block("`if`");

        let mut elifs = Vec::new();
        while self.at_keyword(Keyword::Elif) {
            let elif_start = self.span();
            self.bump();
            let elif_condition = self.parse_expr();
            let body = self.parse_block("`elif`");
            elifs.push(ElifClause {
                span: elif_start.to(self.prev_span()),
                condition: elif_condition,
                body,
            });
        }

        let else_body = if self.at_keyword(Keyword::Else) {
            self.bump();
            Some(self.parse_block("`else`"))
        } else {
            None
        };

        Stmt::If(IfStmt {
            span: start.to(self.prev_span()),
            condition,
            then_body,
            elifs,
            else_body,
        })
    }

    /// Parses `while`.
    pub(crate) fn parse_while(&mut self) -> Stmt {
        let start = self.span();
        self.bump();
        let condition = self.parse_expr();
        let body = self.parse_block("`while`");
        Stmt::While(WhileStmt {
            span: start.to(self.prev_span()),
            condition,
            body,
        })
    }

    /// Parses `for name in iterable`.
    pub(crate) fn parse_for(&mut self) -> Stmt {
        let start = self.span();
        self.bump();
        let binding = self.expect_name("`for`").unwrap_or_default();
        self.expect_keyword(Keyword::In, "`in`");
        let iterable = self.parse_expr();
        let body = self.parse_block("`for`");
        Stmt::For(ForStmt {
            span: start.to(self.prev_span()),
            binding,
            iterable,
            body,
        })
    }

    /// Parses `match` and its `when`/`else` arms.
    pub(crate) fn parse_match(&mut self) -> Stmt {
        let start = self.span();
        self.bump();
        let scrutinee = self.parse_expr();
        let mut arms = Vec::new();

        if self.enter_block("`match`") {
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                let before = self.pos;
                let arm_start = self.span();

                if self.at_keyword(Keyword::When) {
                    self.bump();
                    let pattern = self.parse_pattern();
                    let guard = if self.eat_keyword(Keyword::If) {
                        Some(self.parse_expr())
                    } else {
                        None
                    };
                    let body = self.parse_block("`when`");
                    arms.push(MatchArm {
                        span: arm_start.to(self.prev_span()),
                        pattern: Some(pattern),
                        guard,
                        body,
                    });
                } else if self.at_keyword(Keyword::Else) {
                    self.bump();
                    let body = self.parse_block("`else`");
                    arms.push(MatchArm {
                        span: arm_start.to(self.prev_span()),
                        pattern: None,
                        guard: None,
                        body,
                    });
                }

                if self.pos == before {
                    self.bump();
                }
            }
            self.eat(TokenKind::Dedent);
        }

        Stmt::Match(MatchStmt {
            span: start.to(self.prev_span()),
            scrutinee,
            arms,
        })
    }
}
