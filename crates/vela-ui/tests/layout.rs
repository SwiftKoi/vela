//! Layout, asserted rather than eyeballed.
//!
//! Every one of these is a property rather than a picture. A screenshot tells you a layout
//! looks wrong; it cannot tell you *which* of forty rectangles is one pixel out, and it cannot
//! tell you at all on a machine with no display.

use vela_ui::props::Anchor;
use vela_ui::{Constraints, Kind, Node, Size, SizeSpec, layout};

fn layout_in(node: &Node, width: f32, height: f32) -> vela_ui::Frame {
    layout(node, Constraints::new(width, height))
}

fn measured(width: f32, height: f32) -> Node {
    Node::measured(Size::new(width, height))
}

/// A row places its children side by side, in order, with the gap between them.
#[test]
fn a_row_places_children_left_to_right() {
    let row = Node::new(
        Kind::Row,
        vec![
            measured(30.0, 10.0),
            measured(20.0, 10.0),
            measured(10.0, 10.0),
        ],
    )
    .gap(5.0);

    let frame = layout_in(&row, 200.0, 100.0);
    assert_eq!(frame.children.len(), 3);
    assert_eq!(frame.children[0].rect.x, 0.0);
    assert_eq!(frame.children[1].rect.x, 35.0, "30 + a 5px gap");
    assert_eq!(frame.children[2].rect.x, 60.0, "30 + 20 + two 5px gaps");
    assert_eq!(frame.size().width, 70.0, "30 + 20 + 10 + two 5px gaps");
    assert_eq!(frame.size().height, 10.0, "the tallest child");
}

/// A column is the same, rotated.
#[test]
fn a_column_places_children_top_to_bottom() {
    let column = Node::new(
        Kind::Column,
        vec![measured(10.0, 30.0), measured(10.0, 20.0)],
    )
    .gap(4.0);

    let frame = layout_in(&column, 200.0, 200.0);
    assert_eq!(frame.children[0].rect.y, 0.0);
    assert_eq!(frame.children[1].rect.y, 34.0);
    assert_eq!(frame.size().height, 54.0);
    assert_eq!(frame.size().width, 10.0, "the widest child");
}

/// Padding moves the children in and grows the container to match.
#[test]
fn padding_insets_children_and_grows_the_container() {
    let row = Node::new(Kind::Row, vec![measured(30.0, 10.0)]).pad(8.0);

    let frame = layout_in(&row, 200.0, 100.0);
    assert_eq!(frame.children[0].rect.x, 8.0);
    assert_eq!(frame.children[0].rect.y, 8.0);
    assert_eq!(frame.size(), Size::new(46.0, 26.0), "30+16 by 10+16");
}

/// A nested container's padding is part of its measured size, so its children still have room.
///
/// This is the bug `measure` grew a `resolve` for: only the root's padding used to be added,
/// so a `box pad 24` measured its content and then arranged children inside `content - 48`,
/// which clamps to nothing. A padded container whose children vanish passes every test that
/// only checks the root, which is exactly what the old suite did.
#[test]
fn a_nested_container_measures_its_padding() {
    let box_node = Node::new(Kind::Box, vec![measured(30.0, 10.0)]).pad(8.0);
    let outer = Node::new(Kind::Stack, vec![box_node]);

    let frame = layout_in(&outer, 200.0, 100.0);
    assert_eq!(
        frame.children[0].size(),
        Size::new(46.0, 26.0),
        "the box measured without its padding"
    );
    assert_eq!(
        frame.children[0].children[0].rect.x, 8.0,
        "child ignores pad"
    );
    assert_eq!(
        frame.children[0].children[0].size(),
        Size::new(30.0, 10.0),
        "the padded child collapsed"
    );
}

