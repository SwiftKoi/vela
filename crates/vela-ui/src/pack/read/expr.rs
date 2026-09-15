//! Reading expressions back.
//!
//! Expressions are the only recursive shape in the format, so this is the one place the reader
//! needs a depth guard: a crafted pack could otherwise nest a million lambdas and take the stack
//! down with it. Each variant is its own small function rather than one long match, so a tag and
//! its fields sit together and a missing field is visible where it is read.

use vela_syntax::{BinOp, Expr, StrPart, UnOp};

use super::cursor::Reader;

impl Reader<'_> {
    /// An expression, depth-limited.
    pub(super) fn expr(&mut self) -> Expr {
        if !self.descend() {
            return Expr::Error { span: self.span() };
        }
        let value = self.expr_at_depth();
        self.ascend();
        value
    }

    /// One expression, by its tag.
    fn expr_at_depth(&mut self) -> Expr {
        match self.u8() {
            0 => self.expr_int(),
            1 => self.expr_float(),
            2 => self.expr_path(),
            3 => self.expr_bool(),
            4 => Expr::None { span: self.span() },
            5 => self.expr_name(),
            6 => self.expr_str(),
            7 => self.expr_list(),
            8 => self.expr_map(),
            9 => self.expr_field(),
            10 => self.expr_call(),
            11 => self.expr_index(),
            12 => self.expr_unary(),
            13 => self.expr_binary(),
            14 => self.expr_paren(),
            15 => self.expr_if(),
            16 => self.expr_lambda(),
            _ => Expr::Error { span: self.span() },
        }
    }

    fn expr_int(&mut self) -> Expr {
        let span = self.span();
        let value = self.i64();
        // The radix is not stored: a renderer needs the value, and the canonical spelling of a
        // literal is the source formatter's business, not a compiled pack's.
        Expr::Int {
            span,
            value,
            hex: false,
        }
    }

    fn expr_float(&mut self) -> Expr {
        let span = self.span();
        let value = self.f64();
        Expr::Float { span, value }
    }

    fn expr_path(&mut self) -> Expr {
        let span = self.span();
        let value = self.string();
        Expr::Path { span, value }
    }

    fn expr_bool(&mut self) -> Expr {
        let span = self.span();
        let value = self.flag();
        Expr::Bool { span, value }
    }

    fn expr_name(&mut self) -> Expr {
        let span = self.span();
        let name = self.string();
        Expr::Name { span, name }
    }

    fn expr_str(&mut self) -> Expr {
        let span = self.span();
        let parts = self.str_parts();
        Expr::Str { span, parts }
    }

    fn expr_list(&mut self) -> Expr {
        let span = self.span();
        let count = self.count();
        let mut items = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            items.push(self.expr());
        }
        Expr::List { span, items }
    }

    fn expr_map(&mut self) -> Expr {
        let span = self.span();
        let count = self.count();
        let mut entries = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            let key = self.expr();
            let value = self.expr();
            entries.push((key, value));
        }
        Expr::Map { span, entries }
    }

    fn expr_field(&mut self) -> Expr {
        let span = self.span();
        let base = Box::new(self.expr());
        let name = self.string();
        Expr::Field { span, base, name }
    }

    fn expr_call(&mut self) -> Expr {
        let span = self.span();
        let callee = Box::new(self.expr());
        let count = self.count();
        let mut args = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            args.push(self.expr());
        }
        Expr::Call { span, callee, args }
    }

    fn expr_index(&mut self) -> Expr {
        let span = self.span();
        let base = Box::new(self.expr());
        let index = Box::new(self.expr());
        Expr::Index { span, base, index }
    }

    fn expr_unary(&mut self) -> Expr {
        let span = self.span();
        let op = self.un_op();
        let operand = Box::new(self.expr());
        Expr::Unary { span, op, operand }
    }

    fn expr_binary(&mut self) -> Expr {
        let span = self.span();
        let op = self.bin_op();
        let lhs = Box::new(self.expr());
        let rhs = Box::new(self.expr());
        Expr::Binary { span, op, lhs, rhs }
    }

    fn expr_paren(&mut self) -> Expr {
        let span = self.span();
        let inner = Box::new(self.expr());
        Expr::Paren { span, inner }
    }

    fn expr_if(&mut self) -> Expr {
        let span = self.span();
        let cond = Box::new(self.expr());
        let then_ = Box::new(self.expr());
        let else_ = Box::new(self.expr());
        Expr::If {
            span,
            cond,
            then_,
            else_,
        }
    }

    fn expr_lambda(&mut self) -> Expr {
        let span = self.span();
        let params = self.params();
        let body = Box::new(self.expr());
        Expr::Lambda { span, params, body }
    }

    fn str_parts(&mut self) -> Vec<StrPart> {
        let count = self.count();
        let mut parts = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            parts.push(match self.u8() {
                0 => {
                    let span = self.span();
                    let text = self.string();
                    StrPart::Literal { span, text }
                }
                _ => {
                    let span = self.span();
                    let expr = Box::new(self.expr());
                    StrPart::Interpolation { span, expr }
                }
            });
        }
        parts
    }

    fn un_op(&mut self) -> UnOp {
        match self.u8() {
            0 => UnOp::Neg,
            2 => UnOp::Bang,
            // Anything else, including 1, is `not`: the reader stays total, and a tag this build
            // does not know is a pack whose version was already refused before reading started.
            _ => UnOp::Not,
        }
    }

    fn bin_op(&mut self) -> BinOp {
        match self.u8() {
            0 => BinOp::Add,
            1 => BinOp::Sub,
            2 => BinOp::Mul,
            3 => BinOp::Div,
            4 => BinOp::Rem,
            5 => BinOp::Eq,
            6 => BinOp::Ne,
            7 => BinOp::Lt,
            8 => BinOp::Le,
            9 => BinOp::Gt,
            10 => BinOp::Ge,
            11 => BinOp::And,
            12 => BinOp::Or,
            13 => BinOp::Is,
            14 => BinOp::IsNot,
            15 => BinOp::In,
            16 => BinOp::NotIn,
            _ => BinOp::Coalesce,
        }
    }
}
