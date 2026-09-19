//! Hotspots: the action-bearing nodes of a laid-out screen, in focus order.
//!
//! `SCREENS.md §10` says focus order is derived from tree order and `§11` abstracts input to
//! semantic actions. This is that derivation: walk a laid-out tree and produce every node that
//! carries an action, with the absolute rectangle it occupies. The runtime moves a cursor over
//! this list and activates the one under it; a pointer hit test is the same list with a point.
//!
//! The rectangles are **absolute**, not relative to the parent. The focus highlight is drawn by
//! the runtime from this list, and a relative rectangle would need the whole ancestor chain to
//! place — which is the bookkeeping the layout pass already did once.

use crate::actions::Action;
use crate::layout::{Frame, Rect};
use crate::tree::{Kind, Node};

/// An action-bearing node, placed.
#[derive(Clone, PartialEq, Debug)]
pub struct Hotspot {
    /// Where it is, in frame coordinates.
    pub rect: Rect,
    /// What activating it does.
    pub action: Action,
    /// What it *says*, as one string: every word drawn inside it, in tree order.
    ///
    /// This is how a control is named, and it is Ren'Py's own answer (`testfocus.find_focus` matches
    /// a pattern against a widget's text) rather than the accessibility tree's `label` prop: what a
    /// person clicking "Start" means is the button that *reads* Start, and at this point the string
    /// is the resolved one, so a caption that came from data is named by what it says.
    pub label: String,
}

/// Every action-bearing node under `root`, in focus order (tree order).
#[must_use]
pub fn hotspots(root: &Node, frame: &Frame) -> Vec<Hotspot> {
    let mut out = Vec::new();
    walk(root, frame, 0.0, 0.0, &mut out);
    out
}

/// The hotspot under a point, if any — the pointer path, over the same list the keyboard walks.
#[must_use]
pub fn at(hotspots: &[Hotspot], x: f32, y: f32) -> Option<&Hotspot> {
    hotspots.iter().find(|hotspot| within(&hotspot.rect, x, y))
}

/// Walks the tree and its frame together, carrying the absolute origin.
fn walk(node: &Node, frame: &Frame, parent_x: f32, parent_y: f32, out: &mut Vec<Hotspot>) {
    let left = parent_x + frame.rect.x;
    let top = parent_y + frame.rect.y;

    if let Some(action) = &node.action {
        out.push(Hotspot {
            rect: Rect {
                x: left,
                y: top,
                width: frame.rect.width,
                height: frame.rect.height,
            },
            action: action.clone(),
            label: words(node),
        });
    }

    for (child, child_frame) in node.children.iter().zip(&frame.children) {
        walk(child, child_frame, left, top, out);
    }
}

/// Every word drawn inside a node, in tree order.
#[must_use]
pub fn words(node: &Node) -> String {
    let mut out = String::new();
    collect(node, &mut out);
    out.trim().to_string()
}

/// Appends a node's text and its children's.
fn collect(node: &Node, out: &mut String) {
    if let Kind::Text { text, .. } = &node.kind {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(text);
    }
    for child in &node.children {
        collect(child, out);
    }
}

/// Whether a point is inside a rectangle.
///
/// Half-open: the right and bottom edges belong to the next rectangle, so two adjacent
/// hotspots do not both claim the pixel between them.
fn within(rect: &Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}