/// `grow` shares what is left over, by weight.
#[test]
fn grow_distributes_the_leftover_by_weight() {
    let row = Node::new(
        Kind::Row,
        vec![
            measured(10.0, 10.0),
            measured(10.0, 10.0).grow(1.0),
            measured(10.0, 10.0).grow(3.0),
        ],
    );

    // 100 wide, 30 asked for, 70 left: one part gets 17.5, three parts get 52.5.
    let frame = layout_in(&row, 100.0, 50.0);
    assert!((frame.children[1].rect.width - 27.5).abs() < 0.001);
    assert!((frame.children[2].rect.width - 62.5).abs() < 0.001);
    assert!((frame.size().width - 100.0).abs() < 0.001);
}

/// The total width of a filled row is exactly the width offered — no rounding drift.
#[test]
fn a_filled_row_ends_exactly_at_its_edge() {
    let row = Node::new(
        Kind::Row,
        vec![
            measured(0.0, 10.0).grow(1.0),
            measured(0.0, 10.0).grow(1.0),
            measured(0.0, 10.0).grow(1.0),
        ],
    );

    let frame = layout_in(&row, 300.0, 50.0);
    let last = frame.children.last().expect("three children");
    let edge = last.rect.x + last.rect.width;
    assert!((edge - 300.0).abs() < 0.001, "the row ends at {edge}");
}

/// A container never hands a child more than it has, and never a negative size.
#[test]
fn a_container_does_not_overflow_itself() {
    let row = Node::new(
        Kind::Row,
        vec![measured(500.0, 500.0), measured(500.0, 500.0)],
    );

    let frame = layout_in(&row, 100.0, 100.0);
    for child in &frame.children {
        assert!(child.rect.width <= 100.0, "{} wide", child.rect.width);
        assert!(child.rect.height <= 100.0, "{} tall", child.rect.height);
    }
}

/// Anchors place a child in the space it was given; the space itself does not move.
#[test]
fn anchors_position_within_the_slot() {
    for (anchor, x, y) in [
        (Anchor::TopLeft, 0.0, 0.0),
        (Anchor::Center, 45.0, 20.0),
        (Anchor::BottomRight, 90.0, 40.0),
    ] {
        let mut box_node = Node::new(Kind::Box, vec![measured(10.0, 10.0)]);
        box_node.props.align = anchor;
        box_node.props.width = SizeSpec::Fixed(100.0);
        box_node.props.height = SizeSpec::Fixed(50.0);

        let frame = layout_in(&box_node, 200.0, 200.0);
        assert_eq!(frame.children[0].rect.x, x, "{anchor:?}");
        assert_eq!(frame.children[0].rect.y, y, "{anchor:?}");
        assert_eq!(
            frame.children[0].rect.width, 10.0,
            "{anchor:?} kept its size"
        );
    }
}

/// A child's own `anchor` self-positions it in the slot its parent gives it.
///
/// `SCREENS.md §4.2` defines `anchor` as self-positioning, and until now the solver ignored it:
/// `at bottom` on a dialogue box had no effect. The parent's `align` is the fallback, not the
/// override, which is why the two can both be written.
#[test]
fn a_childs_own_anchor_self_positions_it() {
    let mut child = measured(10.0, 10.0);
    child.props.anchor = Anchor::BottomRight;
    let stack =
        Node::new(Kind::Stack, vec![child]).size(SizeSpec::Fixed(100.0), SizeSpec::Fixed(50.0));

    let frame = layout_in(&stack, 200.0, 200.0);
    assert_eq!(
        frame.children[0].rect.x, 90.0,
        "right edge of a 100-wide slot"
    );
    assert_eq!(frame.children[0].rect.y, 40.0, "bottom of a 50-tall slot");
}

/// A child that names no anchor takes the parent's `align`, so both props still compose.
#[test]
fn a_parents_align_positions_a_default_child() {
    let mut box_node = Node::new(Kind::Box, vec![measured(10.0, 10.0)]);
    box_node.props.align = Anchor::Center;
    box_node.props.width = SizeSpec::Fixed(100.0);
    box_node.props.height = SizeSpec::Fixed(50.0);

    let frame = layout_in(&box_node, 200.0, 200.0);
    assert_eq!(frame.children[0].rect.x, 45.0);
    assert_eq!(frame.children[0].rect.y, 20.0);
}

