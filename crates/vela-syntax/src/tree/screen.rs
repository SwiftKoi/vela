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
    /// `if <condition>:` with an indented body.
    If {
        /// The line's span.
        span: Span,
        /// What is being tested.
        condition: Expr,
        /// The lines drawn when it holds.
        body: Vec<ScreenLine>,
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
    /// A widget, or a prop written on its own line.
    Node(ScreenNode),
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
