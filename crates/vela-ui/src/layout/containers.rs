//! The grid and flow containers.
//!
//! Split from `layout.rs` when that file crossed the 500-line budget, and along a seam that
//! was there anyway: `layout.rs` decides *whether* a container sizes to its content or to
//! what it was offered, and these two decide *how*.
//!
//! Both need the same two things from the parent module — `measure`, to size their children,
//! and `place`, to recurse into one at its final rect.

use crate::layout::{Constraints, Frame, Rect, Size, align, measure, place, placement};
use crate::tree::Node;

/// Measures a grid: each column as wide as its widest cell, each row as tall as its tallest.
pub(crate) fn measure_grid(node: &Node, inner: Constraints, columns: usize) -> (Size, Vec<Size>) {
    let children = measure_children(node, inner);
    let columns = columns.max(1);
    let (widths, heights) = grid_tracks(&children, columns, node.props.gap);
    // Every track carries a gap so the position arithmetic is uniform, which means a grid of
    // N columns has N gaps in its sum and only N-1 in its extent. Subtracting the extra one
    // here is what keeps the reported size equal to the last cell's far edge — a grid that
    // reports a size one gap too large lays out correctly and *measures* wrong, which only
    // shows up when something wraps around it.
    let width = (widths.iter().sum::<f32>() - node.props.gap).max(0.0);
    let height = (heights.iter().sum::<f32>() - node.props.gap).max(0.0);
    (Size::new(width, height), children)
}

/// Measures a flow: children in a line that wraps at the offered width.
pub(crate) fn measure_flow(node: &Node, inner: Constraints) -> (Size, Vec<Size>) {
    let children = measure_children(node, inner);
    let mut widest = 0.0f32;
    let mut line_width = 0.0f32;
    let mut total = 0.0f32;
    let mut line_height = 0.0f32;

    for size in &children {
        // Break *before* the child that does not fit, and never before the first on a line —
        // a child wider than the whole flow gets a line of its own rather than an empty one
        // before it and, in a less careful implementation, a loop that never terminates.
        if line_width > 0.0 && line_width + size.width > inner.max_width {
            widest = widest.max(line_width);
            total += line_height + node.props.gap;
            line_width = 0.0;
            line_height = 0.0;
        }
        line_width += size.width + node.props.gap;
        line_height = line_height.max(size.height);
    }
    widest = widest.max(line_width - node.props.gap);
    total += line_height;

    (Size::new(widest.max(0.0), total.max(0.0)), children)
}

/// The children's desired sizes.
fn measure_children(node: &Node, inner: Constraints) -> Vec<Size> {
    node.children
        .iter()
        .map(|child| measure(child, inner).0)
        .collect()
}

/// Column widths and row heights for a grid, a gap on every track.
fn grid_tracks(children: &[Size], columns: usize, gap: f32) -> (Vec<f32>, Vec<f32>) {
    let rows = children.len().div_ceil(columns).max(1);
    let mut widths = vec![0.0f32; columns];
    let mut heights = vec![0.0f32; rows];

    for (index, size) in children.iter().enumerate() {
        let column = index % columns;
        let row = index / columns;
        widths[column] = widths[column].max(size.width);
        if let Some(height) = heights.get_mut(row) {
            *height = height.max(size.height);
        }
    }
    for width in &mut widths {
        *width += gap;
    }
    for height in &mut heights {
        *height += gap;
    }
    (widths, heights)
}

/// Arranges a grid's cells.
pub(crate) fn arrange_grid(
    node: &Node,
    inner: Rect,
    measured: &[Size],
    columns: usize,
) -> Vec<Frame> {
    let columns = columns.max(1);
    let (widths, heights) = grid_tracks(measured, columns, node.props.gap);

    node.children
        .iter()
        .zip(measured)
        .enumerate()
        .map(|(index, (child, size))| {
            let column = index % columns;
            let row = index / columns;
            let x = inner.x + widths[..column].iter().sum::<f32>();
            let y = inner.y + heights[..row.min(heights.len())].iter().sum::<f32>();
            let track_height = heights.get(row).copied().unwrap_or(0.0);
            let slot = Rect {
                x,
                y,
                width: (widths[column] - node.props.gap).max(size.width),
                height: (track_height - node.props.gap).max(size.height),
            };
            place(
                child,
                align(slot, *size, placement(node.props.align, child)),
            )
        })
        .collect()
}

/// Arranges a flow's children, wrapping at the same points `measure_flow` chose.
pub(crate) fn arrange_flow(node: &Node, inner: Rect, measured: &[Size]) -> Vec<Frame> {
    let mut frames = Vec::with_capacity(node.children.len());
    let mut x = 0.0f32;
    let mut y = 0.0f32;
    let mut line_height = 0.0f32;

    for (child, size) in node.children.iter().zip(measured) {
        if x > 0.0 && x + size.width > inner.width {
            y += line_height + node.props.gap;
            x = 0.0;
            line_height = 0.0;
        }
        frames.push(place(
            child,
            Rect {
                x: inner.x + x,
                y: inner.y + y,
                width: size.width,
                height: size.height,
            },
        ));
        x += size.width + node.props.gap;
        line_height = line_height.max(size.height);
    }
    frames
}
