//! The widget tree a `screen` body describes.
//!
//! # Why a widget and a prop look the same here
//!
//! `SCREENS.md §2` writes both with the same shape:
//!
//! ```vela
//! box at bottom, stretch_x:
//!     pad 24
//!     column gap 8:
//!         text line style = body
//! ```
//!
//! `pad 24` is a prop of the box, `column gap 8:` is a child widget, and syntactically they
//! are the same line: a name followed by words. Telling them apart needs the widget
//! vocabulary, which lives at a higher rank than the parser — and that is *why* `E5005`
//! ("unknown widget") and `E5006` ("wrong prop") exist as separate diagnostics rather than as
//! parse errors.
//!
//! So the parser produces the shape faithfully and the checker decides what each line is.

use crate::tree::Expr;
use vela_span::Span;

/// One `elif` arm of a screen's [`ScreenLine::If`].
///
/// The same shape as the language's `ElifClause` for statements (`LANGUAGE.md §3`,
/// `if_stmt = "if" expr block { "elif" expr block } [ "else" block ]`) — deliberately, because a
/// conditional is one idea in Vela and a screen conditional that could not spell `elif` would be a
/// second, narrower one.
#[derive(Clone, Debug)]
pub struct ScreenElif {
    /// The clause's span, from its `elif` to the end of its body.
    pub span: Span,
    /// What this arm tests. Evaluated only when no arm before it held.
    pub condition: Expr,
    /// The lines drawn when it holds.
    pub body: Vec<ScreenLine>,
}

/// One line of a screen body.
#[derive(Clone, Debug)]
pub enum ScreenLine {
    /// `layer ui` — which layer the screen's root draws into.
    Layer {
        /// The line's span.
        span: Span,
        /// The layer's name.
        name: String,
    },
    /// `style_prefix <name>` — a style every widget in this block falls back to.
    ///
    /// Each widget resolves `{name}_{widget}` if such a style is declared (`say_text` for a `text`),
    /// which is how one line gives a screen's whole contents a skin. A widget that writes its own
    /// `style = …` keeps it; a block that declares its own prefix overrides the enclosing one; and a
    /// used screen does not inherit the caller's, because a screen is a function (`§2.1`).
    StylePrefix {
        /// The line's span.
        span: Span,
        /// The prefix, as written.
        name: String,
    },
    /// `if <condition>:` with an indented body, and the arms that follow it.
    ///
    /// The `elif`s and the `else` are *clauses of this line*, not lines beside it, which is the shape
    /// the statement form already has (`IfStmt`). That is what keeps a conditional one node: the arms
    /// belong to one another, so a stray `else` cannot exist, and choosing an arm is a single
    /// decision rather than a walk that has to remember what the previous line concluded.
    If {
        /// The line's span, over the whole chain.
        span: Span,
        /// What the `if` tests.
        condition: Expr,
        /// The lines drawn when it holds.
        body: Vec<ScreenLine>,
        /// The `elif` arms, in order. Each is tested only if everything before it did not hold.
        elifs: Vec<ScreenElif>,
        /// The `else` arm, if one was written: the lines drawn when nothing held.
        else_body: Option<Vec<ScreenLine>>,
    },
    /// `use <screen> [ ( args ) ] [ : block ]` — another screen, included here.
    ///
    /// A screen is a function (`SCREENS.md §2`), so this is a call: the used screen sees its own
    /// parameters and nothing else. There is no Ren'Py-style shared scope, because a screen whose
    /// meaning depended on where it was used could not have a static dependency set (`§8.2`).
    ///
    /// A block makes the used screen a *wrapper*: it becomes the lines placed where that screen
    /// writes `transclude`.
    Use {
        /// The line's span.
        span: Span,
        /// The screen being used, as written. Resolved in this file, like a style (`§5`).
        name: String,
        /// The arguments: positional values, and named ones written `name = value`.
        args: Vec<ScreenArg>,
        /// The block, if one was written — inserted where the used screen transcludes.
        body: Vec<ScreenLine>,
    },
    /// `transclude` — where a caller's block is placed.
    ///
    /// Nothing is written after it: it takes exactly what the `use` that reaches this screen passed.
    Transclude {
        /// The line's span.
        span: Span,
    },
    /// `key <action> action <call>` — a semantic action this screen answers while it is shown.
    ///
    /// The name is the *host's* (`SCREENS.md §11`: `advance`, `cancel`, `menu_up`, …), not a device
    /// key: input is abstracted to what the player meant, so a screen never asks which button was
    /// pressed and a profile can move the button without touching the screen. The checker holds the
    /// name to the vocabulary it was given, because a binding that names nothing is a key that does
    /// nothing and says nothing.
    Key {
        /// The line's span.
        span: Span,
        /// The semantic action, as written — a name, not Ren'Py's string.
        name: String,
        /// What the screen does when it arrives: a call, or a parameter's name.
        action: Expr,
    },
    /// `timer <seconds> action <call> [repeat]` — what the screen does when time passes.
    ///
    /// The deadline is data here (`Laid::timers`) rather than a behaviour, because the clock is not
    /// this layer's: `SCREENS.md §6` drives animation from `World::clock`, and until something
    /// advances a clock into the screen runtime a timer is a declaration nothing fires yet.
    Timer {
        /// The line's span.
        span: Span,
        /// How long, in seconds. A value, resolved like any other.
        seconds: Expr,
        /// What the screen does when it elapses.
        action: Expr,
        /// Whether it fires again after it elapses.
        repeat: bool,
    },
    /// `for <name> in <expr>:` — the body once per element (`SCREENS.md §2.4`).
    ///
    /// Children from data, and the shape is the statement form's (`ForStmt`). The binding is a
    /// *scope*: a name it binds is not a read of anything outside the loop, which is what keeps the
    /// static dependency set (`§8.2`) honest — a loop over a list of options does not depend on
    /// whatever `option` might otherwise have meant.
    For {
        /// The line's span.
        span: Span,
        /// The name bound to each element.
        binding: String,
        /// What is walked: a list, or a value that is one.
        iterable: Expr,
        /// The lines drawn once per element.
        body: Vec<ScreenLine>,
    },
    /// A widget, or a prop written on its own line.
    Node(ScreenNode),
}