/// A stack overlaps its children and is as big as the largest.
#[test]
fn a_stack_overlaps_and_sizes_to_the_largest() {
    let stack = Node::new(
        Kind::Stack,
        vec![measured(30.0, 10.0), measured(10.0, 40.0)],
    );

    let frame = layout_in(&stack, 200.0, 200.0);
    assert_eq!(frame.size(), Size::new(30.0, 40.0));
    assert_eq!(frame.children[0].rect.x, 0.0);
    assert_eq!(frame.children[1].rect.x, 0.0, "overlapped, not offset");
}

/// A fixed size wins over the content's wishes, and a percentage resolves against the offer.
#[test]
fn explicit_sizes_override_the_content() {
    let fixed = Node::new(Kind::Row, vec![measured(10.0, 10.0)])
        .size(SizeSpec::Fixed(80.0), SizeSpec::Auto);
    assert_eq!(layout_in(&fixed, 200.0, 200.0).size().width, 80.0);

    let half = Node::new(Kind::Row, vec![measured(10.0, 10.0)])
        .size(SizeSpec::Percent(0.5), SizeSpec::Auto);
    assert_eq!(layout_in(&half, 200.0, 200.0).size().width, 100.0);
}

/// Nesting is where a layout solver usually goes wrong: the inner container must be laid out
/// against the rect it was *given*, not the size it asked for, or its children sit in a box
/// smaller than the one drawn.
#[test]
fn a_grown_child_lays_its_own_children_out_against_its_final_rect() {
    // A row inside a row, so `grow` applies along the axis this is about. The cross axis does
    // not stretch by default — `SCREENS.md §4.2` makes that an explicit `stretch_*` prop — so
    // nesting a row in a *column* would test something else entirely.
    let inner = Node::new(
        Kind::Row,
        vec![measured(10.0, 10.0).grow(1.0), measured(10.0, 10.0)],
    );
    let outer = Node::new(Kind::Row, vec![inner.grow(1.0)]);

    let frame = layout_in(&outer, 200.0, 100.0);
    let inner_frame = &frame.children[0];
    assert!(
        (inner_frame.rect.width - 200.0).abs() < 0.001,
        "the inner row filled: {}",
        inner_frame.rect.width
    );

    let last = inner_frame.children.last().expect("two grandchildren");
    let edge = last.rect.x + last.rect.width;
    assert!(
        (edge - 200.0).abs() < 0.001,
        "the grandchild ended at {edge}, not at its parent's edge"
    );
}

/// A grid's columns are as wide as their widest cell, and its rows as tall as their tallest.
///
/// Not equal tracks: a grid of labels and values wants each column to fit its own content,
/// which is why `columns` is the only thing a grid decides.
#[test]
fn a_grid_sizes_tracks_to_their_cells() {
    let grid = Node::new(
        Kind::Grid { columns: 2 },
        vec![
            measured(10.0, 5.0),
            measured(30.0, 5.0),
            measured(20.0, 40.0),
            measured(5.0, 5.0),
        ],
    );

    let frame = layout_in(&grid, 500.0, 500.0);
    // Column 0 holds 10 and 20, so it is 20 wide; column 1 holds 30 and 5, so it is 30.
    assert_eq!(frame.size(), Size::new(50.0, 45.0));
    // Row 0 is 5 tall, row 1 is 40.
    assert_eq!(frame.children[2].rect.x, 0.0, "third cell starts column 0");
    assert_eq!(frame.children[2].rect.y, 5.0, "and row 1");
    assert_eq!(
        frame.children[3].rect.x, 20.0,
        "fourth cell starts column 1"
    );
}

/// A grid's tracks account for the gap exactly once each.
#[test]
fn a_grid_accounts_for_its_gaps() {
    let grid = Node::new(
        Kind::Grid { columns: 2 },
        vec![
            measured(10.0, 10.0),
            measured(10.0, 10.0),
            measured(10.0, 10.0),
        ],
    )
    .gap(6.0);

    let frame = layout_in(&grid, 500.0, 500.0);
    // Two columns of 10 with one 6px gap, two rows of 10 with one 6px gap.
    assert_eq!(frame.size(), Size::new(26.0, 26.0));
    assert_eq!(frame.children[1].rect.x, 16.0, "second column after a gap");
    assert_eq!(frame.children[2].rect.y, 16.0, "second row after a gap");
}

