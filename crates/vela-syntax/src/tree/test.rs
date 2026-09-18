//! Tests: the `test` items that live beside the story they exercise.
//!
//! `TOOLING.md §5` puts them in `.vela` files, next to the stories, rather than in a format of their
//! own — which is why they are an item here: parsed by the same parser, checked by the same checker, and
//! formatted by the same formatter as everything else. A separate test language would have to be taught
//! to resolve labels, evaluate expressions, and keep the formatter honest, and it would be a second
//! answer to questions that already have one.
//!
//! # Why the directives are not keywords
//!
//! Inside a test, a line begins with a directive name — `run`, `advance`, `choose`, `expect`, `cover` —
//! and those are *contextual*: an ordinary identifier in the first position of a test's line, looked up
//! by name. Every one of them is a plausible label or variable in a story, and `expect_name`'s doc
//! comment records four separate bugs from exactly this trade being made the other way. `test` itself is
//! a keyword, because the parser has to know where a test begins and no story has another use for a
//! heading called `test`.

use vela_span::Span;

use crate::tree::Expr;

/// A `test` item.
#[derive(Debug)]
pub struct TestDecl {
    /// The span of the whole item.
    pub span: Span,
    /// The test's name, as written: a sentence, which is what a report prints and `--filter` matches.
    pub name: String,
    /// What the test does, in order.
    pub directives: Vec<Directive>,
}

/// One line of a test.
#[derive(Debug)]
pub struct Directive {
    /// The span of the line, which is what a failure points at.
    pub span: Span,
    /// What it says to do.
    pub kind: DirectiveKind,
}

/// What a directive says to do.
#[derive(Debug)]
pub enum DirectiveKind {
    /// `run from <label>`: start the story here rather than at the entry label.
    ///
    /// The target is a dotted path with its own span, the same pair `JumpStmt` carries, so a reference
    /// written here resolves the way every other reference resolves — and a rename can edit it.
    Run {
        /// The target label's dotted path.
        target: Vec<String>,
        /// The target's own span, without the `from`.
        target_span: Span,
    },
    /// `advance <n>`: consume this many commands the story is waiting on.
    Advance {
        /// How many.
        count: u32,
    },
    /// `advance until shown <expr>`: consume commands until this text is on screen.
    ///
    /// The subject is `shown` rather than a bare expression because what a *screen* shows is not what
    /// the `World` holds: `expect trust == 1` is about the story, and this is about the picture. Two
    /// subjects, two words, so a failing test says which of the two it was about.
    AdvanceUntil {
        /// The text to wait for, as an expression.
        text: Expr,
    },
    /// `expect shown <expr>` / `expect not shown <expr>`: assert what is on screen.
    ExpectShown {
        /// The text to look for, as an expression.
        text: Expr,
        /// Whether the assertion is that it is *not* there.
        negated: bool,
    },
    /// `choose <text>`: pick the menu option whose text is this.
    Choose {
        /// The option's text. An expression rather than a string literal, so a test can name the text
        /// once and use it twice.
        text: Expr,
    },
    /// `expect <expr>`: assert that this holds.
    Expect {
        /// The assertion. A real expression, so the checker types it and a mistake in a test is a
        /// diagnostic rather than a runtime surprise.
        expr: Expr,
    },
    /// `cover <what>`: assert that the story reaches everything.
    Cover {
        /// What is covered.
        mode: CoverMode,
    },
}

/// What a `cover` directive asserts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CoverMode {
    /// Every label is executed by the suite.
    Labels,
    /// Every enum variant is matched by some `match`.
    Variants,
}

impl CoverMode {
    /// The spelling, which is also what the parser matches.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Labels => "labels",
            Self::Variants => "variants",
        }
    }

    /// Recognises a spelling, for the parser and for the report a failure prints.
    #[must_use]
    pub fn lookup(text: &str) -> Option<Self> {
        match text {
            "labels" => Some(Self::Labels),
            "variants" => Some(Self::Variants),
            _ => None,
        }
    }
}
