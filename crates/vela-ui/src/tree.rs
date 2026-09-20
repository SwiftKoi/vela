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
    /// A window onto content that is bigger than it (`SCREENS.md §3.2`).
    ///
    /// The child is measured against *no* limit along the scroll axis, so a column taller than the box
    /// keeps its height instead of being squeezed to fit, and the part outside the box is clipped rather
    /// than drawn over its neighbours. `initial` is where the window starts, as a fraction of the travel:
    /// `0` the top, `1` the bottom — a fraction rather than a pixel offset for the reason §4.2 gives, that
    /// a layout needing arithmetic on pixels is a layout missing a prop.
    Viewport {
        /// Where the window starts, `0.0`..=`1.0` of however far the content can travel.
        initial: f32,
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
    /// A picture, with the name it was resolved to.
    ///
    /// The *name*, not a texture: resolving one to the other is the painter's job, because the platform
    /// that uploaded the picture is the only party that knows which texture it became (`images.rs`).
    /// The name is also what a reference in a screen is, so a node that carried a texture id would be
    /// carrying something no screen can be written with. The size does come from the picture — layout
    /// has to know it before anything is painted — so a screen whose picture has not arrived lays out
    /// at nothing rather than at a guess.
    Image {
        /// The picture's name, as resolved — `bg.room`, or whatever a value held.
        name: String,
        /// What the platform said it measures.
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

/// An interaction state a node can draw in (`SCREENS.md §5`).
///
/// A *style* carries a value per state — `hover_color` is `hover` applied to `color` (see
/// [`State::split`]) — and the runtime picks one when it paints.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum State {
    /// Nothing is happening to the node: the value it has on its own.
    ///
    /// This is what Ren'Py's `idle_*` also means, which is why the two spellings are one setting.
    #[default]
    Idle,
    /// The pointer is over it.
    Hover,
    /// It is the one being navigated to — the focused control (`§10`).
    Selected,
    /// It cannot be used: its `enable_if` does not hold.
    Insensitive,
}

impl State {
    /// The state a style setting's key names, and the key without that prefix.
    ///
    /// `hover_color` is the colour for `hover`, and a key with no prefix is `Idle`. `idle_color` and
    /// `color` are therefore the same setting, which is deliberate: `idle` is not a fourth value to
    /// store but the *name* of the one a prop has when nothing else applies.
    ///
    /// One definition, so the resolver and anything that reads a key agree about what it means —
    /// the same reason a widget's props have a single schema.
    #[must_use]
    pub fn split(key: &str) -> (Self, &str) {
        for (prefix, state) in [
            ("hover_", Self::Hover),
            ("selected_", Self::Selected),
            ("insensitive_", Self::Insensitive),
            ("idle_", Self::Idle),
        ] {
            if let Some(rest) = key.strip_prefix(prefix) {
                return (state, rest);
            }
        }
        (Self::Idle, key)
    }
}

/// What a node draws with, resolved when the screen is instantiated.
///
/// Separate from [`Props`] on purpose: the layout solver reads `props`, the painter reads
/// `paint`, and a colour cannot influence a rectangle. Folding the two together would put a
/// value the solver must never read into the struct it reads from.
///
/// The fields here are the *idle* values; each state's overrides are kept beside them rather than
/// merged in, because a node's state changes while its layout does not. Instantiation cannot pick
/// one: the focus cursor moves every frame, and a tree that had folded `selected` in would have to
/// be rebuilt to recolour a button.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Paint {
    /// A fill colour behind this node, from a `background` prop or setting.
    pub background: Option<Color>,
    /// A text colour, from the node's `style`.
    pub color: Option<Color>,
    /// A text size in pixels, from the node's `style`.
    pub size: Option<f32>,
    /// The font text is shaped with, from the node's `style` (`SCREENS.md §5`).
    ///
    /// A *name*, resolved against the engine's fonts when the text is measured and drawn: a style
    /// cannot know which faces a build carried, and a screen that named one it did not have falls
    /// back to the screen's own font rather than drawing nothing.
    pub font: Option<String>,
    /// What the pointer being over it changes.
    pub hover: Option<Box<Paint>>,
    /// What being the focused control changes.
    pub selected: Option<Box<Paint>>,
    /// What being unusable changes.
    pub insensitive: Option<Box<Paint>>,
}

impl Paint {
    /// This paint with a state's values applied, field by field.
    ///
    /// Field by field rather than wholesale: a state that sets only `color` keeps the idle
    /// `background`, because an override is a *diff* and not a replacement. And the result carries no
    /// overrides of its own, so resolving is not recursive and a painter cannot loop.
    #[must_use]
    pub fn in_state(&self, state: State) -> Self {
        let Some(overridden) = self.over(state) else {
            return self.clone();
        };
        Self {
            background: overridden.background.or(self.background),
            color: overridden.color.or(self.color),
            size: overridden.size.or(self.size),
            font: overridden.font.clone().or_else(|| self.font.clone()),
            hover: None,
            selected: None,
            insensitive: None,
        }
    }

    /// The override for one state, if the style wrote any.
    ///
    /// `Idle` has none: it is the values themselves.
    #[must_use]
    pub fn over(&self, state: State) -> Option<&Self> {
        match state {
            State::Idle => None,
            State::Hover => self.hover.as_deref(),
            State::Selected => self.selected.as_deref(),
            State::Insensitive => self.insensitive.as_deref(),
        }
    }

    /// The values for one state, created empty when nothing has written them yet.
    ///
    /// For a resolver filling a paint in from a style's settings, which may name a state before any
    /// other setting has touched it. `Idle` is the paint itself, so a resolver never allocates for the
    /// common case.
    pub(crate) fn state_mut(&mut self, state: State) -> &mut Self {
        match state {
            State::Idle => self,
            State::Hover => self
                .hover
                .get_or_insert_with(|| Box::new(Self::default()))
                .as_mut(),
            State::Selected => self
                .selected
                .get_or_insert_with(|| Box::new(Self::default()))
                .as_mut(),
            State::Insensitive => self
                .insensitive
                .get_or_insert_with(|| Box::new(Self::default()))
                .as_mut(),
        }
    }
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
    /// Whether the setting this control writes is the one the store holds (`SCREENS.md §5.1`).
    ///
    /// Resolved at instantiation, because that is the one place the action *and* the scope are both in
    /// hand, and stored because it is a fact about this layout: the painter's focus cursor moves without a
    /// re-lay, and a setting does not.
    pub chosen: bool,
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
            chosen: false,
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
            chosen: false,
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

    /// Moves the node from wherever its parent placed it (`SCREENS.md §4.2`).
    #[must_use]
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.props.offset_x = x;
        self.props.offset_y = y;
        self
    }
}