/// A flow wraps when the line is full.
#[test]
fn a_flow_wraps_at_the_offered_width() {
    let flow = Node::new(
        Kind::Flow,
        vec![
            measured(40.0, 10.0),
            measured(40.0, 10.0),
            measured(40.0, 10.0),
        ],
    )
    .gap(5.0);

    // 100 wide holds two of 40 with a gap between (85), not three (130).
    let frame = layout_in(&flow, 100.0, 500.0);
    assert_eq!(frame.children[0].rect.y, 0.0);
    assert_eq!(frame.children[1].rect.y, 0.0);
    assert_eq!(frame.children[2].rect.y, 15.0, "the third wrapped");
    assert_eq!(frame.children[2].rect.x, 0.0, "and started a new line");
    assert_eq!(frame.size().height, 25.0, "two lines of 10 and a 5px gap");
}

/// A child wider than the whole flow gets a line of its own.
///
/// Otherwise it would break before itself onto an empty line, fail to fit there too, and
/// break again — the loop that never terminates, or the empty line that looks like a bug.
#[test]
fn a_flow_gives_an_oversized_child_its_own_line() {
    let flow = Node::new(
        Kind::Flow,
        vec![
            measured(10.0, 10.0),
            measured(500.0, 10.0),
            measured(10.0, 10.0),
        ],
    );

    let frame = layout_in(&flow, 100.0, 500.0);
    assert_eq!(frame.children.len(), 3);
    assert_eq!(frame.children[1].rect.y, 10.0, "the wide one wrapped");
    assert_eq!(frame.children[2].rect.y, 20.0, "and the next was pushed on");
}

/// An empty container is zero-sized rather than panicking, which is what a screen with a
/// conditional list looks like before anything is added to it.
#[test]
fn empty_containers_layout_to_nothing() {
    for kind in [
        Kind::Row,
        Kind::Column,
        Kind::Stack,
        Kind::Flow,
        Kind::Grid { columns: 3 },
    ] {
        let frame = layout_in(&Node::new(kind.clone(), Vec::new()), 100.0, 100.0);
        assert_eq!(frame.size(), Size::ZERO, "{kind:?}");
        assert!(frame.children.is_empty(), "{kind:?}");
    }
}

/// A spacer with no `grow` takes no space.
///
/// `Node::spacer()` sets `grow` for you, but a spacer is a *kind* — writing one by hand
/// without a weight is legal, and it must mean "nothing here". Asking for the whole available
/// extent is the opposite, and it is invisible until a row containing one pushes its siblings
/// off the edge.
#[test]
fn an_ungrown_spacer_takes_no_space() {
    let row = Node::new(
        Kind::Row,
        vec![
            measured(10.0, 10.0),
            Node::new(Kind::Spacer, Vec::new()),
            measured(10.0, 10.0),
        ],
    );

    let frame = layout_in(&row, 200.0, 50.0);
    assert_eq!(frame.size().width, 20.0, "the spacer claimed space");
    assert_eq!(frame.children[1].rect.width, 0.0);
    assert_eq!(
        frame.children[2].rect.x, 10.0,
        "the third child was pushed along"
    );
}

/// `grow` is along the parent's main axis only. The cross axis is `stretch_*`'s job.
#[test]
fn grow_does_not_stretch_the_cross_axis() {
    let row = Node::new(Kind::Row, vec![measured(10.0, 10.0).grow(1.0)]);

    let frame = layout_in(&row, 200.0, 100.0);
    assert_eq!(frame.children[0].rect.width, 200.0, "it grew along the row");
    assert_eq!(
        frame.children[0].rect.height, 10.0,
        "but kept its own height"
    );
}
