//! Drawing children from data: what a `for` produces (`SCREENS.md §2.4`).
//!
//! Split from `instantiate.rs`, which is about the tree a body builds line by line: a loop is one line
//! that builds *several* subtrees, and both halves of the rule it lives by — the element, and the scope
//! the binding owns — are its own.

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

/// A record with a caption and an action, as a menu's choices arrive.
fn option(caption: &str, action: &str) -> Value {
    Value::Record(vec![
        ("caption".to_string(), Value::Str(caption.to_string())),
        (
            "action".to_string(),
            Value::Action(Action::new(action, Vec::new())),
        ),
    ])
}

/// The text a node draws, if it is a text leaf.
fn text_of(node: &Node) -> Option<&str> {
    match &node.kind {
        Kind::Text { text, .. } => Some(text.as_str()),
        _ => None,
    }
}

/// A `for` draws its body once per element, with the binding in scope (`SCREENS.md §2.4`).
///
/// Children from data: the elements are a value the caller handed the screen, and a field of one
/// resolves like any other name — `option.caption` is the text, `option.action` is what the button
/// does. That pair is the shape the sample's `choice(items)` screen is built from, and it was
/// unwritable before this: the menu's choices are data, and a screen could not walk it.
#[test]
fn a_loop_draws_its_body_once_per_element() {
    let source = "\
screen choice(prompt, items):
    column:
        text prompt
        for option in items:
            button:
                text option.caption
                action option.action
";
    let mut args = Args::new();
    args.set("prompt", Value::Str("Pick one.".to_string()));
    args.set(
        "items",
        Value::List(vec![option("Yes", "quit"), option("No", "hide")]),
    );

    let mut text = engine();
    let root = set(source)
        .build(
            "choice",
            &args,
            &mut vela_ui::ScreenState::new(),
            &mut text,
            "sans",
            1280.0,
        )
        .expect("the screen is declared");
    let column = &root.children[0];
    assert_eq!(column.children.len(), 3, "the prompt and two buttons");

    let first = &column.children[1];
    assert_eq!(text_of(&first.children[0]), Some("Yes"));
    assert_eq!(
        first.action.as_ref().map(|action| action.name.as_str()),
        Some("quit"),
        "`option.action` did not resolve to the element's action"
    );
    let second = &column.children[2];
    assert_eq!(text_of(&second.children[0]), Some("No"));
    assert_eq!(
        second.action.as_ref().map(|action| action.name.as_str()),
        Some("hide")
    );
}

/// A loop over nothing draws nothing, and its binding does not leak past it.
///
/// Two rules in one fixture, because they are the two ways a loop can be wrong: a screen handed no
/// list has no children rather than an error — which is what makes it safe to open before the system
/// that feeds it exists — and `for option in items` *shadows* an outer `option` rather than being
/// shadowed by it.
#[test]
fn a_loop_over_nothing_is_empty_and_scopes_its_binding() {
    let source = "\
screen s(items, option):
    column:
        text option
        for option in items:
            text option
";

    let mut empty = Args::new();
    empty.set("option", Value::Str("outer".to_string()));
    let mut text = engine();
    let root = set(source)
        .build(
            "s",
            &empty,
            &mut vela_ui::ScreenState::new(),
            &mut text,
            "sans",
            1280.0,
        )
        .expect("the screen is declared");
    let column = &root.children[0];
    assert_eq!(
        column.children.len(),
        1,
        "an unbound list drew something: {:?}",
        column.children
    );

    let mut filled = Args::new();
    filled.set("option", Value::Str("outer".to_string()));
    filled.set("items", Value::List(vec![Value::Str("inner".to_string())]));
    let mut text = engine();
    let root = set(source)
        .build(
            "s",
            &filled,
            &mut vela_ui::ScreenState::new(),
            &mut text,
            "sans",
            1280.0,
        )
        .expect("the screen is declared");
    let column = &root.children[0];
    assert_eq!(column.children.len(), 2);
    assert_eq!(text_of(&column.children[0]), Some("outer"));
    assert_eq!(
        text_of(&column.children[1]),
        Some("inner"),
        "the loop's binding did not shadow the parameter of the same name"
    );
}
