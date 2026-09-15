//! Writing expressions.
//!
//! One function per variant, so a tag and the fields that follow it sit together and it is visible
//! when one is missed. The tags are the reader's, in `read/expr.rs` — the two are one contract
//! written twice, and a mismatch is what the codec's own tests exist to catch.

use vela_span::Span;
use vela_syntax::{BinOp, Expr, StrPart, UnOp};

use super::codec::{Writer, count};

impl Writer {
    pub(super) fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Int { span, value } => self.number(*span, *value),
            Expr::Float { span, value } => self.decimal(*span, *value),
            Expr::Path { span, value } => self.leaf(2, *span, value),
            Expr::Bool { span, value } => self.boolean(*span, *value),
            Expr::None { span } => self.marked(4, *span),
            Expr::Name { span, name } => self.leaf(5, *span, name),
            Expr::Str { span, parts } => self.text_expr(*span, parts),
            Expr::List { span, items } => self.list(*span, items),
            Expr::Map { span, entries } => self.map(*span, entries),
            Expr::Field { span, base, name } => self.field(*span, base, name),
            Expr::Call { span, callee, args } => self.call(*span, callee, args),
            Expr::Index { span, base, index } => self.index(*span, base, index),
            Expr::Unary { span, op, operand } => self.unary(*span, *op, operand),
            Expr::Binary { span, op, lhs, rhs } => self.binary(*span, *op, lhs, rhs),
            Expr::Paren { span, inner } => self.paren(*span, inner),
            Expr::If {
                span,
                cond,
                then_,
                else_,
            } => self.conditional(*span, cond, then_, else_),
            Expr::Lambda { span, params, body } => self.lambda(*span, params, body),
            Expr::Error { span } => self.marked(17, *span),
        }
    }

    /// A variant that is only a tag and a span.
    fn marked(&mut self, tag: u8, span: Span) {
        self.u8(tag);
        self.span(span);
    }

    fn number(&mut self, span: Span, value: i64) {
        self.u8(0);
        self.span(span);
        self.i64(value);
    }

    fn decimal(&mut self, span: Span, value: f64) {
        self.u8(1);
        self.span(span);
        self.f64(value);
    }

    fn boolean(&mut self, span: Span, value: bool) {
        self.u8(3);
        self.span(span);
        self.flag(value);
    }

    /// A variant that is a tag, a span, and one string.
    fn leaf(&mut self, tag: u8, span: Span, text: &str) {
        self.marked(tag, span);
        self.string(text);
    }

    fn text_expr(&mut self, span: Span, parts: &[StrPart]) {
        self.marked(6, span);
        self.str_parts(parts);
    }

    fn list(&mut self, span: Span, items: &[Expr]) {
        self.marked(7, span);
        self.exprs(items);
    }

    fn map(&mut self, span: Span, entries: &[(Expr, Expr)]) {
        self.marked(8, span);
        self.u32(count(entries.len()));
        for (key, value) in entries {
            self.expr(key);
            self.expr(value);
        }
    }

    fn field(&mut self, span: Span, base: &Expr, name: &str) {
        self.marked(9, span);
        self.expr(base);
        self.string(name);
    }

    fn call(&mut self, span: Span, callee: &Expr, args: &[Expr]) {
        self.marked(10, span);
        self.expr(callee);
        self.exprs(args);
    }

    fn index(&mut self, span: Span, base: &Expr, index: &Expr) {
        self.marked(11, span);
        self.expr(base);
        self.expr(index);
    }

    fn unary(&mut self, span: Span, op: UnOp, operand: &Expr) {
        self.marked(12, span);
        self.un_op(op);
        self.expr(operand);
    }

    fn binary(&mut self, span: Span, op: BinOp, lhs: &Expr, rhs: &Expr) {
        self.marked(13, span);
        self.bin_op(op);
        self.expr(lhs);
        self.expr(rhs);
    }

    fn paren(&mut self, span: Span, inner: &Expr) {
        self.marked(14, span);
        self.expr(inner);
    }

    fn conditional(&mut self, span: Span, cond: &Expr, then_: &Expr, else_: &Expr) {
        self.marked(15, span);
        self.expr(cond);
        self.expr(then_);
        self.expr(else_);
    }

    fn lambda(&mut self, span: Span, params: &[vela_syntax::Param], body: &Expr) {
        self.marked(16, span);
        self.params(params);
        self.expr(body);
    }

    fn exprs(&mut self, exprs: &[Expr]) {
        self.u32(count(exprs.len()));
        for expr in exprs {
            self.expr(expr);
        }
    }

    fn str_parts(&mut self, parts: &[StrPart]) {
        self.u32(count(parts.len()));
        for part in parts {
            match part {
                StrPart::Literal { span, text } => self.leaf(0, *span, text),
                StrPart::Interpolation { span, expr } => {
                    self.marked(1, *span);
                    self.expr(expr);
                }
            }
        }
    }

    fn un_op(&mut self, op: UnOp) {
        self.u8(match op {
            UnOp::Neg => 0,
            UnOp::Not => 1,
        });
    }

    fn bin_op(&mut self, op: BinOp) {
        self.u8(match op {
            BinOp::Add => 0,
            BinOp::Sub => 1,
            BinOp::Mul => 2,
            BinOp::Div => 3,
            BinOp::Rem => 4,
            BinOp::Eq => 5,
            BinOp::Ne => 6,
            BinOp::Lt => 7,
            BinOp::Le => 8,
            BinOp::Gt => 9,
            BinOp::Ge => 10,
            BinOp::And => 11,
            BinOp::Or => 12,
            BinOp::Is => 13,
            BinOp::IsNot => 14,
            BinOp::In => 15,
            BinOp::NotIn => 16,
            BinOp::Coalesce => 17,
        });
    }
}
