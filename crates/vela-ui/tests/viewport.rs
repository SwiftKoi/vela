//! A viewport: a window onto content bigger than it (`SCREENS.md §3.2`).
//!
//! Two halves, and both are asserted here because both are invisible in the other's test: the *layout*
//! keeps the content whole and moves it by the scroll fraction, and the *paint* clips the subtree to the
//! box. A viewport that did only the first would draw its content over whatever came next.

use vela_render::{DrawList, Op};
use vela_span::FileId;
use vela_syntax::parse;
use vela_text::{Font, TextEngine};
use vela_ui::{Args, ScreenSet};

/// The bundled face, so text measures to something real.
fn engine() -> TextEngine {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);
    text
}

/// Compiles a one-file source, asserting it parses.
fn set(source: &str) -> ScreenSet {
    let parsed = parse(FileId::from_raw(0), source);
    assert!(
        parsed.diagnostics.is_empty(),
        "the fixture must parse: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
    );
    ScreenSet::from_items(&parsed.program.items)
}

/// A column whose text is taller than the box it is shown in.
///
/// Three lines plus two gaps is about 103 pixels, so a 60-pixel window has to scroll and a 600-pixel one
/// does not — which is what makes the two worth asserting against each other.
const CONTENT: &str = "\
screen s:
    viewport size 60, initial 1.0:
        column gap 10:
            text \"one\"
            text \"two\"
            text \"three\"
";

/// The child keeps the height it asked for, and the window starts at the bottom.
///
/// The content is *not* squeezed into the box — that is the whole difference between a viewport and a
/// plain container, and it is what `initial` has to travel over.
#[test]
fn a_viewport_keeps_its_content_whole() {
    let mut text = engine();
    let laid = set(CONTENT)
        .lay("s", &Args::new(), (1280, 720), &mut text, "sans")
        .expect("the screen is declared");

    let viewport = &laid.frame.children[0];
    assert_eq!(viewport.rect.height, 60.0, "the box is what it was told");

    let content = &viewport.children[0];
    assert!(
        content.rect.height > 60.0,
        "the content was squeezed into the box: {}",
        content.rect.height
    );
}

/// `initial 1.0` shows the bottom: the content is moved up by the whole travel.
#[test]
fn initial_selects_where_the_window_starts() {
    let mut text = engine();
    let bottom = set(CONTENT)
        .lay("s", &Args::new(), (1280, 720), &mut text, "sans")
        .expect("the screen is declared");
    let content_bottom = &bottom.frame.children[0].children[0];
    let travel = content_bottom.rect.height - 60.0;
    assert!(travel > 0.0, "the fixture does not scroll at all");
    assert!(
        (content_bottom.rect.y + travel).abs() < 0.5,
        "`initial 1.0` should show the last {travel} pixels: y = {}",
        content_bottom.rect.y
    );

    // The same screen starting at the top: the content is not moved at all.
    let top = set(&CONTENT.replace("initial 1.0", "initial 0.0"))
        .lay("s", &Args::new(), (1280, 720), &mut text, "sans")
        .expect("the screen is declared");
    let content_top = &top.frame.children[0].children[0];
    assert!(
        content_top.rect.y.abs() < 0.5,
        "`initial 0.0` moved the content: y = {}",
        content_top.rect.y
    );
}

/// Content that fits does not scroll: there is no travel to take a fraction of.
#[test]
fn a_viewport_whose_content_fits_does_not_move_it() {
    let mut text = engine();
    let source = CONTENT.replace("size 60", "size 600");
    let laid = set(&source)
        .lay("s", &Args::new(), (1280, 720), &mut text, "sans")
        .expect("the screen is declared");

    let content = &laid.frame.children[0].children[0];
    assert!(content.rect.height <= 600.0, "the fixture has to fit");
    assert!(
        content.rect.y.abs() < 0.5,
        "a viewport with room to spare moved its content: y = {}",
        content.rect.y
    );
}

/// The subtree is clipped to the box, and the clip is closed before the next sibling.
#[test]
fn a_viewport_clips_its_subtree() {
    let mut text = engine();
    let mut draw = DrawList::new();
    set(CONTENT).draw("s", &Args::new(), (1280, 720), &mut text, "sans", &mut draw);

    let clips: Vec<[f32; 4]> = draw
        .ops()
        .iter()
        .filter_map(|op| match op {
            Op::Clip(clip) => Some(clip.bounds()),
            Op::Unclip | Op::Quad(_) => None,
        })
        .collect();
    assert_eq!(clips.len(), 1, "expected exactly one window: {clips:?}");
    assert_eq!(
        clips[0],
        [0.0, 0.0, 60.0, 60.0],
        "the clip is not the viewport's own box"
    );
    assert!(
        draw.ops().iter().any(|op| matches!(op, Op::Unclip)),
        "the window was never closed"
    );
}
