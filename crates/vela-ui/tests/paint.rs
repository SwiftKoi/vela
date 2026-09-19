//! The bridge: a laid-out screen becomes rectangles and glyphs, with no GPU anywhere.
//!
//! These assert the draw list, not pixels. A screenshot says "the frame differs"; a draw list
//! says which rectangle is in the wrong place — and it says it on a machine with no display,
//! which is where this has to run.

use vela_render::{Color, DrawList};
use vela_span::FileId;
use vela_syntax::parse;
use vela_text::{Font, TextEngine};
use vela_ui::{Args, ImageTable, Picture, ScreenSet, Value};

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

/// A control whose action writes what the store holds draws as `selected`, and so does its subtree.
///
/// This is what makes a radio row show the current choice without the author writing a companion test
/// (`SCREENS.md §5.1`): the two options below are identical but for the value they write, so a frame in
/// which one of them is accent-coloured and the other is not is a frame in which the *store* decided.
#[test]
fn a_control_that_writes_the_stores_value_draws_as_selected() {
    const RADIO: &str = "theme dusk:\n    color fg = 0xe6e6f0\n    color accent = 0x00ff00\n\nstyle radio:\n    color = theme.fg\n    selected_color = theme.accent\n\nscreen display:\n    column gap 4:\n        button:\n            action preference(\"display_mode\", \"window\")\n            text \"Window\" style = radio\n        button:\n            action preference(\"display_mode\", \"fullscreen\")\n            text \"Fullscreen\" style = radio\n";

    /// The colour of the first glyph drawn at all, which is the first option's text.
    fn first_color(source: &str, stored: Option<vela_world::Value>) -> Color {
        let mut text = engine();
        let mut draw = DrawList::new();
        let mut args = Args::new();
        if let Some(value) = stored {
            let mut preferences = vela_world::Preferences::new();
            preferences.set("display_mode", value);
            args.set_preferences(preferences);
        }
        assert!(
            set(source).draw("display", &args, (1280, 720), &mut text, "sans", &mut draw),
            "the screen was not found"
        );
        // Glyphs come out in draw order, so the first is the first option's text.
        draw.glyphs()
            .next()
            .expect("the option's text drew no glyphs")
            .color
    }

    // Nobody has chosen: `display_mode` is declared `window`, so the *default's* option is the marked one.
    assert_eq!(first_color(RADIO, None), Color::rgb(0x00, 0xff, 0x00));
    // A player who chose the other: the mark moves with the store, without the screen changing a word.
    assert_eq!(
        first_color(RADIO, Some(vela_world::Value::Str("fullscreen".into()))),
        Color::rgb(0xe6, 0xe6, 0xf0)
    );
}

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
        .lay(
            "menu",
            &Args::new(),
            &vela_ui::ScreenState::new(),
            (1280, 720),
            &mut text,
            "sans",
        )
        .expect("the menu is declared");
    assert_eq!(laid.hotspots.len(), 2, "one hotspot per button");

    let mut plain = DrawList::new();
    vela_ui::paint(
        &laid.node,
        &laid.frame,
        &mut text,
        "sans",
        &mut plain,
        None,
        &ImageTable::new(),
    );
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
        &ImageTable::new(),
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

/// A style's font is the one its text is *measured* with, not merely stored on the node.
///
/// The observable is the engine's layout cache, which is keyed by everything that changes the answer —
/// the font among them. Two identical texts differing only in the font their styles name therefore
/// take two entries; if the name never reached the engine they would share one.
#[test]
fn a_style_font_measures_the_text() {
    let source = "\
theme dusk:
    font ui = \"sans\"
    font kanji = \"cjk\"

style kanji:
    font = theme.kanji

screen s:
    column:
        text \"x\" style = kanji
        text \"x\"
";
    let mut text = engine();
    // The one bundled face under a second name: a different cache *key*, the same glyphs, which is
    // exactly enough to see whether the style's name reached the engine.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let face = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    text.add_font("cjk", face);

    set(source)
        .build(
            "s",
            &Args::new(),
            &mut vela_ui::ScreenState::new(),
            &mut text,
            "sans",
            1280.0,
        )
        .expect("the screen is declared");
    assert_eq!(
        text.cached_layouts(),
        2,
        "the two texts shared a layout, so the style's font never reached the engine"
    );
}

/// A style naming a face the engine does not carry falls back rather than drawing nothing.
///
/// `SCREENS.md §5`: the same silent-fallback rule a `style_prefix` follows, so a project may write the
/// token before the face behind it is wired up — and a screen whose text vanished would be the worse
/// answer.
#[test]
fn an_unroutable_font_falls_back() {
    let source = "\
theme dusk:
    font kanji = \"SourceHanSans\"

style kanji:
    font = theme.kanji

screen s:
    text \"Still here.\" style = kanji
";
    let mut text = engine();
    let mut draw = DrawList::new();
    let drawn = set(source).draw("s", &Args::new(), (1280, 720), &mut text, "sans", &mut draw);
    assert!(drawn, "the screen was not found");
    assert!(draw.glyph_count() > 0, "the text vanished with its font");
}

/// A picture is drawn at the rectangle layout gave it, sampling the texture the platform supplied.
///
/// The name belongs to the screen and the texture to the platform, and this is the one place the two
/// meet (`SCREENS.md §3`, `images.rs`). The size comes from the table rather than from the node because
/// the picture is pixels: a screen cannot know how big it is, and the party that uploaded it does.
#[test]
fn a_picture_paints_its_texture() {
    let images = ImageTable::from_entries([(
        "bg.room".to_string(),
        Picture {
            texture: 7,
            width: 320,
            height: 180,
        },
    )]);
    let source = "screen s:\n    image bg.room\n";
    let mut text = engine();
    let laid = set(source)
        .with_images(images.clone())
        .lay(
            "s",
            &Args::new(),
            &vela_ui::ScreenState::new(),
            (1280, 720),
            &mut text,
            "sans",
        )
        .expect("the screen is declared");

    let mut draw = DrawList::new();
    vela_ui::paint(
        &laid.node,
        &laid.frame,
        &mut text,
        "sans",
        &mut draw,
        None,
        &images,
    );

    assert_eq!(draw.image_count(), 1, "the picture was not drawn");
    let quad = draw.images().next().expect("the picture");
    assert_eq!(quad.image, 7, "the texture the platform supplied");
    assert_eq!(
        (quad.width, quad.height),
        (320.0, 180.0),
        "its measured size"
    );
}

/// A picture the platform knows nothing about draws nothing, and the screen around it is still drawn.
///
/// A missing picture is a build in progress rather than a broken screen: the presenter answers a scene
/// whose image was never built the same way, and a guess would be a picture nobody asked for.
#[test]
fn a_picture_with_no_texture_draws_nothing() {
    let source =
        "screen s:\n    column:\n        image missing.thing\n        text \"Still here.\"\n";
    let images = ImageTable::new();
    let mut text = engine();
    let laid = set(source)
        .with_images(images.clone())
        .lay(
            "s",
            &Args::new(),
            &vela_ui::ScreenState::new(),
            (1280, 720),
            &mut text,
            "sans",
        )
        .expect("the screen is declared");

    let mut draw = DrawList::new();
    vela_ui::paint(
        &laid.node,
        &laid.frame,
        &mut text,
        "sans",
        &mut draw,
        None,
        &images,
    );

    assert_eq!(draw.image_count(), 0, "a picture nobody supplied was drawn");
    assert!(
        draw.glyph_count() > 0,
        "the rest of the screen was lost too"
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
