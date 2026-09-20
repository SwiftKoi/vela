//! Declared props: the one definition three consumers read.
//!
//! `SCREENS.md §9`: the prop schema is the source of truth for the **type checker**, the
//! **LSP completion list**, and the **generated docs**. Three consumers, one definition — so
//! they cannot drift. The alternative is what every UI toolkit eventually does: a checker
//! that knows one set of props, documentation that knows another, and an editor that
//! completes a third.

/// What a prop's value is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PropType {
    /// A number: `pad 8`.
    Number,
    /// A size in pixels: `size 320`, `width 280`.
    ///
    /// A *number* rather than a wildcard: an axis that follows its content is the default, and one
    /// that fills its parent is `stretch_x`/`stretch_y` (`SCREENS.md §4.2`), so there is nothing
    /// left for a value to say. `50%` was documented here and never parsed — `%` is the modulo
    /// operator — which is why the row now names the spelling that exists.
    Dimension,
    /// One of a fixed set: `align center`.
    Anchor,
    /// A bare word, checked against the widget's own vocabulary.
    Word,
    /// A string: `text "Hello."`.
    Text,
    /// A dotted asset path: `image "bg.street"`.
    Asset,
    /// A label or screen name, resolved by the checker rather than here.
    Target,
    /// Any value: what to write into something (`set_screen_variable(device, "mouse")`).
    ///
    /// The one type that is not about *spelling*: an argument of this kind is resolved by the screen
    /// before the runtime sees it, so `"mouse"` arrives as the string and `item.kind` as whatever the
    /// element holds — which is the difference between a write that means what it says and one that
    /// stores the text of the expression.
    Value,
}

impl PropType {
    /// The name a diagnostic prints and a completion list shows.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::Number => "a number",
            Self::Dimension => "a size",
            Self::Anchor => "an anchor",
            Self::Word => "a word",
            Self::Text => "text",
            Self::Asset => "an asset",
            Self::Target => "a label or screen",
            Self::Value => "a value",
        }
    }
}

/// One declared prop.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PropDecl {
    /// The name as written in a screen.
    pub name: &'static str,
    /// What its value must be.
    pub ty: PropType,
    /// Whether a screen must give it.
    pub required: bool,
    /// What it means, for the docs and for a hover.
    pub doc: &'static str,
}

/// The props a widget accepts.
///
/// A slice rather than a map: the set is fixed at compile time and small, and a linear scan
/// over a dozen entries is faster than hashing — as well as keeping the order stable, which
/// is what lets the docs and the completion list be golden-tested.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PropsSchema {
    /// The props, in documentation order.
    pub props: &'static [PropDecl],
}

impl PropsSchema {
    /// A schema with the given props.
    #[must_use]
    pub const fn new(props: &'static [PropDecl]) -> Self {
        Self { props }
    }

    /// Whether the widget accepts `name`.
    #[must_use]
    pub fn accepts(&self, name: &str) -> bool {
        self.props.iter().any(|prop| prop.name == name)
    }

    /// The declaration for `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&'static PropDecl> {
        self.props.iter().find(|prop| prop.name == name)
    }

    /// The prop names, for a completion list.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.props.iter().map(|prop| prop.name).collect()
    }
}

/// The props every widget accepts.
///
/// Shared rather than repeated: a prop that applies to everything should be declared once, or
/// the day someone adds `z` to `text` and forgets `button` is the day the checker starts
/// rejecting valid screens.
pub const COMMON: &[PropDecl] = &[
    PropDecl {
        name: "size",
        ty: PropType::Dimension,
        required: false,
        doc: "Both axes at once; `width` and `height` set one each.",
    },
    PropDecl {
        name: "width",
        ty: PropType::Dimension,
        required: false,
        doc: "A fixed width; the height still follows the content.",
    },
    PropDecl {
        name: "height",
        ty: PropType::Dimension,
        required: false,
        doc: "A fixed height; the width still follows the content.",
    },
    PropDecl {
        name: "min",
        ty: PropType::Dimension,
        required: false,
        doc: "Smallest size this node may take. Declared, but nothing reads it yet.",
    },
    PropDecl {
        name: "max",
        ty: PropType::Dimension,
        required: false,
        doc: "Largest size this node may take. Declared, but nothing reads it yet.",
    },
    PropDecl {
        name: "grow",
        ty: PropType::Number,
        required: false,
        doc: "Flex weight along the parent's main axis.",
    },
    PropDecl {
        name: "anchor",
        ty: PropType::Anchor,
        required: false,
        doc: "Where this node sits inside its parent's slot.",
    },
    PropDecl {
        name: "at",
        ty: PropType::Word,
        required: false,
        doc: "Position, scale, and alpha offset.",
    },
    PropDecl {
        name: "id",
        ty: PropType::Word,
        required: false,
        doc: "A stable name, so an edit keeps this node's state.",
    },
    PropDecl {
        name: "style",
        ty: PropType::Word,
        required: false,
        doc: "A `style` declaration to draw with.",
    },
    PropDecl {
        name: "background",
        ty: PropType::Word,
        required: false,
        doc: "A fill colour behind this node, as a theme token (`theme.bg`).",
    },
    PropDecl {
        name: "stretch_x",
        ty: PropType::Word,
        required: false,
        doc: "Fill the parent along the horizontal axis.",
    },
    PropDecl {
        name: "stretch_y",
        ty: PropType::Word,
        required: false,
        doc: "Fill the parent along the vertical axis.",
    },
];

/// The props an interactive widget accepts, on top of [`COMMON`].
///
/// `SCREENS.md §7`: interaction produces *typed actions* and a screen never mutates state
/// directly — so a widget declares what it does, not what it changes.
pub const INTERACTIVE: &[PropDecl] = &[
    PropDecl {
        name: "action",
        ty: PropType::Word,
        required: false,
        doc: "What activating this does: `action jump(forest.confession)`.",
    },
    PropDecl {
        name: "enable_if",
        ty: PropType::Word,
        required: false,
        doc: "A condition; the widget is inert when it does not hold.",
    },
    PropDecl {
        name: "label",
        ty: PropType::Text,
        required: false,
        doc: "What a screen reader announces, when the content is not enough.",
    },
    PropDecl {
        name: "value",
        ty: PropType::Word,
        required: false,
        doc: "What a `bar`, `slider`, or `input` shows or edits.",
    },
];

/// The props every container accepts, on top of [`COMMON`].
pub const CONTAINER: &[PropDecl] = &[
    PropDecl {
        name: "pad",
        ty: PropType::Number,
        required: false,
        doc: "Space inside the container's edges.",
    },
    PropDecl {
        name: "align",
        ty: PropType::Anchor,
        required: false,
        doc: "Where children sit in the space they are given.",
    },
];
