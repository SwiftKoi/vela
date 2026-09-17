//! Evaluating a screen body: the tree it produces, asserted rather than eyeballed.
//!
//! The point of the instantiator is that a screen is a *function* — arguments in, tree out —
//! so these are about what tree a given body and arguments produce, not about pixels. Pixels
//! are `paint.rs`'s question.

use vela_render::Color;
use vela_span::FileId;
use vela_syntax::parse;
use vela_text::{Font, TextEngine};
use vela_ui::{Action, Args, Kind, Node, ScreenSet, State, Value};

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

/// A `style` carries a value per interaction state (`SCREENS.md §5`).
///
/// The state is named by a prefix on the setting's key, so one body describes how a control looks at
/// rest and under the focus cursor. `idle` is not a fourth value: it names the value a setting has on
/// its own, which is why `color` and `idle_color` are one setting.
#[test]
fn a_style_resolves_a_value_per_interaction_state() {
    let source = "\
theme dusk:
    color off = 0x101010
    color on = 0xff0000
    color dim = 0x333333

style item:
    color = theme.off
    hover_color = theme.on
    selected_color = theme.on
    insensitive_color = theme.dim

screen s:
    text \"x\" style = item
";
    let paint = styled_paint(source);

    assert_eq!(paint.color, Some(Color::rgb(0x10, 0x10, 0x10)), "the value");
    assert_eq!(
        paint.in_state(State::Hover).color,
        Some(Color::rgb(0xff, 0, 0))
    );
    assert_eq!(
        paint.in_state(State::Selected).color,
        Some(Color::rgb(0xff, 0, 0))
    );
    assert_eq!(
        paint.in_state(State::Insensitive).color,
        Some(Color::rgb(0x33, 0x33, 0x33))
    );
    assert_eq!(
        paint.in_state(State::Idle).color,
        paint.color,
        "idle is the value itself, not a state to store"
    );
}

/// `idle_color` and `color` are one setting, and the later one wins — the rule a setting body already
/// has (`LANGUAGE.md §7`).
#[test]
fn idle_color_is_the_value_itself() {
    let paint = styled_paint(
        "theme dusk:\n    color a = 0x111111\n    color b = 0x222222\n\nstyle item:\n    color = theme.a\n    idle_color = theme.b\n\nscreen s:\n    text \"x\" style = item\n",
    );
    assert_eq!(paint.color, Some(Color::rgb(0x22, 0x22, 0x22)));
    assert!(paint.over(State::Idle).is_none(), "idle stores nothing");
}

/// A derived style keeps what it does not mention, including a state its base set.
#[test]
fn a_derived_style_overrides_only_what_it_names() {
    let paint = styled_paint(
        "theme dusk:\n    color off = 0x101010\n    color on = 0xff0000\n\nstyle base:\n    color = theme.off\n    hover_color = theme.on\n\nstyle derived from base:\n    selected_color = theme.on\n\nscreen s:\n    text \"x\" style = derived\n",
    );
    assert_eq!(
        paint.color,
        Some(Color::rgb(0x10, 0x10, 0x10)),
        "the base's value"
    );
    assert_eq!(
        paint.in_state(State::Hover).color,
        Some(Color::rgb(0xff, 0, 0)),
        "the base's hover value was lost"
    );
    assert_eq!(
        paint.in_state(State::Selected).color,
        Some(Color::rgb(0xff, 0, 0))
    );
}

/// A state that changes one thing keeps the value of everything else: an override is a *diff*.
#[test]
fn a_state_override_is_a_diff_not_a_replacement() {
    let paint = styled_paint(
        "theme dusk:\n    color off = 0x101010\n    color on = 0xff0000\n\nstyle item:\n    color = theme.off\n    background = theme.off\n    hover_color = theme.on\n\nscreen s:\n    text \"x\" style = item\n",
    );
    let hover = paint.in_state(State::Hover);
    assert_eq!(hover.color, Some(Color::rgb(0xff, 0, 0)));
    assert_eq!(
        hover.background,
        Some(Color::rgb(0x10, 0x10, 0x10)),
        "the hover state dropped the background it did not mention"
    );
}

