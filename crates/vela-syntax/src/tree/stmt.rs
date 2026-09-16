//! Statements.

use vela_span::Span;

use crate::tree::{Expr, Type};

/// A `say` statement: narration, or a character speaking.
#[derive(Debug)]
pub struct SayStmt {
    /// The statement's span.
    pub span: Span,
    /// The speaker, if one was named.
    pub speaker: Option<String>,
    /// Image attributes selecting a variant, e.g. `sad` in `eileen sad "..."`.
    pub attributes: Vec<String>,
    /// The line itself.
    pub line: Expr,
    /// Say-scoped options, e.g. `(volume=0.5)`.
    pub options: Vec<(String, Expr)>,
    /// A transition applied when the line is advanced.
    pub transition: Option<String>,
}

/// One choice in a [`MenuStmt`].
#[derive(Debug)]
pub struct MenuChoice {
    /// The choice's span.
    pub span: Span,
    /// The text shown to the player.
    pub text: Expr,
    /// A guard; the choice is only offered when this holds.
    pub condition: Option<Expr>,
    /// What choosing it does.
    pub body: Vec<Stmt>,
}

/// A `menu` statement.
#[derive(Debug)]
pub struct MenuStmt {
    /// The statement's span.
    pub span: Span,
    /// An optional prompt shown above the choices.
    pub prompt: Option<Expr>,
    /// The choices.
    pub choices: Vec<MenuChoice>,
}

/// A `jump` statement.
#[derive(Debug)]
pub struct JumpStmt {
    /// The statement's span.
    pub span: Span,
    /// The target label's dotted path.
    pub target: Vec<String>,
    /// The target's own span, without the `jump` keyword.
    ///
    /// The statement's span covers the keyword too, and a rename may replace the *name* and nothing
    /// else — a tool that rewrote `jump forest.clearing` as a whole statement would delete the word
    /// `jump`. The parser already had this span in hand; recording it is what makes a reference a
    /// range a tool can edit rather than one it can only jump to.
    pub target_span: Span,
}

/// A `call` statement.
#[derive(Debug)]
pub struct CallStmt {
    /// The statement's span.
    pub span: Span,
    /// The target label's dotted path.
    pub target: Vec<String>,
    /// The target's own span, for the reason [`JumpStmt::target_span`] gives.
    pub target_span: Span,
    /// A transition applied while the call runs.
    pub transition: Option<String>,
}

/// A `return` statement.
#[derive(Debug)]
pub struct ReturnStmt {
    /// The statement's span.
    pub span: Span,
    /// The value returned, if any.
    pub value: Option<Expr>,
}

/// A `scene`, `show`, or `hide` statement.
#[derive(Debug)]
pub struct StageStmt {
    /// The statement's span.
    pub span: Span,
    /// Which of the three it is.
    pub kind: StageKind,
    /// The image being staged, as a dotted path.
    pub image: Vec<String>,
    /// Image attributes, e.g. `happy`.
    pub attributes: Vec<String>,
    /// Transforms applied, from an `at` clause.
    pub transforms: Vec<String>,
    /// A transition applied, from a `with` clause.
    pub transition: Option<String>,
}

/// Which staging statement a [`StageStmt`] is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StageKind {
    /// `scene`
    Scene,
    /// `show`
    Show,
    /// `hide`
    Hide,
}

/// A standalone `with` statement, applying a transition to the current scene.
#[derive(Debug)]
pub struct WithStmt {
    /// The statement's span.
    pub span: Span,
    /// The transition name.
    pub transition: String,
}

/// The kinds of `wait`.
#[derive(Debug)]
pub enum WaitEvent {
    /// `wait click`.
    Click,
    /// `wait <duration>`.
    Duration(Expr),
}

/// A `pause` or `wait` statement.
#[derive(Debug)]
pub struct WaitStmt {
    /// The statement's span.
    pub span: Span,
    /// What is being waited for.
    pub event: WaitEvent,
}

/// A `play`, `stop`, or `queue` statement.
#[derive(Debug)]
pub struct AudioStmt {
    /// The statement's span.
    pub span: Span,
    /// Which of the three it is.
    pub kind: AudioKind,
    /// The channel, e.g. `music`.
    pub channel: String,
    /// The source, for `play` and `queue`.
    pub source: Option<Expr>,
    /// Whether `play` was given `loop`.
    pub looping: bool,
    /// A fade duration.
    pub fade: Option<Expr>,
}

/// Which audio statement an [`AudioStmt`] is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AudioKind {
    /// `play`
    Play,
    /// `stop`
    Stop,
    /// `queue`
    Queue,
}

/// One `elif` clause of an [`IfStmt`].
#[derive(Debug)]
pub struct ElifClause {
    /// The clause's span.
    pub span: Span,
    /// Its condition.
    pub condition: Expr,
    /// Its body.
    pub body: Vec<Stmt>,
}

