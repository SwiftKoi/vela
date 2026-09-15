//! The stages the engine ships with.
//!
//! Both are ordinary `Stage` implementations. Nothing marks them as special or built-in, and
//! the graph does not know they exist — which is what makes a third one a new file rather
//! than an edit to the graph.

use crate::draw::Color;
use crate::graph::{Frame, Stage};

/// Fills the frame with a colour.
pub struct ClearStage {
    /// The fill.
    pub color: Color,
}

impl Stage for ClearStage {
    fn name(&self) -> &'static str {
        "clear"
    }

    fn run(&self, frame: &mut Frame<'_>) {
        frame.renderer.clear(frame, self.color);
    }
}

/// Draws the frame's draw list.
pub struct GeometryStage;

impl Stage for GeometryStage {
    fn name(&self) -> &'static str {
        "geometry"
    }

    fn run(&self, frame: &mut Frame<'_>) {
        let draw = frame.draw.clone();
        frame.renderer.draw(frame, &draw);
    }
}
