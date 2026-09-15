//! The widget tree, and the props that drive its layout.
//!
//! A screen is a tree of nodes. Each node is one of a small set of containers plus a leaf, and
//! every node carries the same prop bag — so adding a prop is one field and one arm of the
//! layout pass, rather than a new variant that every pass has to learn about.
//!
//! Nothing here is a variant a plugin cannot be: `SCREENS.md §3` lists the default widget set
//! rather than the only one, and `Kind::Widget` carries a registered name for the rest. A
//! tree that only the built-ins could express would make the registry a decoration.

use vela_render::Color;

use crate::actions::Action;
use crate::props::{Props, SizeSpec};

/// What a node is.
#[derive(Clone, PartialEq, Debug)]
pub enum Kind {
    /// One child, aligned inside the node's own area.
    Box,
    /// Children stacked left to right.
    Row,
    /// Children stacked top to bottom.
    Column,
    /// Children overlapped in z-order, sized to the largest.
    Stack,
    /// Children in a grid of fixed width.
    ///
    /// The column count is the *only* thing a grid decides; every column is as wide as its
    /// widest cell and every row as tall as its tallest. Equal columns would be the easy
    /// thing to build and the wrong one — a grid of labels and values wants each column to
    /// fit its own content.
    Grid {
        /// How many columns before wrapping to the next row.
        columns: usize,
    },
    /// Children laid out in a line that wraps.
    ///
    /// The layout a row of tags or a paragraph of inline buttons wants, and the one a `row`
    /// cannot give: a row puts everything on one line and clips.
    Flow,
    /// Empty space, flexible along the parent's main axis.
    Spacer,
    /// A leaf whose size comes from outside the layout — text, an image, a video.
    ///
    /// The size is *measured*, not declared: `vela-text` shapes and wraps, and this is where
    /// that answer arrives. Keeping it as a value rather than a callback means a layout has no
    /// hidden inputs, so a test can predict it exactly.
    Measured {
        /// What the leaf would like to be.
        size: Size,
    },
    /// A registered widget, by name, with a measured size.
    ///
    /// `CONVENTIONS.md §4.2`: a widget is a registry entry, not a variant. A plugin's widget
    /// is a `Widget` here, and the layout pass treats it exactly as it treats a built-in.
    Widget {
        /// The registered name.
        name: String,
        /// What it would like to be.
        size: Size,
    },
    /// A run of text, with the string it draws.
    ///
    /// The content is carried here rather than looked up when painting because the widget
    /// tree is the *evaluated* screen: `text line` has already been resolved to a string by
    /// the time a node exists, and a painter that re-evaluated the source would need the
    /// screen's arguments again. The size is measured with `vela-text` at the same moment,
    /// which keeps the layout solver pure and font-free.
    Text {
        /// The resolved text.
        text: String,
        /// What it measured to.
        size: Size,
    },
}

/// A width and a height in pixels.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Size {
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl Size {
    /// A size.
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    /// A size with nothing in it.
    pub const ZERO: Self = Self::new(0.0, 0.0);
}

/// What a node draws with, resolved when the screen is instantiated.
///
/// Separate from [`Props`] on purpose: the layout solver reads `props`, the painter reads
/// `paint`, and a colour cannot influence a rectangle. Folding the two together would put a
/// value the solver must never read into the struct it reads from.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Paint {
    /// A fill colour behind this node, from a `background` prop.
    pub background: Option<Color>,
    /// A text colour, from the node's `style`.
    pub color: Option<Color>,
    /// A text size in pixels, from the node's `style`.
    pub size: Option<f32>,
}

/// One node.
#[derive(Clone, PartialEq, Debug)]
pub struct Node {
    /// What it is.
    pub kind: Kind,
    /// How it is laid out.
    pub props: Props,
    /// How it is drawn.
    pub paint: Paint,
    /// What activating it does, if it is interactive.
    ///
    /// Stored on the node rather than the painter because it is not paint: a node with an
    /// action is a *hotspot*, and where it sits is a laid-out rectangle, not a colour.
    pub action: Option<Action>,
    /// What is inside it.
    pub children: Vec<Node>,
}

impl Node {
    /// A container with children.
    #[must_use]
    pub fn new(kind: Kind, children: Vec<Node>) -> Self {
        Self {
            kind,
            props: Props::default(),
            paint: Paint::default(),
            action: None,
            children,
        }
    }

    /// A leaf that measures to `size`.
    #[must_use]
    pub fn measured(size: Size) -> Self {
        Self {
            kind: Kind::Measured { size },
            props: Props::default(),
            paint: Paint::default(),
            action: None,
            children: Vec::new(),
        }
    }

    /// Flexible empty space.
    #[must_use]
    pub fn spacer() -> Self {
        let mut node = Self::new(Kind::Spacer, Vec::new());
        node.props.grow = 1.0;
        node
    }

    /// Sets the padding.
    #[must_use]
    pub fn pad(mut self, pad: f32) -> Self {
        self.props.pad = pad;
        self
    }

    /// Sets the gap between children.
    #[must_use]
    pub fn gap(mut self, gap: f32) -> Self {
        self.props.gap = gap;
        self
    }

    /// Sets the flex weight along the parent's main axis.
    #[must_use]
    pub fn grow(mut self, grow: f32) -> Self {
        self.props.grow = grow;
        self
    }

    /// Fixes the size on both axes.
    #[must_use]
    pub fn size(mut self, width: SizeSpec, height: SizeSpec) -> Self {
        self.props.width = width;
        self.props.height = height;
        self
    }
}
