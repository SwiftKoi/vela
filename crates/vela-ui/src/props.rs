//! The props a node carries, and the layout modes they select.

/// How a size is decided when it is not the children's to decide.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum SizeSpec {
    /// Whatever the content needs, bounded by the parent's proposal.
    #[default]
    Auto,
    /// A fixed number of pixels.
    Fixed(f32),
    /// A fraction of the parent's proposal, `0.0..=1.0`.
    Percent(f32),
}

impl SizeSpec {
    /// Resolves against a proposal.
    #[must_use]
    pub fn resolve(self, available: f32, content: f32) -> f32 {
        let wanted = match self {
            Self::Auto => content,
            Self::Fixed(value) => value,
            Self::Percent(fraction) => available * fraction,
        };
        // A child never gets more than it was offered: the parent's proposal is a contract,
        // and a child that overflows it is a child that draws outside its box.
        wanted.clamp(0.0, available.max(0.0))
    }
}

/// Where a child sits in the space it was given.
///
/// Anchors rather than coordinates, per `SCREENS.md §4.2`: a layout that needs arithmetic on
/// pixel positions is a layout missing a prop, and an anchor is the prop it is missing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Anchor {
    /// Top-left.
    #[default]
    TopLeft,
    /// Top edge, horizontally centred.
    Top,
    /// Top-right.
    TopRight,
    /// Left edge, vertically centred.
    Left,
    /// Centred on both axes.
    Center,
    /// Right edge, vertically centred.
    Right,
    /// Bottom-left.
    BottomLeft,
    /// Bottom edge, horizontally centred.
    Bottom,
    /// Bottom-right.
    BottomRight,
    /// Fill the space rather than sitting in it.
    Stretch,
}

impl Anchor {
    /// The horizontal fraction, `0.0` left, `0.5` centred, `1.0` right.
    #[must_use]
    pub fn horizontal(self) -> f32 {
        match self {
            Self::TopLeft | Self::Left | Self::BottomLeft => 0.0,
            Self::Top | Self::Center | Self::Bottom => 0.5,
            Self::TopRight | Self::Right | Self::BottomRight => 1.0,
            // Stretch is resolved before this is asked; filling is not a point on an axis.
            Self::Stretch => 0.0,
        }
    }

    /// The vertical fraction, `0.0` top, `0.5` centred, `1.0` bottom.
    #[must_use]
    pub fn vertical(self) -> f32 {
        match self {
            Self::TopLeft | Self::Top | Self::TopRight => 0.0,
            Self::Left | Self::Center | Self::Right => 0.5,
            Self::BottomLeft | Self::Bottom | Self::BottomRight => 1.0,
            Self::Stretch => 0.0,
        }
    }
}

/// Everything the layout pass reads.
///
/// One bag for every node rather than a variant per kind: a prop that applies to three
/// containers should be one field that three arms read, not three fields that can disagree.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Props {
    /// Space inside the node's edges.
    pub pad: f32,
    /// Space between children.
    pub gap: f32,
    /// Where children sit.
    pub align: Anchor,
    /// Where this node sits inside its parent.
    pub anchor: Anchor,
    /// The width rule.
    pub width: SizeSpec,
    /// The height rule.
    pub height: SizeSpec,
    /// Flex weight along the parent's main axis.
    pub grow: f32,
}

impl Default for Props {
    fn default() -> Self {
        Self {
            pad: 0.0,
            gap: 0.0,
            align: Anchor::TopLeft,
            anchor: Anchor::TopLeft,
            width: SizeSpec::Auto,
            height: SizeSpec::Auto,
            grow: 0.0,
        }
    }
}
