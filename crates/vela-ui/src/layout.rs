//! The layout solver: measure, then arrange.
//!
//! `SCREENS.md §4.1` prescribes a two-pass constraint solver, and the reason is that one pass
//! cannot do it: a parent that does not yet know how big its children want to be cannot divide
//! space between them, and a child that has not been told what is left cannot know how big to
//! be. Measure answers the first; arrange answers the second.
//!
//! The whole solver is pure — a tree in, rects out — which is deliberate. Layout is the part
//! of a UI that is most often "fixed" by eye against a running window, and a pure function is
//! the one thing a screenshot cannot give you: an answer you can assert on, in a test, with no
//! GPU and no display.

mod containers;

use crate::props::Anchor;
use crate::tree::{Kind, Node, Size};
use containers::{arrange_flow, arrange_grid, measure_flow, measure_grid};

/// A rectangle in the parent's coordinate space.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl Rect {
    /// A rectangle at the origin.
    #[must_use]
    pub fn at_origin(size: Size) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: size.width,
            height: size.height,
        }
    }
}

/// What a parent offers a child.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Constraints {
    /// The largest width the child may take.
    pub max_width: f32,
    /// The largest height the child may take.
    pub max_height: f32,
}

impl Constraints {
    /// Offers a size.
    #[must_use]
    pub const fn new(max_width: f32, max_height: f32) -> Self {
        Self {
            max_width,
            max_height,
        }
    }

    /// Offers a size in both directions.
    #[must_use]
    pub const fn exact(size: Size) -> Self {
        Self::new(size.width, size.height)
    }

    /// The same constraints shrunk by `amount` on every side.
    #[must_use]
    pub fn deflate(self, amount: f32) -> Self {
        Self::new(
            (self.max_width - amount * 2.0).max(0.0),
            (self.max_height - amount * 2.0).max(0.0),
        )
    }

    /// The same constraints shrunk by different amounts horizontally and vertically.
    #[must_use]
    pub fn deflate_axes(self, horizontal: f32, vertical: f32) -> Self {
        Self::new(
            (self.max_width - horizontal * 2.0).max(0.0),
            (self.max_height - vertical * 2.0).max(0.0),
        )
    }
}

/// A laid-out node: where it goes, how big it is, and where its children go.
#[derive(Clone, PartialEq, Debug)]
pub struct Frame {
    /// The node's own rectangle, relative to its parent's content box.
    pub rect: Rect,
    /// The children's frames, in the same order as the node's children.
    pub children: Vec<Frame>,
}

impl Frame {
    /// The frame's size.
    #[must_use]
    pub fn size(&self) -> Size {
        Size::new(self.rect.width, self.rect.height)
    }
}

/// Lays out `node` inside `constraints`.
#[must_use]
pub fn layout(node: &Node, constraints: Constraints) -> Frame {
    let (size, children) = measure(node, constraints);
    let rect = Rect::at_origin(size);
    Frame {
        rect,
        children: arrange(node, &children, size),
    }
}

/// Pass one: how big does this node want to be, and how big does each child want to be?
///
/// Returns the node's own size and its children's desired sizes, in order. The children are
/// re-measured in `place` once their final rect is known — a second pass that costs one walk
/// and removes the need to thread a parallel structure of constraints through every arm,
/// which is the kind of bookkeeping that goes wrong quietly.
///
/// The returned size is *resolved* — padding added and the `size` rule applied — for every
/// node, not only the root. It used to be resolved in `layout` alone, which meant a nested
/// container was measured to its content and drawn at its padding: a `box pad 24` around a
/// 30px label measured 30 and then arranged its children inside `30 - 48`, which clamps to
/// nothing. A container that draws a padding it did not measure for is a container whose
/// children vanish, so the resolution has to travel with the measurement.
pub(crate) fn measure(node: &Node, constraints: Constraints) -> (Size, Vec<Size>) {
    let inner = constraints.deflate(node.props.pad);

    let (content, children) = match &node.kind {
        // A leaf wants its intrinsic size; `resolve` applies any explicit rule on top.
        Kind::Measured { size }
        | Kind::Widget { size, .. }
        | Kind::Text { size, .. }
        | Kind::Image { size, .. } => (*size, Vec::new()),
        // A spacer holds no content, so under `Auto` it wants nothing and relies on `grow`
        // to be given anything. Asking for the whole available extent — which is what using
        // `inner.max_*` as the content did — makes an unweighted spacer eat the row and push
        // its siblings off the edge. `resolve` still honors an explicit size or percentage.
        Kind::Spacer => (Size::ZERO, Vec::new()),
        Kind::Box => {
            let children: Vec<Size> = node
                .children
                .iter()
                .map(|child| measure(child, inner).0)
                .collect();
            (children.first().copied().unwrap_or(Size::ZERO), children)
        }
        // A window: the child is measured against no limit downwards, so a column taller than the box
        // keeps its height — and the viewport asks for the space it was *offered*, because a window that
        // sized itself to its content would never have anything to scroll.
        Kind::Viewport { .. } => {
            let unbounded = Constraints::new(inner.max_width, f32::INFINITY);
            let children: Vec<Size> = node
                .children
                .iter()
                .map(|child| measure(child, unbounded).0)
                .collect();
            let content = Size::new(inner.max_width, inner.max_height);
            (content, children)
        }
        Kind::Row => measure_linear(node, inner, true),
        Kind::Column => measure_linear(node, inner, false),
        Kind::Grid { columns } => measure_grid(node, inner, *columns),
        Kind::Flow => measure_flow(node, inner),
        Kind::Stack => {
            let children: Vec<Size> = node
                .children
                .iter()
                .map(|child| measure(child, inner).0)
                .collect();
            let widest = children
                .iter()
                .fold(0.0f32, |widest, size| widest.max(size.width));
            let tallest = children
                .iter()
                .fold(0.0f32, |tallest, size| tallest.max(size.height));
            (Size::new(widest, tallest), children)
        }
    };

    (resolve(node, constraints, content), children)
}

