//! What a screen remembers: a `default` and the value it keeps (`SCREENS.md §2.5`).
//!
//! A screen is otherwise a function of its arguments — arguments in, tree out — and a variable is the
//! one thing that outlives a layout. So these are about the two halves of that: what a variable starts
//! as (the initializer, once) and what it is afterwards (whatever the caller hands back).

use vela_span::FileId;
use vela_syntax::parse;
use vela_text::{Font, TextEngine};
use vela_ui::{Args, Kind, Node, ScreenSet, ScreenState, Value};

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

/// The text a node draws, if it is a text leaf.
fn text_of(node: &Node) -> Option<&str> {
    match &node.kind {
        Kind::Text { text, .. } => Some(text.as_str()),
        _ => None,
    }
}

/// A `default` gives the screen a name the body reads, initialized from its own expression once.
///
/// The initializer runs on the first layout — and may read an argument, which is what makes a screen's
/// opening state the caller's choice — and the value it produces is what the screen keeps.
#[test]
fn a_screen_variable_initializes_the_first_time_it_is_laid_out() {
    let source = "\
screen counters(label, start):
    default count = start
    column:
        text label
        text count
";
    let mut args = Args::new();
    args.set("label", Value::Str("Count".to_string()));
    args.set("start", Value::Num(3.0));

    let mut text = engine();
    let mut state = ScreenState::new();
    let counters = set(source);
    let root = counters
        .build("counters", &args, &mut state, &mut text, "sans", 1280.0)
        .expect("the screen is declared");
    let column = &root.children[0];
    assert_eq!(
        text_of(&column.children[1]),
        Some("3"),
        "the initializer ran"
    );
    assert_eq!(
        state.get("count"),
        Some(&Value::Num(3.0)),
        "and the value is the screen's from then on"
    );

    // A second layout keeps what the caller holds: that is what makes a write survive the frame after
    // it, rather than being undone by the declaration that declared the variable.
    let mut kept = ScreenState::new();
    kept.set("count", Value::Num(7.0));
    let root = counters
        .build("counters", &args, &mut kept, &mut text, "sans", 1280.0)
        .expect("the screen is declared");
    let column = &root.children[0];
    assert_eq!(
        text_of(&column.children[1]),
        Some("7"),
        "a value the caller holds was re-initialized"
    );

    // And a name this screen does not declare is dropped: a renamed variable should not leave a store
    // nothing can read behind it.
    let mut stale = ScreenState::new();
    stale.set("gone", Value::Num(1.0));
    let _ = counters
        .build("counters", &args, &mut stale, &mut text, "sans", 1280.0)
        .expect("the screen is declared");
    assert!(
        stale.get("gone").is_none(),
        "a name the body no longer declares survived"
    );
    assert_eq!(stale.len(), 1, "the screen declares exactly one variable");
}

/// A variable initializer may read the one declared above it, and a screen with none holds nothing.
#[test]
fn a_variable_may_start_from_one_declared_above_it() {
    let source = "\
screen s(start):
    default first = start
    default second = first
    column:
        text first
        text second
";
    let mut args = Args::new();
    args.set("start", Value::Str("hello".to_string()));

    let mut text = engine();
    let mut state = ScreenState::new();
    let root = set(source)
        .build("s", &args, &mut state, &mut text, "sans", 1280.0)
        .expect("the screen is declared");
    let column = &root.children[0];
    assert_eq!(text_of(&column.children[0]), Some("hello"));
    assert_eq!(text_of(&column.children[1]), Some("hello"));

    // A screen that declares nothing holds nothing, which is most of them.
    let mut text = engine();
    let mut state = ScreenState::new();
    set("screen plain:\n    text \"Hi.\"\n")
        .build("plain", &Args::new(), &mut state, &mut text, "sans", 1280.0)
        .expect("the screen is declared");
    assert!(state.is_empty(), "a screen with no `default` has no state");
}
