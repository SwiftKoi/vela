//! Isolates rectangle geometry: three opaque rects, no text.
use std::path::PathBuf;
use vela_render::{Capture, ClearStage, Color, DrawList, GeometryStage, RectQuad, RenderGraph};

fn main() {
    let out = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("rects.png"), PathBuf::from);
    let mut capture = Capture::new(400, 300).expect("adapter");
    let mut draw = DrawList::new();
    // Red, top-left quadrant. Distinct colours so a mixup is obvious.
    draw.push_rect(RectQuad::from_corners(
        20.0,
        20.0,
        180.0,
        140.0,
        Color::rgb(220, 60, 60),
    ));
    // Green, top-right.
    draw.push_rect(RectQuad::from_corners(
        220.0,
        20.0,
        380.0,
        140.0,
        Color::rgb(60, 200, 90),
    ));
    // Blue, bottom, wide.
    draw.push_rect(RectQuad::from_corners(
        20.0,
        170.0,
        380.0,
        280.0,
        Color::rgb(70, 110, 230),
    ));
    let mut graph = RenderGraph::new();
    graph.push(Box::new(ClearStage {
        color: Color::rgb(10, 10, 14),
    }));
    graph.push(Box::new(GeometryStage));
    capture.save(&graph, &draw, &out).expect("save");
    println!("wrote {}", out.display());
}