/// Measures a row or a column: children plus the gaps between them.
fn measure_linear(node: &Node, inner: Constraints, horizontal: bool) -> (Size, Vec<Size>) {
    let children: Vec<Size> = node
        .children
        .iter()
        .map(|child| measure(child, inner).0)
        .collect();

    let gap_total = node.props.gap * (children.len().saturating_sub(1)) as f32;
    let asked: f32 = children.iter().map(|size| main_of(*size, horizontal)).sum();
    let cross = children
        .iter()
        .map(|size| cross_of(*size, horizontal))
        .fold(0.0f32, f32::max);

    // A row or column containing a child that grows **fills what it was offered**, because
    // that child is going to consume the leftover. Measuring to the content instead would
    // leave no leftover to distribute — the container would ask for exactly what its static
    // children need, and the growing one would get nothing. Which is what happened: `grow`
    // worked for a child of a fixed-size parent and silently did nothing for a child of an
    // auto-sized one.
    let grows = node.children.iter().any(|child| child.props.grow > 0.0);
    let main = if grows {
        main_of(Size::new(inner.max_width, inner.max_height), horizontal)
    } else {
        asked
    };

    let size = if horizontal {
        Size::new(main + gap_total, cross)
    } else {
        Size::new(cross, main + gap_total)
    };
    (size, children)
}

/// The extent along a linear container's stacking axis.
fn main_of(size: Size, horizontal: bool) -> f32 {
    if horizontal { size.width } else { size.height }
}

/// The extent across a linear container's stacking axis.
fn cross_of(size: Size, horizontal: bool) -> f32 {
    if horizontal { size.height } else { size.width }
}

/// The node's own size: padding around whatever the content needed, then any explicit rule.
fn resolve(node: &Node, constraints: Constraints, content: Size) -> Size {
    let padded = Size::new(
        content.width + node.props.pad * 2.0,
        content.height + node.props.pad * 2.0,
    );
    Size::new(
        node.props
            .width
            .resolve(constraints.max_width, padded.width),
        node.props
            .height
            .resolve(constraints.max_height, padded.height),
    )
}

/// Pass two: assign rectangles.
fn arrange(node: &Node, measured: &[Size], size: Size) -> Vec<Frame> {
    let inner = Rect {
        x: node.props.pad,
        y: node.props.pad,
        width: (size.width - node.props.pad * 2.0).max(0.0),
        height: (size.height - node.props.pad * 2.0).max(0.0),
    };

    match &node.kind {
        Kind::Measured { .. }
        | Kind::Widget { .. }
        | Kind::Text { .. }
        | Kind::Image { .. }
        | Kind::Spacer => Vec::new(),
        Kind::Box => node
            .children
            .first()
            .zip(measured.first())
            .map(|(child, child_size)| {
                place(
                    child,
                    align(inner, *child_size, placement(node.props.align, child)),
                )
            })
            .into_iter()
            .collect(),
        // The child keeps the size it asked for — *not* clamped to the box, which is what `align` would
        // do and what would leave nothing to scroll — and is moved up by however far the window has
        // travelled. `initial` is the fraction of that travel, so `1.0` shows the bottom.
        Kind::Viewport { initial } => node
            .children
            .first()
            .zip(measured.first())
            .map(|(child, child_size)| {
                let travel = (child_size.height - inner.height).max(0.0);
                let offset = travel * initial.clamp(0.0, 1.0);
                place(
                    child,
                    Rect {
                        x: inner.x,
                        y: inner.y - offset,
                        width: child_size.width,
                        height: child_size.height,
                    },
                )
            })
            .into_iter()
            .collect(),
        Kind::Row => arrange_linear(node, inner, measured, true),
        Kind::Column => arrange_linear(node, inner, measured, false),
        Kind::Grid { columns } => arrange_grid(node, inner, measured, *columns),
        Kind::Flow => arrange_flow(node, inner, measured),
        Kind::Stack => node
            .children
            .iter()
            .zip(measured)
            .map(|(child, child_size)| {
                place(
                    child,
                    align(inner, *child_size, placement(node.props.align, child)),
                )
            })
            .collect(),
    }
}