impl ScreenLine {
    /// Every body this line holds, in the order it draws them: an `if`'s arms, or a `for`'s iteration.
    ///
    /// Empty for a line that holds no body of its own. A walk over a screen body has to see all of
    /// them — a button in an `elif`, or inside a loop, is as much a button as one at the top — and a
    /// walker that destructured one body and moved on compiles perfectly while losing the rest, which
    /// no compiler catches. So the question is answered once here rather than remembered at each of a
    /// dozen sites.
    ///
    /// A `use` and a widget are deliberately *not* here: both carry something besides a body —
    /// arguments, props — so a walker that recursed through this would skip its own work on them.
    #[must_use]
    pub fn bodies(&self) -> Vec<&[ScreenLine]> {
        match self {
            Self::If {
                body,
                elifs,
                else_body,
                ..
            } => {
                let mut arms: Vec<&[ScreenLine]> = Vec::with_capacity(2 + elifs.len());
                arms.push(body);
                arms.extend(elifs.iter().map(|clause| clause.body.as_slice()));
                arms.extend(else_body.iter().map(Vec::as_slice));
                arms
            }
            // A loop draws its body once per element, so the body is drawn — for as many elements as
            // there are, which is a runtime question and not one a static walk can answer.
            Self::For { body, .. } => vec![body],
            Self::Key { .. }
            | Self::Timer { .. }
            | Self::Layer { .. }
            | Self::StylePrefix { .. }
            | Self::Transclude { .. }
            | Self::Node(_)
            | Self::Use { .. } => Vec::new(),
        }
    }
}

/// One word or value after a name.
///
/// Three shapes, and the parser cannot tell which is which without the widget vocabulary:
///
/// ```vela
/// text "hi"                   // a value, which is the text widget's content
/// text name style = speaker   // a value, then a named arg
/// box at bottom, stretch_x    // two named args, the second with no value
/// pad 24                      // a named arg
/// ```
///
/// So both shapes are represented and the checker decides.
#[derive(Clone, Debug)]
pub enum ScreenArg {
    /// A value with no name, which is how a leaf takes its content.
    Value(Expr),
    /// A name, with or without a value — a bare name is a flag (`stretch_x`).
    Named {
        /// The arg's span.
        span: Span,
        /// Its name.
        name: String,
        /// Its value, if it takes one.
        value: Option<Expr>,
    },
}

/// A named line with props, and possibly children.
#[derive(Clone, Debug)]
pub struct ScreenNode {
    /// The line's span.
    pub span: Span,
    /// The widget or prop name.
    pub name: String,
    /// The words and values written after the name.
    pub args: Vec<ScreenArg>,
    /// The indented lines under it, if it ended in `:`.
    pub children: Vec<ScreenLine>,
}

impl ScreenNode {
    /// Whether the line opened a block.
    #[must_use]
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }
}
