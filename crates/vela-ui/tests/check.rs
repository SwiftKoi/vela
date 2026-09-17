//! Checking a widget tree against the registry.
//!
//! This is the exit criterion that asks for *"unknown widget / wrong prop / bad arg each
//! produce the right `E5xxx` with a suggestion"* — so every test here asserts a code **and**
//! the help text, because a code without a suggestion is half the diagnostic.

use vela_span::FileId;
use vela_syntax::{Item, ScreenDecl, parse};
use vela_ui::{ActionRegistry, SemanticActions, WidgetRegistry, check_screen};

/// Every `screen` a parsed fixture declares, as the checker wants them.
fn screens_of(parsed: &vela_syntax::ParseResult) -> Vec<&ScreenDecl> {
    parsed
        .program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Screen(screen) => Some(screen),
            _ => None,
        })
        .collect()
}

/// Checks a screen body and returns `(code, help)` for each diagnostic.
fn diagnose(body: &str) -> Vec<(String, String)> {
    diagnose_source(&format!("screen s:\n{body}"))
}

/// Checks whatever a source declares, so a fixture can give its screen parameters.
fn diagnose_source(source: &str) -> Vec<(String, String)> {
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
    let screens = screens_of(&parsed);
    let Some(screen) = screens.first() else {
        panic!("expected a screen");
    };
    check_screen(
        &screen.body,
        &screen.params,
        &WidgetRegistry::builtin(),
        &screens,
        &ActionRegistry::builtin(),
        &SemanticActions::builtin(),
    )
    .into_iter()
    .map(|d| {
        (
            d.code.as_str().to_string(),
            d.help.unwrap_or_else(|| "<no help>".to_string()),
        )
    })
    .collect()
}

/// A valid screen produces nothing. The checker's first job is not to cry wolf.
#[test]
fn a_valid_screen_is_clean() {
    let source = "screen dialogue(name: str?, line: str):\n    layer ui\n    box at bottom:\n        pad 24\n        column gap 8:\n            text name\n            text line style = body\n";
    let parsed = parse(FileId::from_raw(0), source);
    let screens = screens_of(&parsed);
    let Some(screen) = screens.first() else {
        panic!("expected a screen");
    };
    let diagnostics = check_screen(
        &screen.body,
        &screen.params,
        &WidgetRegistry::builtin(),
        &screens,
        &ActionRegistry::builtin(),
        &SemanticActions::builtin(),
    );
    assert!(
        diagnostics.is_empty(),
        "{:?}",
        diagnostics
            .iter()
            .map(|d| format!("{}: {}", d.code.as_str(), d.message))
            .collect::<Vec<_>>()
    );
}

