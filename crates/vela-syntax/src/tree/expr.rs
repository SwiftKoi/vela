//! Expressions.

use vela_span::Span;

use crate::tree::Param;

/// A binary operator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BinOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Rem,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `&&` written `and`
    And,
    /// `||` written `or`
    Or,
    /// `is`
    Is,
    /// `is not`
    IsNot,
    /// `in`
    In,
    /// `not in`
    NotIn,
    /// `??`, the optional fallback.
    Coalesce,
}

/// A prefix operator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnOp {
    /// `-`
    Neg,
    /// `!`
    Not,
}

/// One piece of a string literal.
///
/// `"Score: {score}"` is a literal part, then an interpolation. Keeping the split in the
/// tree rather than desugaring during parsing means the type checker can report on the
/// interpolated expression's own span.
#[derive(Clone, Debug)]
pub enum StrPart {
    /// Literal text, with escapes already resolved.
    Literal {
        /// Where the text came from.
        span: Span,
        /// The text, escapes resolved.
        text: String,
    },
    /// An interpolated expression, written `{...}`.
    Interpolation {
        /// The span of the whole `{...}`.
        span: Span,
        /// The expression inside the braces.
        expr: Box<Expr>,
    },
}

/// An expression.
#[derive(Clone, Debug)]
pub enum Expr {
    /// An integer literal.
    Int {
        /// The literal's span.
        span: Span,
        /// Its value. `0x` literals are already in base ten here.
        value: i64,
    },
    /// A floating-point literal.
    Float {
        /// The literal's span.
        span: Span,
        /// Its value.
        value: f64,
    },
    /// A path literal, `@"art/forest.png"`.
    Path {
        /// The literal's span.
        span: Span,
        /// The path, with the `@` and the quotes removed.
        value: String,
    },
    /// `true` or `false`.
    Bool {
        /// The literal's span.
        span: Span,
        /// Which of the two.
        value: bool,
    },
    /// `none`.
    None {
        /// The literal's span.
        span: Span,
    },
    /// A bare name: `trust`.
    ///
    /// Dotted names are not stored here. `a.b` is a [`Expr::Field`] on a name, because
    /// that is what it means; contexts that want a *path* (a label reference, an image
    /// name) build one from consecutive fields rather than from a special expression.
    Name {
        /// The name's span.
        span: Span,
        /// The name.
        name: String,
    },
    /// A string literal, split into literal and interpolated parts.
    Str {
        /// The literal's span, including quotes.
        span: Span,
        /// The parts, in order.
        parts: Vec<StrPart>,
    },
    /// `[a, b]`.
    List {
        /// The list's span.
        span: Span,
        /// The elements.
        items: Vec<Expr>,
    },
    /// `{k: v}`.
    Map {
        /// The map's span.
        span: Span,
        /// The entries, in source order.
        entries: Vec<(Expr, Expr)>,
    },
    /// `base.name`.
    Field {
        /// The field access's span.
        span: Span,
        /// What is being selected from.
        base: Box<Expr>,
        /// The field name.
        name: String,
    },
    /// `callee(args)`.
    Call {
        /// The call's span.
        span: Span,
        /// What is being called.
        callee: Box<Expr>,
        /// The arguments.
        args: Vec<Expr>,
    },
    /// `base[index]`.
    Index {
        /// The index expression's span.
        span: Span,
        /// What is being indexed.
        base: Box<Expr>,
        /// The index.
        index: Box<Expr>,
    },
    /// A prefix operator application.
    Unary {
        /// The expression's span.
        span: Span,
        /// The operator.
        op: UnOp,
        /// The operand.
        operand: Box<Expr>,
    },
    /// A binary operator application.
    Binary {
        /// The expression's span.
        span: Span,
        /// The operator.
        op: BinOp,
        /// The left operand.
        lhs: Box<Expr>,
        /// The right operand.
        rhs: Box<Expr>,
    },
    /// A parenthesised expression.
    Paren {
        /// The span, including the parentheses.
        span: Span,
        /// The inner expression.
        inner: Box<Expr>,
    },
    /// `a if cond else b`.
    If {
        /// The expression's span.
        span: Span,
        /// The condition.
        cond: Box<Expr>,
        /// The value when the condition holds.
        then_: Box<Expr>,
        /// The value otherwise.
        else_: Box<Expr>,
    },
    /// `fn(params) -> body`.
    Lambda {
        /// The lambda's span.
        span: Span,
        /// Its parameters.
        params: Vec<Param>,
        /// Its body.
        body: Box<Expr>,
    },
    /// A span where an expression should have been.
    ///
    /// Present so that a syntax error inside a larger construct does not discard the
    /// whole construct: the checker can still see the shape of what was written.
    Error {
        /// The offending span.
        span: Span,
    },
}

impl StrPart {
    /// The part's span.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Literal { span, .. } | Self::Interpolation { span, .. } => *span,
        }
    }
}

impl Expr {
    /// The expression's span.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Int { span, .. }
            | Self::Float { span, .. }
            | Self::Path { span, .. }
            | Self::Bool { span, .. }
            | Self::None { span }
            | Self::Name { span, .. }
            | Self::Str { span, .. }
            | Self::List { span, .. }
            | Self::Map { span, .. }
            | Self::Field { span, .. }
            | Self::Call { span, .. }
            | Self::Index { span, .. }
            | Self::Unary { span, .. }
            | Self::Binary { span, .. }
            | Self::Paren { span, .. }
            | Self::If { span, .. }
            | Self::Lambda { span, .. }
            | Self::Error { span } => *span,
        }
    }
}