/// `style_prefix` gives every widget in a block a style named for it (`SCREENS.md §5.2`).
///
/// This is how one line skins a screen's whole contents: `say` and a `text` are `say_text`, and the
/// screen never names that style itself.
#[test]
fn a_style_prefix_gives_each_widget_its_style() {
    let source = "\
theme dusk:
    color on = 0x111111

style say_text:
    color = theme.on

screen s:
    style_prefix say
    text \"x\"
";
    assert_eq!(text_colour(source, "s"), Some(Color::rgb(0x11, 0x11, 0x11)));
}

/// A widget's own `style = …` is applied after the prefix, so it wins — the prefix is a fallback.
#[test]
fn a_widgets_own_style_beats_the_prefix() {
    let source = "\
theme dusk:
    color a = 0x111111
    color b = 0x222222

style say_text:
    color = theme.a

style mine:
    color = theme.b

screen s:
    style_prefix say
    text \"x\" style = mine
";
    assert_eq!(text_colour(source, "s"), Some(Color::rgb(0x22, 0x22, 0x22)));
}

/// A prefix naming styles a project has not written is a fallback, not an error — which is what makes
/// it safe to write a prefix before every style it names exists.
#[test]
fn a_prefix_that_names_nothing_falls_back() {
    let source = "screen s:\n    style_prefix nowhere\n    text \"x\"\n";
    assert_eq!(text_colour(source, "s"), None);
}

/// A nested block's prefix wins inside itself, and only there.
#[test]
fn a_nested_prefix_overrides_the_enclosing_one() {
    let source = "\
theme dusk:
    color a = 0x111111
    color b = 0x222222

style out_text:
    color = theme.a

style in_text:
    color = theme.b

screen s:
    style_prefix out
    column:
        style_prefix in
        text \"inner\"
    text \"outer\"
";
    let colours = text_colours(source, "s");
    assert_eq!(
        colours,
        vec![
            Some(Color::rgb(0x22, 0x22, 0x22)),
            Some(Color::rgb(0x11, 0x11, 0x11))
        ],
        "the inner block's prefix did not scope to the inner block"
    );
}

/// A used screen does not inherit the caller's prefix: a screen is a function (`§2.1`), so its look
/// cannot depend on where it was used.
#[test]
fn a_used_screen_does_not_inherit_the_callers_prefix() {
    let source = "\
theme dusk:
    color a = 0x111111

style out_text:
    color = theme.a

screen inner:
    text \"from the used screen\"

screen s:
    style_prefix out
    text \"from the caller\"
    use inner
";
    let colours = text_colours(source, "s");
    assert_eq!(
        colours,
        vec![Some(Color::rgb(0x11, 0x11, 0x11)), None],
        "the used screen's text picked up the caller's prefix"
    );
}

/// Every `text` leaf's colour, in tree order.
fn text_colours(source: &str, screen: &str) -> Vec<Option<Color>> {
    let root = build(source, screen);
    let mut out = Vec::new();
    fn walk(node: &Node, out: &mut Vec<Option<Color>>) {
        if matches!(node.kind, Kind::Text { .. }) {
            out.push(node.paint.color);
        }
        for child in &node.children {
            walk(child, out);
        }
    }
    walk(&root, &mut out);
    out
}

/// The colour the first `text` leaf draws with, if it has one.
fn text_colour(source: &str, screen: &str) -> Option<Color> {
    text_colours(source, screen).first().copied().flatten()
}

/// Builds a screen from a fixture, asserting it parses.
fn build(source: &str, screen: &str) -> Node {
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
    let mut text = engine();
    ScreenSet::from_items(&parsed.program.items)
        .build(screen, &Args::new(), &mut text, "sans", 1280.0)
        .expect("the screen is declared")
}

/// The paint of the one text leaf a fixture declares.
fn styled_paint(source: &str) -> vela_ui::Paint {
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
    let mut text = engine();
    let root = ScreenSet::from_items(&parsed.program.items)
        .build("s", &Args::new(), &mut text, "sans", 1280.0)
        .expect("the screen is declared");
    root.children[0].paint.clone()
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
