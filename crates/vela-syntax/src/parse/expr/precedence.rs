//! The precedence chain.

use crate::lex::token::{Keyword, TokenKind};
use crate::parse::parser::Parser;
use crate::tree::{BinOp, Expr, UnOp};
impl Parser<'_> {
    /// Parses an expression, including a trailing conditional.
    pub(crate) fn parse_expr(&mut self) -> Expr {
        let expr = self.parse_or();
        if !self.at_keyword(Keyword::If) {
            return expr;
        }

        self.bump();
        let cond = self.parse_expr();
        self.expect_keyword(Keyword::Else, "`else`");
        let else_ = self.parse_expr();
        let span = expr.span().to(else_.span());
        Expr::If {
            span,
            cond: Box::new(cond),
            then_: Box::new(expr),
            else_: Box::new(else_),
        }
    }

    pub(crate) fn parse_or(&mut self) -> Expr {
        let mut lhs = self.parse_coalesce();
        while self.at_keyword(Keyword::Or) {
            self.bump();
            let rhs = self.parse_coalesce();
            lhs = binary(BinOp::Or, lhs, rhs);
        }
        lhs
    }

    pub(crate) fn parse_coalesce(&mut self) -> Expr {
        let lhs = self.parse_and();
        if !self.at(TokenKind::QuestionQuestion) {
            return lhs;
        }
        self.bump();
        // Right-associative, so `a ?? b ?? c` falls back left to right.
        let rhs = self.parse_coalesce();
        binary(BinOp::Coalesce, lhs, rhs)
    }

    pub(crate) fn parse_and(&mut self) -> Expr {
        let mut lhs = self.parse_not();
        while self.at_keyword(Keyword::And) {
            self.bump();
            let rhs = self.parse_not();
            lhs = binary(BinOp::And, lhs, rhs);
        }
        lhs
    }

    pub(crate) fn parse_not(&mut self) -> Expr {
        if !self.at_keyword(Keyword::Not) {
            return self.parse_comparison();
        }
        let start = self.span();
        self.bump();
        let operand = self.parse_not();
        let span = start.to(operand.span());
        Expr::Unary {
            span,
            op: UnOp::Not,
            operand: Box::new(operand),
        }
    }

    pub(crate) fn parse_comparison(&mut self) -> Expr {
        let lhs = self.parse_additive();
        let Some((op, width)) = self.comparison_op() else {
            return lhs;
        };
        for _ in 0..width {
            self.bump();
        }
        let rhs = self.parse_additive();
        binary(op, lhs, rhs)
    }

    /// The comparison operator at the cursor, and how many tokens it spans.
    ///
    /// `is not` and `not in` are two tokens; a leading `not` has already been consumed
    /// by `parse_not`, so a `not` here can only be the second half of `not in`.
    fn comparison_op(&self) -> Option<(BinOp, usize)> {
        let one = |op| Some((op, 1));
        match self.peek() {
            TokenKind::EqEq => one(BinOp::Eq),
            TokenKind::BangEq => one(BinOp::Ne),
            TokenKind::Lt => one(BinOp::Lt),
            TokenKind::Le => one(BinOp::Le),
            TokenKind::Gt => one(BinOp::Gt),
            TokenKind::Ge => one(BinOp::Ge),
            TokenKind::Keyword(Keyword::In) => one(BinOp::In),
            TokenKind::Keyword(Keyword::Is) => {
                if self.peek_at(1) == TokenKind::Keyword(Keyword::Not) {
                    Some((BinOp::IsNot, 2))
                } else {
                    one(BinOp::Is)
                }
            }
            TokenKind::Keyword(Keyword::Not)
                if self.peek_at(1) == TokenKind::Keyword(Keyword::In) =>
            {
                Some((BinOp::NotIn, 2))
            }
            _ => None,
        }
    }

    pub(crate) fn parse_additive(&mut self) -> Expr {
        let mut lhs = self.parse_multiplicative();
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => return lhs,
            };
            self.bump();
            let rhs = self.parse_multiplicative();
            lhs = binary(op, lhs, rhs);
        }
    }

    pub(crate) fn parse_multiplicative(&mut self) -> Expr {
        let mut lhs = self.parse_unary();
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Rem,
                _ => return lhs,
            };
            self.bump();
            let rhs = self.parse_unary();
            lhs = binary(op, lhs, rhs);
        }
    }

    pub(crate) fn parse_unary(&mut self) -> Expr {
        let op = match self.peek() {
            TokenKind::Minus => UnOp::Neg,
            // `!` is not `not`: it binds tighter than every binary operator, which is why it lives
            // in this level of the chain and `not` lives a level above comparison.
            TokenKind::Bang => UnOp::Bang,
            _ => return self.parse_postfix(),
        };
        let start = self.span();
        self.bump();
        let operand = self.parse_unary();
        let span = start.to(operand.span());
        Expr::Unary {
            span,
            op,
            operand: Box::new(operand),
        }
    }
}

/// Builds a binary expression covering both operands.
fn binary(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    let span = lhs.span().to(rhs.span());
    Expr::Binary {
        span,
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }
}
