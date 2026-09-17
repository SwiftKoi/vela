//! What a style resolves to at instantiation: its colour, its per-state values, its font, and the
//! values a `style_prefix` hands out.
//!
//! Split from `instantiate.rs` by `REPO_LAYOUT.md §3.1`'s first recipe — by responsibility. That file
//! answers *what tree does a body build*; this one answers *what does a style do to the node that
//! names it*. The two stopped fitting in one file when fonts joined colour, size, and state, and the
//! seam is real rather than a line count: nothing here is about a tree's shape.

use vela_render::Color;
use vela_span::FileId;
use vela_syntax::parse;
use vela_text::{Font, TextEngine};
use vela_ui::{Args, Kind, Node, ScreenSet, State};

/// The bundled face, so text measures to something real.
fn engine() -> TextEngine {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);
    text
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

/// A style's `font` setting reaches the node's paint (`SCREENS.md §5`).
///
/// This is how a screen draws two scripts: the font is the *style's*, so a screen that draws Japanese
/// names beside Latin ones names two. The name is not resolved here — whether the engine carries the
/// face is a paint question, and a style cannot know what a build shipped.
#[test]
fn a_style_sets_a_nodes_font() {
    let paint = styled_paint(
        "theme dusk:\n    font ui = \"sans\"\n    font kanji = \"SourceHanSans\"\n\nstyle kanji:\n    font = theme.kanji\n\nscreen s:\n    text \"x\" style = kanji\n",
    );
    assert_eq!(paint.font.as_deref(), Some("SourceHanSans"));
}

/// A `font` naming a token the theme does not declare resolves to nothing rather than to a guess.
#[test]
fn a_font_naming_no_token_resolves_to_nothing() {
    let paint = styled_paint(
        "theme dusk:\n    color fg = 0xe6e6f0\n\nstyle odd:\n    font = theme.nowhere\n\nscreen s:\n    text \"x\" style = odd\n",
    );
    assert_eq!(paint.font, None);
}

/// A font is a fallback, like the rest of a style: a widget that names no style keeps its font.
#[test]
fn a_text_with_no_style_has_no_font() {
    let paint = styled_paint(
        "theme dusk:\n    font kanji = \"SourceHanSans\"\n\nstyle kanji:\n    font = theme.kanji\n\nscreen s:\n    text \"x\"\n",
    );
    assert_eq!(paint.font, None);
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
        .build(
            screen,
            &Args::new(),
            &mut vela_ui::ScreenState::new(),
            &mut text,
            "sans",
            1280.0,
        )
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
        .build(
            "s",
            &Args::new(),
            &mut vela_ui::ScreenState::new(),
            &mut text,
            "sans",
            1280.0,
        )
        .expect("the screen is declared");
    root.children[0].paint.clone()
}
