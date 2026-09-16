//! The bridge: a laid-out screen becomes rectangles and glyphs, with no GPU anywhere.
//!
//! These assert the draw list, not pixels. A screenshot says "the frame differs"; a draw list
//! says which rectangle is in the wrong place — and it says it on a machine with no display,
//! which is where this has to run.

use vela_render::{Color, DrawList};
use vela_span::FileId;
use vela_syntax::parse;
use vela_text::{Font, TextEngine};
use vela_ui::{Args, ScreenSet, Value};

fn engine() -> TextEngine {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);
    text
}

fn set(source: &str) -> ScreenSet {
    let parsed = parse(FileId::from_raw(0), source);
    assert!(parsed.diagnostics.is_empty(), "the fixture must parse");
    ScreenSet::from_items(&parsed.program.items)
}

fn args(line: &str) -> Args {
    let mut args = Args::new();
    args.set("name", Value::None);
    args.set("line", Value::Str(line.to_string()));
    args
}

/// A dialogue box with a themed fill and a styled line.
const DIALOGUE: &str = "theme dusk:\n    color bg = 0x10121a\n    color fg = 0xe6e6f0\n\nstyle body:\n    color = theme.fg\n\nscreen dialogue(name: str?, line: str):\n    layer ui\n    box at bottom, background = theme.bg:\n        pad 24\n        column gap 8:\n            if name is not none:\n                text name\n            text line style = body\n";

/// A background box paints a quad in the theme's colour, and the text paints glyphs.
#[test]
fn a_screen_paints_a_rect_and_its_text() {
    let mut text = engine();
    let mut draw = DrawList::new();
    let drawn = set(DIALOGUE).draw(
        "dialogue",
        &args("Hi."),
        (1280, 720),
        &mut text,
        "sans",
        &mut draw,
    );

    assert!(drawn, "the screen was not found");
    assert_eq!(draw.rect_count(), 1, "expected only the box's background");
    let rect = draw.rects().next().expect("the box's background");
    assert_eq!(rect.color, Color::rgb(0x10, 0x12, 0x1a));
    assert!(draw.glyph_count() > 0, "the line produced no glyphs");
}

/// `at bottom` actually puts the box at the bottom, which is the whole point of the anchor.
#[test]
fn a_bottom_anchored_box_sits_at_the_bottom() {
    let mut text = engine();
    let mut draw = DrawList::new();
    set(DIALOGUE).draw(
        "dialogue",
        &args("Hi."),
        (1280, 720),
        &mut text,
        "sans",
        &mut draw,
    );

    let rect = *draw.rects().next().expect("the box's background");
    assert!(rect.y > 0.0, "the box started at the top: y = {}", rect.y);
    let bottom = rect.y + rect.height;
    assert!(
        (bottom - 720.0).abs() < 0.5,
        "the box ended at {bottom}, not at the frame's edge"
    );
}

/// Text is inside the box it belongs to: no glyph starts above the box's own top edge.
#[test]
fn text_lands_inside_its_box() {
    let mut text = engine();
    let mut draw = DrawList::new();
    set(DIALOGUE).draw(
        "dialogue",
        &args("Hi."),
        (1280, 720),
        &mut text,
        "sans",
        &mut draw,
    );

    let box_top = draw.rects().next().expect("the box's background").y;
    for glyph in draw.glyphs() {
        assert!(glyph.y >= box_top, "a glyph escaped the box at {}", glyph.y);
    }
}

/// An undeclared screen draws nothing and says so, rather than silently producing an empty
/// frame a caller would read as "the screen is blank".
#[test]
fn an_undeclared_screen_paints_nothing() {
    let mut text = engine();
    let mut draw = DrawList::new();
    let drawn = set(DIALOGUE).draw(
        "settings",
        &Args::new(),
        (1280, 720),
        &mut text,
        "sans",
        &mut draw,
    );
    assert!(!drawn);
    assert!(draw.is_empty());
}

/// A style's per-state value is what the *focused* control draws.
///
/// This is also what pins the focus numbering: the painter counts action-bearing nodes in tree order,
/// and `focus::hotspots` produces the list a caller moves the cursor over. Two walks that disagreed
/// would recolour the wrong button, so the second button is the one focused here.
#[test]
fn the_focused_control_draws_its_selected_colour() {
    let source = "\
theme dusk:
    color off = 0x101010
    color on = 0xff0000

style item:
    color = theme.off
    selected_color = theme.on

screen menu:
    column:
        button:
            text \"One\" style = item
            action quit()
        button:
            text \"Two\" style = item
            action quit()
";
    let off = Color::rgb(0x10, 0x10, 0x10);
    let on = Color::rgb(0xff, 0x00, 0x00);

    let screens = set(source);
    let mut text = engine();
    let laid = screens
        .lay("menu", &Args::new(), (1280, 720), &mut text, "sans")
        .expect("the menu is declared");
    assert_eq!(laid.hotspots.len(), 2, "one hotspot per button");

    let mut plain = DrawList::new();
    vela_ui::paint(&laid.node, &laid.frame, &mut text, "sans", &mut plain, None);
    assert!(plain.glyph_count() > 0, "the buttons drew no text");
    assert!(
        plain.glyphs().all(|glyph| glyph.color == off),
        "a menu with no focus cursor drew a selected colour"
    );

    let mut focused = DrawList::new();
    vela_ui::paint(
        &laid.node,
        &laid.frame,
        &mut text,
        "sans",
        &mut focused,
        Some(1),
    );
    let colours: Vec<Color> = focused.glyphs().map(|glyph| glyph.color).collect();
    assert!(
        colours.contains(&on),
        "the focused button's text kept its idle colour"
    );
    assert!(
        colours.contains(&off),
        "the unfocused button changed colour as well"
    );
}

/// Building the same screen twice produces the same draw list, byte for byte.
#[test]
fn painting_is_deterministic() {
    let mut first = engine();
    let mut a = DrawList::new();
    set(DIALOGUE).draw(
        "dialogue",
        &args("The rain had stopped."),
        (1280, 720),
        &mut first,
        "sans",
        &mut a,
    );

    let mut second = engine();
    let mut b = DrawList::new();
    set(DIALOGUE).draw(
        "dialogue",
        &args("The rain had stopped."),
        (1280, 720),
        &mut second,
        "sans",
        &mut b,
    );

    assert_eq!(a, b);
}
