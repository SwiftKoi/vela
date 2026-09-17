//! The draw list preserves submission order, which is what lets a layer occlude one below it.
//!
//! A frame is layered — a pause panel over a dialogue — and the renderer batches *runs* of
//! consecutive quads of one kind. These assert the order the batching depends on, and that the
//! vertex array follows it, with no GPU.

use vela_render::{Clip, Color, DrawList, GlyphQuad, Op, Quad, RectQuad, build_vertices};

fn rect(x: f32) -> RectQuad {
    RectQuad {
        x,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        color: Color::rgb(255, 0, 0),
    }
}

fn glyph(x: f32) -> GlyphQuad {
    GlyphQuad {
        x,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        uv: [0.0, 0.0, 1.0, 1.0],
        color: Color::rgb(255, 255, 255),
    }
}

/// Rectangles and glyphs interleave in the order they were pushed.
#[test]
fn quads_keep_their_submission_order() {
    let mut draw = DrawList::new();
    draw.push_rect(rect(0.0));
    draw.push_glyph(glyph(20.0));
    draw.push_rect(rect(40.0));

    let xs: Vec<f32> = draw
        .quads()
        .map(|quad| match quad {
            Quad::Rect(rect) => rect.x,
            Quad::Glyph(glyph) => glyph.x,
            Quad::Image(image) => image.x,
        })
        .collect();
    assert_eq!(xs, vec![0.0, 20.0, 40.0]);
    assert_eq!(draw.rect_count(), 2);
    assert_eq!(draw.glyph_count(), 1);
    assert_eq!(draw.len(), 3);
}

/// The vertex array follows the same order, so the batched runs draw the right thing.
#[test]
fn vertices_follow_submission_order() {
    let mut draw = DrawList::new();
    draw.push_glyph(glyph(5.0));
    draw.push_rect(rect(100.0));

    let vertices = build_vertices(&draw);
    assert_eq!(vertices.len(), 8, "two quads, four vertices each");
    assert_eq!(
        vertices[0].position[0], 5.0,
        "the glyph was pushed first and must come first"
    );
    assert_eq!(
        vertices[4].position[0], 100.0,
        "the rectangle follows the glyph"
    );
}

/// A rectangle pushed after a glyph is drawn after it — the layer that could not occlude, fixed.
#[test]
fn a_later_rectangle_follows_an_earlier_glyph() {
    let mut draw = DrawList::new();
    draw.push_glyph(glyph(0.0));
    draw.push_rect(rect(0.0));

    let kinds: Vec<bool> = draw.quads().map(Quad::is_glyph).collect();
    assert_eq!(kinds, vec![true, false]);
}

/// A clip is stamped on the quads inside it and on nothing outside, and it closes again.
///
/// This is what a viewport is: the vertices inside the window carry its rectangle, so the fragment
/// stage can drop what falls outside, and the quads after the pop go back to being unbounded.
#[test]
fn a_clip_is_stamped_on_the_quads_inside_it() {
    let mut draw = DrawList::new();
    draw.push_rect(rect(0.0));
    draw.push_clip(Clip::rect(10.0, 20.0, 100.0, 50.0));
    draw.push_rect(rect(30.0));
    draw.pop_clip();
    draw.push_rect(rect(200.0));

    let vertices = build_vertices(&draw);
    assert_eq!(vertices.len(), 12, "three quads");
    assert_eq!(
        vertices[0].clip,
        Clip::UNBOUNDED.bounds(),
        "a quad before the clip was clipped"
    );
    assert_eq!(
        vertices[4].clip,
        [10.0, 20.0, 110.0, 70.0],
        "the clipped quad did not carry the window"
    );
    assert_eq!(
        vertices[8].clip,
        Clip::UNBOUNDED.bounds(),
        "the clip was not closed"
    );
}

/// Nested clips intersect, so an inner window cannot draw outside the outer one.
#[test]
fn nested_clips_intersect() {
    let mut draw = DrawList::new();
    draw.push_clip(Clip::rect(0.0, 0.0, 100.0, 100.0));
    draw.push_clip(Clip::rect(50.0, 50.0, 100.0, 100.0));
    draw.push_rect(rect(0.0));

    let vertices = build_vertices(&draw);
    assert_eq!(
        vertices[0].clip,
        [50.0, 50.0, 100.0, 100.0],
        "the inner clip was not narrowed by the outer one"
    );
}

/// A clip on its own draws nothing: a list that pushed one and stopped has no pixels in it.
#[test]
fn a_clip_alone_is_not_a_frame() {
    let mut draw = DrawList::new();
    draw.push_clip(Clip::rect(0.0, 0.0, 10.0, 10.0));
    assert!(draw.is_empty());
    assert_eq!(draw.len(), 0, "a clip is not a quad");
    assert!(draw.ops().iter().any(|op| matches!(op, Op::Clip(_))));
}
