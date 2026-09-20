//! Where a node goes: the slot its parent gives it, and how far from there it is moved.
//!
//! `SCREENS.md §4.2`'s nudge, and the one property that makes it a nudge rather than a coordinate —
//! which is what every test here is about: **measurement does not see it**. The parent is the size it
//! was, the sibling is where it was, and the node itself is simply drawn somewhere else. What it does
//! move is everything a *press* reads: the drawn rectangle and the focusable one are the same rect.

use vela_span::FileId;
use vela_syntax::parse;
use vela_text::{Font, TextEngine};
use vela_ui::{Args, Constraints, Kind, Laid, Node, Rect, ScreenSet, Size, SizeSpec, layout};

/// The bundled face, so text measures to something real.
fn engine() -> TextEngine {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);
    text
}

/// A node that knows how big it is, so a slot's arithmetic is the only arithmetic in the test.
fn measured(width: f32, height: f32) -> Node {
    Node::measured(Size::new(width, height))
}

/// A screen from source, laid out at the reference frame.
fn laid(source: &str, name: &str) -> Laid {
    let parsed = parse(FileId::from_raw(0), source);
    assert!(parsed.diagnostics.is_empty(), "{parsed:?}");
    let mut text = engine();
    ScreenSet::from_items(&parsed.program.items)
        .lay(
            name,
            &Args::new(),
            &vela_ui::ScreenState::new(),
            (1280, 720),
            &mut text,
            "sans",
        )
        .expect("the screen is declared")
}

/// A nudge moves the node and nothing else: the parent keeps its size and the sibling keeps its slot.
#[test]
fn a_nudge_moves_the_node_without_moving_the_layout() {
    let nudged = measured(30.0, 10.0).offset(100.0, 40.0);
    let row = Node::new(Kind::Row, vec![measured(30.0, 10.0), nudged]);

    let frame = layout(&row, Constraints::new(200.0, 200.0));
    assert_eq!(
        frame.size(),
        Size::new(60.0, 10.0),
        "the row measured around the nudge"
    );
    assert_eq!(frame.children[0].rect.x, 0.0, "the sibling moved");
    assert_eq!(
        frame.children[1].rect,
        Rect {
            x: 130.0,
            y: 40.0,
            width: 30.0,
            height: 10.0
        },
        "30 from its slot start, then the nudge"
    );
}

/// A nudged control is focusable where it is drawn, which is what a press reads.
///
/// Two buttons in a stack get the same slot, so the difference between their rectangles is exactly
/// the nudge — and a rect the focus walk disagrees with the paint walk about would be a button drawn
/// in one place and pressed in another.
#[test]
fn a_nudged_control_is_focusable_where_it_is_drawn() {
    const TWO: &str = "screen nudge:\n    stack:\n        button xoffset 40, yoffset = -30:\n            text \"Nudged\"\n            action quit()\n        button:\n            text \"Placed\"\n            action close_screen()\n";

    let laid = laid(TWO, "nudge");
    let nudged = laid.hotspots[0].rect;
    let placed = laid.hotspots[1].rect;
    assert_eq!(
        nudged.x,
        placed.x + 40.0,
        "the focus rect did not move: {nudged:?}"
    );
    assert_eq!(
        nudged.y,
        placed.y - 30.0,
        "the focus rect did not move: {nudged:?}"
    );
}

/// The props reach the node, one axis each, and a nudge says nothing about size.
#[test]
fn a_written_axis_reaches_the_node() {
    const NUDGED: &str = "screen props:\n    box xoffset 40, yoffset = -30:\n        text \"hi\"\n";

    let laid = laid(NUDGED, "props");
    let box_node = &laid.node.children[0];
    assert_eq!(
        box_node.props.offset_x, 40.0,
        "`xoffset 40` was not applied"
    );
    assert_eq!(
        box_node.props.offset_y, -30.0,
        "`yoffset = -30` was not applied"
    );
    assert_eq!(
        box_node.props.width,
        SizeSpec::Auto,
        "a nudge is not a size: nothing said how wide it was"
    );
}