/// Arranges a row or a column, distributing what the children asked for plus any flex.
fn arrange_linear(node: &Node, inner: Rect, measured: &[Size], horizontal: bool) -> Vec<Frame> {
    let gap_total = node.props.gap * (measured.len().saturating_sub(1)) as f32;
    let available = main_of(Size::new(inner.width, inner.height), horizontal);
    let asked: f32 = measured.iter().map(|size| main_of(*size, horizontal)).sum();
    let leftover = (available - asked - gap_total).max(0.0);

    // Flex is shared by weight, and only when there *is* leftover: a row whose children
    // already overflow does not get to shrink them, because shrinking is how a layout starts
    // silently clipping content.
    let total_grow: f32 = node.children.iter().map(|child| child.props.grow).sum();
    let share = if total_grow > 0.0 {
        leftover / total_grow
    } else {
        0.0
    };

    let mut cursor = 0.0f32;
    let mut frames = Vec::with_capacity(node.children.len());
    for (child, child_size) in node.children.iter().zip(measured) {
        let main = main_of(*child_size, horizontal) + child.props.grow * share;
        // The cross axis is the container's: a child of a row is offered the row's height,
        // and its own anchor decides where it sits inside that.
        let cross = cross_of(Size::new(inner.width, inner.height), horizontal);
        let slot = if horizontal {
            Rect {
                x: inner.x + cursor,
                y: inner.y,
                width: main,
                height: cross,
            }
        } else {
            Rect {
                x: inner.x,
                y: inner.y + cursor,
                width: cross,
                height: main,
            }
        };
        cursor += main + node.props.gap;

        let anchor = placement(node.props.align, child);
        let placed = if child.props.grow > 0.0 {
            grown_slot(slot, *child_size, anchor, horizontal)
        } else {
            align(slot, *child_size, anchor)
        };
        frames.push(place(child, placed));
    }
    frames
}

/// The slot a grown child occupies: the whole main axis, its own cross axis.
///
/// `grow` is about the parent's *main* axis — a row's width, a column's height. Stretching
/// the cross axis too would be a different prop (`stretch_x`/`stretch_y`, `SCREENS.md §4.2`),
/// and conflating them means a grown child silently becomes full-height the moment it is
/// given any weight, which then has to be undone with an explicit size.
fn grown_slot(slot: Rect, child_size: Size, anchor: Anchor, horizontal: bool) -> Rect {
    let cross_available = cross_of(Size::new(slot.width, slot.height), horizontal);
    let cross = cross_of(child_size, horizontal).min(cross_available);
    let free = (cross_available - cross).max(0.0);
    let offset = free
        * if horizontal {
            anchor.vertical()
        } else {
            anchor.horizontal()
        };

    if horizontal {
        Rect {
            x: slot.x,
            y: slot.y + offset,
            width: slot.width,
            height: cross,
        }
    } else {
        Rect {
            x: slot.x + offset,
            y: slot.y,
            width: cross,
            height: slot.height,
        }
    }
}

/// The anchor that places `child` inside the slot its parent gives it.
///
/// `SCREENS.md §4.2` defines a node's `anchor` as *self-positioning in the parent* and a
/// container's `align` as *where children sit*. Both can be written, so the two have to
/// compose: a child that names its own anchor wins, and one that does not takes the parent's
/// `align`. `TopLeft` is the default of both, which means "unset" and "written as top-left"
/// are the same value — the price of a default, resolved the way a first screen expects.
pub(crate) fn placement(parent: Anchor, child: &Node) -> Anchor {
    if child.props.anchor == Anchor::default() {
        parent
    } else {
        child.props.anchor
    }
}

/// Places `child_size` inside `slot` according to `anchor`.
pub(crate) fn align(slot: Rect, child_size: Size, anchor: Anchor) -> Rect {
    if anchor == Anchor::Stretch {
        return slot;
    }
    let free_x = (slot.width - child_size.width).max(0.0);
    let free_y = (slot.height - child_size.height).max(0.0);
    Rect {
        x: slot.x + free_x * anchor.horizontal(),
        y: slot.y + free_y * anchor.vertical(),
        width: child_size.width.min(slot.width),
        height: child_size.height.min(slot.height),
    }
}

/// Recurses into a child at its final rect.
///
/// The child's own size is the rect it was *given*, not the size it asked for: a container
/// that ended up larger than its content must lay its children out against the space it has,
/// or they would be arranged inside a box smaller than the one drawn.
pub(crate) fn place(child: &Node, rect: Rect) -> Frame {
    let size = Size::new(rect.width, rect.height);
    let (_, measured) = measure(child, Constraints::exact(size));
    Frame {
        rect,
        children: arrange(child, &measured, size),
    }
}
