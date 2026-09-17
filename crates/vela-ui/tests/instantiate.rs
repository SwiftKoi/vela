//! Evaluating a screen body: the tree it produces, asserted rather than eyeballed.
//!
//! The point of the instantiator is that a screen is a *function* — arguments in, tree out —
//! so these are about what tree a given body and arguments produce, not about pixels. Pixels
//! are `paint.rs`'s question, and what a *style* resolves to is `styling.rs`'s.

use vela_span::FileId;
use vela_syntax::parse;
use vela_text::{Font, TextEngine};
use vela_ui::{Action, Args, Kind, Node, ScreenSet, Value};

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
    built_screen(source, "dialogue", args)
}

/// The root of a built screen, by name — for a fixture that is not the dialogue screen.
fn built_screen(source: &str, name: &str, args: &Args) -> Node {
    let mut text = engine();
    set(source)
        .build(name, args, &mut text, "sans", 1280.0)
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

/// An action handed in as a parameter reaches the widget that holds it.
///
/// `SCREENS.md §7`: the caller supplies the answer, which is the whole point of a confirm screen.
/// Before this, `action yes_action` produced no action at all — the screen drew, both buttons were
/// inert, and no diagnostic said why.
#[test]
fn an_action_parameter_reaches_the_button() {
    let source = "\
screen confirm(message, yes_action, no_action):
    column:
        text message
        button:
            text \"Yes\"
            action yes_action
        button:
            text \"No\"
            action no_action
";
    let mut args = Args::new();
    args.set("message", Value::Str("Leave?".to_string()));
    args.set("yes_action", Value::Action(Action::new("quit", Vec::new())));
    args.set(
        "no_action",
        Value::Action(Action::new("hide", vec!["confirm".to_string()])),
    );

    let root = built_screen(source, "confirm", &args);
    let column = &root.children[0];
    let name = |node: &Node| node.action.as_ref().map(|action| action.name.clone());
    assert_eq!(name(&column.children[1]), Some("quit".to_string()));
    assert_eq!(name(&column.children[2]), Some("hide".to_string()));
}

/// And an action a screen *passes on*: a `use` argument is a value like any other (`§2.1`).
#[test]
fn an_action_travels_through_a_use() {
    let source = "\
screen row(do):
    button:
        text \"Go\"
        action do

screen menu:
    use row(quit())
";
    let root = built_screen(source, "menu", &Args::new());
    assert_eq!(
        root.children[0]
            .action
            .as_ref()
            .map(|action| action.name.as_str()),
        Some("quit"),
        "the action did not survive the composition"
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

/// An `if` draws the first arm that holds, and the `else` when none does.
///
/// *First*, not last: a chain where two conditions hold must draw the earlier one, which is what an
/// `elif` means. The arms are one line, so this is one decision rather than a walk that remembers what
/// the line before it concluded.
#[test]
fn an_if_draws_the_first_arm_that_holds() {
    let source = "\
screen s(first: bool, second: bool):
    if first:
        text \"one\"
    elif second:
        text \"two\"
    else:
        text \"three\"
";
    let arm = |first: bool, second: bool| -> String {
        let mut args = Args::new();
        args.set("first", Value::Bool(first));
        args.set("second", Value::Bool(second));
        let root = built_screen(source, "s", &args);
        match &root.children[0].kind {
            Kind::Text { text, .. } => text.clone(),
            other => panic!("expected the chosen arm's text, got {other:?}"),
        }
    };
    assert_eq!(arm(true, true), "one", "the `then` arm wins a tie");
    assert_eq!(arm(false, true), "two");
    assert_eq!(arm(false, false), "three", "and `else` is the last resort");
}

/// An `if` no arm of which holds, and which has no `else`, draws nothing.
#[test]
fn an_if_with_no_arm_holding_draws_nothing() {
    let mut args = Args::new();
    args.set("flag", Value::Bool(false));
    let root = built_screen(
        "screen s(flag: bool):\n    if flag:\n        text \"shown\"\n",
        "s",
        &args,
    );
    assert!(
        root.children.is_empty(),
        "an arm that does not hold drew something: {:?}",
        root.children
    );
}
