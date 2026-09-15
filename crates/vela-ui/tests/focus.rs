//! Hotspots: which parts of a laid-out screen can be activated, and where they are.
//!
//! Focus order is tree order (`SCREENS.md §10`), and the rectangle has to be *absolute* because
//! the runtime draws the focus highlight from it. Both are asserted here; the runtime that
//! moves a cursor over this list is `vela-cli`'s, and tested where it lives.

use vela_span::FileId;
use vela_syntax::parse;
use vela_text::{Font, TextEngine};
use vela_ui::{Args, Laid, ScreenSet, focus};

fn engine() -> TextEngine {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);
    text
}

/// The pause screen, stripped to its buttons.
const PAUSE: &str = "screen pause:\n    layer ui\n    box background = 0x10121a:\n        column gap 12:\n            button:\n                text \"Resume\"\n                action close_screen()\n            button:\n                text \"Settings\"\n                action open_screen(settings)\n            button:\n                text \"Quit\"\n                action quit()\n";

fn laid() -> Laid {
    let parsed = parse(FileId::from_raw(0), PAUSE);
    assert!(parsed.diagnostics.is_empty(), "the fixture must parse");
    let mut text = engine();
    ScreenSet::from_items(&parsed.program.items)
        .lay("pause", &Args::new(), (1280, 720), &mut text, "sans")
        .expect("the screen is declared")
}

/// Every button is a hotspot, in tree order, carrying the action it was written with.
#[test]
fn hotspots_are_buttons_in_tree_order_with_their_actions() {
    let laid = laid();
    let names: Vec<&str> = laid
        .hotspots
        .iter()
        .map(|hotspot| hotspot.action.name.as_str())
        .collect();
    assert_eq!(names, vec!["close_screen", "open_screen", "quit"]);
    assert_eq!(laid.hotspots[1].action.args, vec!["settings"]);
}

/// A column of buttons stacks downward, and each has a real rectangle.
#[test]
fn hotspots_stack_down_the_screen() {
    let laid = laid();
    for hotspot in &laid.hotspots {
        assert!(
            hotspot.rect.width > 0.0 && hotspot.rect.height > 0.0,
            "an empty hotspot: {:?}",
            hotspot.rect
        );
    }
    assert!(laid.hotspots[1].rect.y > laid.hotspots[0].rect.y);
    assert!(laid.hotspots[2].rect.y > laid.hotspots[1].rect.y);
    assert_eq!(laid.hotspots[0].rect.y, 0.0, "the first sits at the top");
}

/// A point inside a button hits that button, and a point outside every button hits nothing.
#[test]
fn a_point_hits_the_button_under_it() {
    let laid = laid();
    let first = laid.hotspots[0].rect;

    let hit = focus::at(
        &laid.hotspots,
        first.x + first.width / 2.0,
        first.y + first.height / 2.0,
    )
    .expect("a point in the first button hits it");
    assert_eq!(hit.action.name, "close_screen");

    // The box is only as wide as its text, so far to the right is empty space.
    assert!(focus::at(&laid.hotspots, 1279.0, 0.0).is_none());
}

/// A screen with no buttons has no hotspots, which is what makes it non-navigable rather than
/// a screen with one invisible control.
#[test]
fn a_screen_without_buttons_has_no_hotspots() {
    let parsed = parse(
        FileId::from_raw(0),
        "screen plain:\n    box:\n        text \"Nothing to press\"\n",
    );
    let mut text = engine();
    let laid = ScreenSet::from_items(&parsed.program.items)
        .lay("plain", &Args::new(), (1280, 720), &mut text, "sans")
        .expect("the screen is declared");
    assert!(laid.hotspots.is_empty());
}
