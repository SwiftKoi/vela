//! Evaluating a screen body: the tree it produces, asserted rather than eyeballed.
//!
//! The point of the instantiator is that a screen is a *function* — arguments in, tree out —
//! so these are about what tree a given body and arguments produce, not about pixels. Pixels
//! are `paint.rs`'s question.

use vela_span::FileId;
use vela_syntax::parse;
use vela_text::{Font, TextEngine};
use vela_ui::{Args, Kind, Node, ScreenSet, Value};

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

/// The dialogue screen from `examples/standard`, near enough.
const DIALOGUE: &str = "theme dusk:\n    color bg = 0x10121a\n    color fg = 0xe6e6f0\n\nstyle body:\n    color = theme.fg\n\nscreen dialogue(name: str?, line: str):\n    layer ui\n    box at bottom:\n        pad 24\n        column gap 8:\n            if name is not none:\n                text name\n            text line style = body\n";

fn args(name: Option<&str>, line: &str) -> Args {
    let mut args = Args::new();
    args.set(
        "name",
        name.map_or(Value::None, |name| Value::Str(name.to_string())),
    );
    args.set("line", Value::Str(line.to_string()));
    args
}

/// The root of a built screen.
fn built(source: &str, args: &Args) -> Node {
    let mut text = engine();
    set(source)
        .build("dialogue", args, &mut text, "sans", 1280.0)
        .expect("the screen is declared")
}

/// The body produces the box, the box holds the column, and the column holds the texts.
#[test]
fn a_screen_body_becomes_a_widget_tree() {
    let root = built(DIALOGUE, &args(Some("Eileen"), "The rain had stopped."));

    // The screen is placed in a full-window stack, so a top-level anchor has space to work in.
    assert!(matches!(root.kind, Kind::Stack));
    assert_eq!(
        root.props.width,
        vela_ui::SizeSpec::Percent(1.0),
        "the screen does not fill the frame"
    );

    let box_node = &root.children[0];
    assert!(matches!(box_node.kind, Kind::Box));
    assert_eq!(box_node.props.pad, 24.0, "`pad 24` was not applied");
    assert_eq!(
        box_node.props.anchor,
        vela_ui::Anchor::Bottom,
        "`at bottom` was not read as an anchor"
    );

    let column = &box_node.children[0];
    assert!(matches!(column.kind, Kind::Column));
    assert_eq!(column.props.gap, 8.0, "`gap 8` was not applied");
}

/// A `text` leaf carries the resolved string and the size it measured to.
#[test]
fn text_resolves_from_arguments_and_measures() {
    let root = built(DIALOGUE, &args(Some("Eileen"), "The rain had stopped."));
    let column = &root.children[0].children[0];
    assert_eq!(column.children.len(), 2, "the name and the line");

    let Kind::Text { text, size } = &column.children[0].kind else {
        panic!("the first child is the speaker, not text");
    };
    assert_eq!(text, "Eileen");
    assert!(
        size.width > 0.0 && size.height > 0.0,
        "unmeasured: {size:?}"
    );

    let Kind::Text { text, .. } = &column.children[1].kind else {
        panic!("the second child is the line, not text");
    };
    assert_eq!(text, "The rain had stopped.");
}

/// A conditional is evaluated against the arguments, not guessed.
#[test]
fn an_optional_argument_drops_its_branch() {
    let with = built(DIALOGUE, &args(Some("Eileen"), "Hi."));
    let without = built(DIALOGUE, &args(None, "Hi."));

    let with_column = &with.children[0].children[0];
    let without_column = &without.children[0].children[0];
    assert_eq!(with_column.children.len(), 2);
    assert_eq!(
        without_column.children.len(),
        1,
        "`if name is not none` did not drop the speaker"
    );
}

/// The style a text names is resolved through the theme, and both leaves carry its colour.
#[test]
fn a_style_resolves_its_theme_colour() {
    let root = built(DIALOGUE, &args(Some("Eileen"), "Hi."));
    let column = &root.children[0].children[0];
    let line = &column.children[1];

    assert_eq!(
        line.paint.color,
        Some(vela_render::Color::rgb(0xe6, 0xe6, 0xf0)),
        "`style body` did not resolve to `theme.fg`"
    );
}

/// A `background` prop resolves a theme token to a fill.
#[test]
fn a_background_prop_resolves_a_theme_token() {
    let root = built(
        "theme dusk:\n    color bg = 0x10121a\n\nscreen dialogue(line: str):\n    box at bottom, background = theme.bg:\n        text line\n",
        &args(None, "Hi."),
    );
    let box_node = &root.children[0];
    assert_eq!(
        box_node.paint.background,
        Some(vela_render::Color::rgb(0x10, 0x12, 0x1a))
    );
}

/// An unknown screen is not a screen.
#[test]
fn an_undeclared_screen_builds_nothing() {
    let mut text = engine();
    let set = set(DIALOGUE);
    assert!(!set.has("settings"));
    assert!(set.has("dialogue"));
    assert_eq!(set.names(), vec!["dialogue"]);
    assert!(
        set.build("settings", &Args::new(), &mut text, "sans", 1280.0)
            .is_none()
    );
}

/// A grid's `columns` prop decides its kind, since the kind is where it is kept.
#[test]
fn a_grid_reads_its_column_count() {
    let source = "screen s:\n    grid columns 3:\n        text \"a\"\n        text \"b\"\n        text \"c\"\n        text \"d\"\n";
    let mut text = engine();
    let root = set(source)
        .build("s", &Args::new(), &mut text, "sans", 1280.0)
        .expect("the screen is declared");
    assert!(matches!(root.children[0].kind, Kind::Grid { columns: 3 }));
}
