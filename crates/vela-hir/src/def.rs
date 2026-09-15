//! Definitions: what a name names.

use vela_span::Span;

/// What kind of thing a name names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DefKind {
    /// A `label`: an entry point for control flow.
    Label,
    /// A `fn`.
    Function,
    /// An `effect`: a capability the host provides.
    Effect,
    /// A `const`.
    Constant,
    /// A `default`: world state persisted in saves.
    Default,
    /// A `struct`.
    Struct,
    /// An `enum`.
    Enum,
    /// A `character`.
    Character,
    /// An `image`.
    Image,
    /// A `transform`.
    Transform,
    /// A `screen`.
    Screen,
    /// A `style`.
    Style,
    /// A `theme`.
    Theme,
}

impl DefKind {
    /// The words a diagnostic uses for this kind.
    ///
    /// Worth having as a method rather than an inline string: the same phrase appears in
    /// "defined more than once" and "first defined here as …", and the two must agree.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::Label => "a label",
            Self::Function => "a function",
            Self::Effect => "an effect",
            Self::Constant => "a constant",
            Self::Default => "a default",
            Self::Struct => "a struct",
            Self::Enum => "an enum",
            Self::Character => "a character",
            Self::Image => "an image",
            Self::Transform => "a transform",
            Self::Screen => "a screen",
            Self::Style => "a style",
            Self::Theme => "a theme",
        }
    }
}

/// One definition.
#[derive(Debug)]
pub struct Definition {
    /// The name it is defined under.
    pub name: String,
    /// What it is.
    pub kind: DefKind,
    /// Where it was written.
    pub span: Span,
}