/// `E5005`, with the name it probably meant.
#[test]
fn an_unknown_widget_is_reported_with_a_suggestion() {
    let diagnostics = diagnose("    colunm\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let (code, help) = &diagnostics[0];
    assert_eq!(code, "E5005");
    assert!(help.contains("column"), "{help}");
}

/// A name nowhere near anything registered gets the code and no guess.
///
/// A wrong suggestion is worse than none: it sends someone looking in the wrong place.
#[test]
fn an_unrecognisable_widget_gets_no_suggestion() {
    let diagnostics = diagnose("    canvas\n");
    assert_eq!(diagnostics[0].0, "E5005");
    assert_eq!(diagnostics[0].1, "<no help>");
}

/// `E5006` — a prop that exists, just not on this widget.
#[test]
fn a_prop_on_the_wrong_widget_is_reported() {
    let diagnostics = diagnose("    text \"hi\" columns 3\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "E5006");
}

/// A misspelled prop suggests the one that was meant.
#[test]
fn a_misspelled_prop_suggests_the_right_one() {
    let diagnostics = diagnose("    row gapp 8:\n        text \"hi\"\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let (code, help) = &diagnostics[0];
    assert_eq!(code, "E5006");
    assert!(help.contains("gap"), "{help}");
}

/// `E5004` — a single-child widget given two.
///
/// The most common screen mistake after a typo: indenting a sibling one level too far.
#[test]
fn two_children_under_a_single_child_widget_is_reported() {
    let diagnostics = diagnose("    box:\n        text \"a\"\n        text \"b\"\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "E5004");
    assert!(
        diagnostics[0].1.contains("column"),
        "the help should say what to do"
    );
}

/// A prop on its own line belongs to the widget above it, and is not an unknown widget.
///
/// This is the ambiguity the parser cannot resolve and the registry can: `pad` is not a
/// widget, and `box` takes it.
#[test]
fn a_bare_prop_line_is_not_an_unknown_widget() {
    let diagnostics = diagnose("    box:\n        pad 24\n        text \"hi\"\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// A condition that calls something cannot be decided, and says so (`W4013`).
///
/// A screen decides from what it has (`SCREENS.md §2.2`). A call reads like a decision and is not one —
/// it is false — so the screen draws the wrong arm while saying nothing, which is the failure this
/// warning exists to make loud. The sample's `renpy.variant(...)` and `GamepadExists()` are both this
/// shape and both are §7's host systems, rather than mistakes in the screen.
#[test]
fn a_condition_that_calls_something_is_reported() {
    let diagnostics = diagnose("    if GamepadExists():\n        text \"pad\"\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "W4013");
    assert!(diagnostics[0].1.contains("parameter"), "{diagnostics:?}");
}

/// Each call is reported, and every nesting is walked: an `elif` and a loop both hold conditions a
/// reader would otherwise not be told about.
#[test]
fn every_call_in_every_condition_is_reported() {
    let diagnostics = diagnose(
        "    if One() or Two():\n        text \"a\"\n    elif Three():\n        text \"b\"\n    for item in items:\n        if Four():\n            text \"c\"\n",
    );
    let codes: Vec<&str> = diagnostics.iter().map(|(code, _)| code.as_str()).collect();
    assert_eq!(
        codes,
        vec!["W4013", "W4013", "W4013", "W4013"],
        "{diagnostics:?}"
    );
}

/// A condition made of comparisons is clean: that is what a screen decides with, and the point of the
/// operators is that a screen can now ask its own questions.
#[test]
fn a_condition_made_of_comparisons_is_clean() {
    let diagnostics = diagnose(
        "    default device = \"keyboard\"\n    if device == \"keyboard\" or device != \"mouse\":\n        text device\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// A question the host answers is not an action and not an undecidable condition (`SCREENS.md §2.6`).
///
/// The call is checked against the *vocabulary of names* instead — an action registry has nothing to say
/// about `variant`, and `W4013` is for conditions that cannot be decided at all, which this one can.
#[test]
fn a_variant_question_is_clean() {
    let diagnostics = diagnose(
        "    if variant(\"pc\") or variant(\"web\"):\n        text \"Help\"\n    text variant(\"small\")\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// A variant name the engine does not know is an error rather than Ren'Py's silent `False`.
///
/// This is the whole reason the vocabulary is closed: `tablet` is a variant Ren'Py has and Vela does
/// not, and a migration that writes it should hear about it now rather than draw the `else` arm.
#[test]
fn an_unknown_variant_is_an_error() {
    let diagnostics = diagnose("    if variant(\"tablet\"):\n        text \"Tablet\"\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "E5018");
    assert!(
        diagnostics[0].1.contains("names the platform"),
        "{diagnostics:?}"
    );
}

/// A name that is not written out cannot be checked, so it is an error too.
///
/// The vocabulary is closed and the check happens where the name is — a computed name would make the
/// question unanswerable at check time, which is the trade the closed vocabulary buys.
#[test]
fn a_computed_variant_name_is_an_error() {
    let diagnostics =
        diagnose("    default device = \"pc\"\n    if variant(device):\n        text \"Help\"\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "E5018");
    assert!(
        diagnostics[0].1.contains("pc, web, mobile, small"),
        "{diagnostics:?}"
    );
}

/// A variant question inside a condition is *not* reported as undecidable, and one nested in a call
/// that is undecidable still is.
#[test]
fn a_variant_question_narrows_w4013() {
    let diagnostics =
        diagnose("    if variant(\"pc\") and GamepadExists():\n        text \"Help\"\n");
    let codes: Vec<&str> = diagnostics.iter().map(|(code, _)| code.as_str()).collect();
    assert_eq!(codes, vec!["W4013"], "{diagnostics:?}");
}

/// A prop line under a widget that does *not* take it is still a mistake — and the mistake is
/// an unknown widget, because that is what it looks like from here.
#[test]
fn a_prop_line_under_the_wrong_parent_is_reported() {
    let diagnostics = diagnose("    text \"hi\"\n    pad 24\n");
    assert!(!diagnostics.is_empty(), "a stray prop should be reported");
}

/// One mistake does not hide the next: both unknown widgets are reported.
#[test]
fn every_unknown_widget_is_reported() {
    let diagnostics = diagnose("    colunm\n    txt \"a\"\n    roww\n");
    assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
}

/// A `Layer` line is not a widget and is not checked against the registry.
#[test]
fn a_layer_line_is_not_a_widget() {
    let diagnostics = diagnose("    layer ui\n    box:\n        text \"hi\"\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// An `if` inside a widget keeps that widget as its parent, so props inside it still resolve.
#[test]
fn a_conditional_keeps_its_parent_widget() {
    let diagnostics = diagnose("    box:\n        if true:\n            pad 24\n    \n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// A widget in an `elif` or an `else` is checked like any other.
///
/// Which arm draws is a runtime question, and a typo in an arm that has not been run yet is still a
/// typo — so the arms beside the `then` are walked, not skipped.
#[test]
fn a_mistake_in_an_elif_or_else_is_reported() {
    let diagnostics = diagnose(
        "    if true:\n        text \"ok\"\n    elif false:\n        txt \"a\"\n    else:\n        colunm\n",
    );
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "E5005");
    assert_eq!(diagnostics[1].0, "E5005");
}

/// `E5014` — a `key` naming something the host does not deliver, with the name it probably meant.
///
/// The binding is what makes a modal screen modal, so a misspelled one is a screen whose escape hatch
/// answers nothing — and nothing about the screen would look wrong.
#[test]
fn an_unknown_semantic_action_is_reported_with_a_suggestion() {
    let diagnostics = diagnose("    key menue_up action close_screen()\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let (code, help) = &diagnostics[0];
    assert_eq!(code, "E5014");
    assert!(help.contains("menu_up"), "{help}");
}

/// A `key`'s action is an action like any other: a misspelled one is `E5012`, in a binding as in a prop.
#[test]
fn a_key_binding_checks_its_action() {
    let diagnostics = diagnose("    key cancel action clse_screen()\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "E5012");
}

/// The bindings the engine delivers are clean, and so is a timer.
#[test]
fn a_valid_binding_is_clean() {
    let diagnostics =
        diagnose("    key cancel action close_screen()\n    timer 3.0 action quit()\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// A widget inside a loop is checked like any other: the body is drawn, so a typo in it is a typo.
#[test]
fn a_mistake_in_a_loop_is_reported() {
    let diagnostics = diagnose(
        "    for option in items:\n        button:\n            text option.caption\n            action clse_screen()\n",
    );
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "E5012");
}

/// A screen variable is the screen's own, so a parameter cannot have its name (`E5016`).
///
/// Two bindings of one name in one screen are two answers to one question, and the caller's answer is
/// the one the screen would silently ignore — the failure this check exists to prevent, because the
/// screen would look like it worked.
#[test]
fn a_screen_variable_cannot_be_named_after_a_parameter() {
    let diagnostics =
        diagnose_source("screen s(device):\n    default device = \"keyboard\"\n    text device\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "E5016");
    assert!(diagnostics[0].1.contains("rename"), "{diagnostics:?}");
}

/// And declaring one twice is the same mistake between two lines of the screen itself.
#[test]
fn a_screen_variable_is_declared_once() {
    let diagnostics = diagnose("    default tab = \"keyboard\"\n    default tab = \"mouse\"\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "E5016");
    assert!(diagnostics[0].1.contains("remove one"), "{diagnostics:?}");
}

/// A variable declared inside a widget or an arm is `E5015`: *whether* it exists cannot depend on what
/// drew (`SCREENS.md §2.5`).
#[test]
fn a_screen_variable_declared_inside_a_block_is_reported() {
    let diagnostics =
        diagnose("    column:\n        default tab = \"keyboard\"\n        text tab\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "E5015");
    assert!(
        diagnostics[0].1.contains("move the `default`"),
        "{diagnostics:?}"
    );
}

/// A write names a variable this screen declares, or it is `E5017` with the nearest name offered.
///
/// The name is written as a *name*, which is the one thing Ren'Py's stringly-typed form cannot be held
/// to: `set_screen_variable(devise, …)` is a button that writes a name nothing reads.
#[test]
fn a_write_to_an_undeclared_variable_is_reported() {
    let diagnostics = diagnose(
        "    default device = \"keyboard\"\n    button:\n        text \"Mouse\"\n        action set_screen_variable(devise, \"mouse\")\n",
    );
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "E5017");
    assert!(diagnostics[0].1.contains("device"), "{diagnostics:?}");
}

/// The vocabulary around a variable is clean: declared once, read, and written.
#[test]
fn a_valid_variable_is_clean() {
    let diagnostics = diagnose(
        "    default device = \"keyboard\"\n    column:\n        text device\n        button:\n            text \"Mouse\"\n            action set_screen_variable(device, \"mouse\")\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
