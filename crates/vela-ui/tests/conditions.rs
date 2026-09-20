//! Deciding with comparisons: what a screen condition can ask (`SCREENS.md §2.2`).
//!
//! The arm that draws *is* the assertion. A condition the evaluator cannot make draws the `else`, so a
//! test that names the text proves the comparison was made rather than answered false — which is the
//! failure this file exists to catch, because a screen that draws the wrong arm says nothing.

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

/// The first text the screen draws, with `value` as its one argument.
fn drawn(condition: &str, value: Value) -> String {
    let source = format!(
        "screen s(value):\n    column:\n        if {condition}:\n            text \"yes\"\n        else:\n            text \"no\"\n"
    );
    let mut args = Args::new();
    args.set("value", value);
    let mut text = engine();
    let laid = set(&source)
        .lay(
            "s",
            &args,
            &ScreenState::new(),
            (1280, 720),
            &mut text,
            "sans",
        )
        .expect("the screen is declared");
    first_text(&laid.node).expect("both arms draw").to_string()
}

/// The text of the first `text` node.
fn first_text(node: &Node) -> Option<&str> {
    if let Kind::Text { text, .. } = &node.kind {
        return Some(text.as_str());
    }
    node.children.iter().find_map(first_text)
}

/// `==` and `!=` compare the values a screen has, by value and not by kind.
///
/// This is the pair item 13's own use needs: the sample's help screen picks its tab with
/// `if device == "keyboard"`, and a comparison that answered false would draw the wrong one in silence.
#[test]
fn equality_compares_what_the_screen_has() {
    for (condition, value, expected) in [
        (
            "value == \"keyboard\"",
            Value::Str("keyboard".into()),
            "yes",
        ),
        ("value == \"mouse\"", Value::Str("keyboard".into()), "no"),
        ("value != \"mouse\"", Value::Str("keyboard".into()), "yes"),
        ("value == 3", Value::Num(3.0), "yes"),
        ("value == 3", Value::Num(4.0), "no"),
        ("value != 3", Value::Num(4.0), "yes"),
        ("value == true", Value::Bool(true), "yes"),
        ("value == none", Value::None, "yes"),
        // Not coerced: a number is not the string that spells it.
        ("value == \"1\"", Value::Num(1.0), "no"),
        ("value == none", Value::Str("nothing".into()), "no"),
    ] {
        assert_eq!(drawn(condition, value.clone()), expected, "`{condition}`");
    }
}

/// `<`, `<=`, `>` and `>=` order numbers and strings, and report nothing for anything else.
///
/// A number against a string has no order, so the answer is false rather than a coercion nobody asked
/// for — the same rule equality follows.
#[test]
fn ordering_compares_numbers_and_strings() {
    for (condition, value, expected) in [
        ("value < 5", Value::Num(3.0), "yes"),
        ("value < 5", Value::Num(5.0), "no"),
        ("value <= 5", Value::Num(5.0), "yes"),
        ("value > 5", Value::Num(7.0), "yes"),
        ("value >= 5", Value::Num(5.0), "yes"),
        ("value > \"apple\"", Value::Str("banana".into()), "yes"),
        ("value < \"apple\"", Value::Str("banana".into()), "no"),
        ("value <= \"apple\"", Value::Str("apple".into()), "yes"),
        // No order between kinds, and no order against `none`.
        ("value < \"a\"", Value::Num(1.0), "no"),
        ("value < 5", Value::None, "no"),
        ("value > 5", Value::Bool(true), "no"),
    ] {
        assert_eq!(drawn(condition, value.clone()), expected, "`{condition}`");
    }
}

/// `and`, `or` and `not` combine conditions, and `is` is still equality.
///
/// The sample's platform test is the shape that has to work: `A or (B and not C)` — a chain, not a
/// single comparison, and one the evaluator could not make at all until this landed.
#[test]
fn boolean_operators_combine_conditions() {
    for (condition, value, expected) in [
        ("value and true", Value::Bool(true), "yes"),
        ("value and true", Value::Bool(false), "no"),
        ("value or false", Value::Bool(true), "yes"),
        ("value or false", Value::Bool(false), "no"),
        ("not value", Value::Bool(false), "yes"),
        ("not value", Value::Bool(true), "no"),
        ("value == 1 or value == 2", Value::Num(2.0), "yes"),
        (
            "value == 1 or (value < 3 and not value == 2)",
            Value::Num(2.0),
            "no",
        ),
        (
            "value == 1 or (value < 3 and not value == 2)",
            Value::Num(1.0),
            "yes",
        ),
        // An unbound name is false, which is what makes `not` of it true (`§2.2`).
        ("value is not none", Value::Str("x".into()), "yes"),
        ("value is none", Value::Str("x".into()), "no"),
    ] {
        assert_eq!(drawn(condition, value.clone()), expected, "`{condition}`");
    }
}

/// A record's *field* decides a branch, not only a text or an argument.
///
/// `SCREENS.md §2.2` says a condition is over "what the screen has — its parameters, its variables, what a
/// loop walks", and a field of one of those is that value *read*, not a new source. The arm was missing:
/// the condition walk had one for a name, a comparison and a question, and everything else was `false` — so
/// `if value.empty` drew the else branch of a correct screen and nothing reported it. Found by writing the
/// first thing that needs it: `slots(3)` answers a list of records, and a save screen draws an empty cell by
/// testing one.
#[test]
fn a_records_field_decides_a_branch() {
    let empty = Value::Record(vec![("empty".to_string(), Value::Bool(true))]);
    let used = Value::Record(vec![("empty".to_string(), Value::Bool(false))]);

    assert_eq!(drawn("value.empty", empty), "yes");
    assert_eq!(drawn("value.empty", used.clone()), "no");
    assert_eq!(
        drawn("value.empty is true", used.clone()),
        "no",
        "and a comparison the field feeds is the same read"
    );
    assert_eq!(
        drawn("value.missing", used),
        "no",
        "a field that is not there is false rather than a crash"
    );
}