/// An `if` statement.
#[derive(Debug)]
pub struct IfStmt {
    /// The statement's span.
    pub span: Span,
    /// The leading condition.
    pub condition: Expr,
    /// The body taken when the condition holds.
    pub then_body: Vec<Stmt>,
    /// The `elif` clauses, in order.
    pub elifs: Vec<ElifClause>,
    /// The `else` body, if present.
    pub else_body: Option<Vec<Stmt>>,
}

/// A `while` statement.
#[derive(Debug)]
pub struct WhileStmt {
    /// The statement's span.
    pub span: Span,
    /// The loop condition.
    pub condition: Expr,
    /// The loop body.
    pub body: Vec<Stmt>,
}

/// A `for` statement.
#[derive(Debug)]
pub struct ForStmt {
    /// The statement's span.
    pub span: Span,
    /// The name bound on each iteration.
    pub binding: String,
    /// What is being iterated.
    pub iterable: Expr,
    /// The loop body.
    pub body: Vec<Stmt>,
}

/// A `match` statement.
#[derive(Debug)]
pub struct MatchStmt {
    /// The statement's span.
    pub span: Span,
    /// What is being matched.
    pub scrutinee: Expr,
    /// The arms.
    pub arms: Vec<MatchArm>,
}

/// One arm of a [`MatchStmt`].
#[derive(Debug)]
pub struct MatchArm {
    /// The arm's span.
    pub span: Span,
    /// The pattern, or `None` for `else`.
    pub pattern: Option<Pattern>,
    /// A guard, from `if`.
    pub guard: Option<Expr>,
    /// The arm's body.
    pub body: Vec<Stmt>,
}

/// A `when` pattern.
#[derive(Debug)]
pub struct Pattern {
    /// The pattern's span.
    pub span: Span,
    /// The variant's dotted path, empty for the `_` wildcard.
    pub path: Vec<String>,
    /// Names bound to the variant's fields.
    pub bindings: Vec<String>,
}

/// A `var` statement.
#[derive(Debug)]
pub struct VarStmt {
    /// The statement's span.
    pub span: Span,
    /// The variable's name.
    pub name: String,
    /// A declared type, if one was written.
    pub ty: Option<Type>,
    /// The initial value.
    pub value: Expr,
}

/// Which assignment operator an [`AssignStmt`] uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AssignOp {
    /// `=`
    Assign,
    /// `+=`
    Add,
    /// `-=`
    Sub,
    /// `*=`
    Mul,
    /// `/=`
    Div,
}

/// An assignment to an existing place.
#[derive(Debug)]
pub struct AssignStmt {
    /// The statement's span.
    pub span: Span,
    /// What is being assigned to.
    pub target: Expr,
    /// How.
    pub op: AssignOp,
    /// The value.
    pub value: Expr,
}

/// An expression evaluated for its effect.
#[derive(Debug)]
pub struct ExprStmt {
    /// The statement's span.
    pub span: Span,
    /// The expression.
    pub expr: Expr,
}

/// A statement.
#[derive(Debug)]
pub enum Stmt {
    /// Dialogue or narration.
    Say(SayStmt),
    /// A choice.
    Menu(MenuStmt),
    /// Transfer control to a label.
    Jump(JumpStmt),
    /// Call a label and come back.
    Call(CallStmt),
    /// Return from a call, or end a label.
    Return(ReturnStmt),
    /// Stage an image.
    Stage(StageStmt),
    /// Apply a transition.
    With(WithStmt),
    /// Wait, by duration or for a click.
    Wait(WaitStmt),
    /// Play, stop, or queue audio.
    Audio(AudioStmt),
    /// Branch.
    If(IfStmt),
    /// Loop while a condition holds.
    While(WhileStmt),
    /// Loop over a sequence.
    For(ForStmt),
    /// Branch on a value's shape.
    Match(MatchStmt),
    /// Declare a local variable.
    Var(VarStmt),
    /// Assign to an existing place.
    Assign(AssignStmt),
    /// Evaluate an expression.
    Expr(ExprStmt),
    /// A span where a statement should have been.
    Error {
        /// The offending span.
        span: Span,
    },
}

impl Stmt {
    /// The statement's span.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Say(s) => s.span,
            Self::Menu(s) => s.span,
            Self::Jump(s) => s.span,
            Self::Call(s) => s.span,
            Self::Return(s) => s.span,
            Self::Stage(s) => s.span,
            Self::With(s) => s.span,
            Self::Wait(s) => s.span,
            Self::Audio(s) => s.span,
            Self::If(s) => s.span,
            Self::While(s) => s.span,
            Self::For(s) => s.span,
            Self::Match(s) => s.span,
            Self::Var(s) => s.span,
            Self::Assign(s) => s.span,
            Self::Expr(s) => s.span,
            Self::Error { span } => *span,
        }
    }
}
