//! The action registry, and the checker that reads it.
//!
//! `SCREENS.md §7` says the action set is a registry and that adding an action is a registry entry.
//! So the tests are about the *registry*: that the set is the documented one, that order is stable,
//! that a plugin's action is indistinguishable from a built-in, and that a misspelled one gets a
//! suggestion rather than silence — plus the gate, which is what makes the registry a vocabulary
//! rather than a documentation source: a name that is not registered is an error, and so is the
//! wrong number of arguments.

use vela_diag::Diagnostic;
use vela_span::FileId;
use vela_syntax::{Item, ScreenDecl, ScreenLine, parse};
use vela_ui::actions::ActionDecl;
use vela_ui::widgets::PropType;
use vela_ui::{ActionRegistry, SemanticActions, WidgetRegistry, check_screen};

/// Every diagnostic a one-screen source produces.
fn diagnostics(source: &str) -> Vec<Diagnostic> {
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
    let screens: Vec<&ScreenDecl> = parsed
        .program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Screen(screen) => Some(screen),
            _ => None,
        })
        .collect();
    let registry = WidgetRegistry::builtin();
    let actions = ActionRegistry::builtin();
    screens
        .iter()
        .flat_map(|screen| {
            check_screen(
                &screen.body,
                &screen.params,
                &registry,
                &screens,
                &actions,
                &SemanticActions::builtin(),
            )
        })
        .collect()
}

/// The codes a source produces, in order.
fn codes(source: &str) -> Vec<String> {
    diagnostics(source)
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect()
}

#[test]
fn the_default_set_is_the_documented_one() {
    let registry = ActionRegistry::builtin();
    for name in [
        "jump",
        "call",
        "return",
        "set",
        "toggle",
        "play",
        "stop",
        "open_screen",
        "close_screen",
        "hide",
        "wait",
        "quit",
        "quick_save",
        "quick_load",
        "rollback",
        "skip",
        "preference",
        "file_page",
        "file_page_previous",
        "file_page_next",
        "file_action",
        "file_delete",
        "set_screen_variable",
        "toggle_preference",
        "language",
        "end_replay",
        "gamepad_calibrate",
    ] {
        assert!(registry.get(name).is_some(), "`{name}` is missing");
    }
    assert_eq!(registry.len(), 27, "SCREENS.md §7 lists twenty-seven");
}

/// The registry says which entries the runtime acts on, and most of them it does not yet.
///
/// The flag is the honest half of a vocabulary that runs ahead of its systems: a reader of the
/// reference, a hover, and `vela run` all give the same answer about whether a button does anything.
#[test]
fn an_action_says_whether_it_is_dispatched() {
    let registry = ActionRegistry::builtin();
    let dispatched: Vec<&str> = registry
        .names()
        .into_iter()
        .filter(|name| registry.get(name).is_some_and(|action| action.dispatched))
        .collect();
    assert_eq!(
        dispatched,
        vec![
            "open_screen",
            "close_screen",
            "hide",
            "quit",
            "quick_save",
            "quick_load",
            "rollback",
            // The two that write the *player's* store rather than the story's: a window and a headless
            // run both carry them out, and nothing draws one yet (`RUNTIME.md §2.1`).
            "preference",
            "toggle_preference",
            // The action that writes a screen's *own* store, and the first of them that is a *value*
            // rather than a name (`SCREENS.md §2.5`).
            "set_screen_variable"
        ]
    );

    // And the sentence a reference page and a hover both print says so rather than implying it works.
    let skip = registry.get("skip").expect("`skip` is registered");
    assert!(!skip.dispatched);
    assert!(
        skip.summary().contains("not dispatched"),
        "{}",
        skip.summary()
    );
    assert!(
        !registry
            .get("quit")
            .unwrap()
            .summary()
            .contains("not dispatched")
    );
}

/// Order is registration order, for the same reason the widget registry's is: a completion
/// list, a docs page, and a golden all read this, and all three should be the same list twice.
#[test]
fn the_order_is_stable() {
    let first = ActionRegistry::builtin().names();
    assert_eq!(first, ActionRegistry::builtin().names());
    assert_eq!(first[0], "jump");
    assert_eq!(first.last(), Some(&"gamepad_calibrate"));
}

/// Arity is what `E5008`-style checking reads, so it has to be right for the two shapes that
/// exist: an action with arguments and one without.
#[test]
fn actions_declare_their_arguments() {
    let registry = ActionRegistry::builtin();
    assert_eq!(registry.get("jump").unwrap().arity(), 1);
    assert_eq!(registry.get("set").unwrap().arity(), 2);
    assert_eq!(registry.get("quit").unwrap().arity(), 0);
    assert_eq!(
        registry.get("play").unwrap().arg_names(),
        vec!["channel", "source"]
    );
}

/// A `jump` argument is a label, not a number — the distinction the checker needs to report
/// `set(trust, "yes")` without guessing.
#[test]
fn arguments_are_typed() {
    let registry = ActionRegistry::builtin();
    let jump = registry.get("jump").unwrap();
    assert_eq!(jump.args[0].ty, PropType::Target);
    let play = registry.get("play").unwrap();
    assert_eq!(play.args[1].ty, PropType::Asset);
}

#[test]
fn a_misspelled_action_gets_a_suggestion() {
    let registry = ActionRegistry::builtin();
    assert_eq!(registry.closest("jumpp"), Some("jump"));
    assert_eq!(registry.closest("toggl"), Some("toggle"));
    assert_eq!(registry.closest("teleport"), None, "too far to be a typo");
}

/// A registered action replaces a built-in in place, and a new one is appended — the two
/// things `CONVENTIONS.md §4` promises a registry gives you.
#[test]
fn a_plugin_action_is_a_registry_entry() {
    let mut registry = ActionRegistry::builtin();
    let before = registry.names();

    registry.register(ActionDecl {
        name: "set",
        args: &[],
        doc: "A project's own `set`.",
        dispatched: true,
    });
    assert_eq!(registry.names(), before, "a replacement moved the list");
    assert_eq!(registry.len(), 27, "a duplicate was added");
    assert_eq!(
        registry.get("set").unwrap().arity(),
        0,
        "the replacement did not win"
    );

    registry.register(ActionDecl {
        name: "teleport",
        args: &[vela_ui::widgets::PropDecl {
            name: "where",
            ty: PropType::Target,
            required: true,
            doc: "Where to teleport to.",
        }],
        doc: "A plugin's action.",
        dispatched: true,
    });
    assert_eq!(registry.len(), 28);
    assert_eq!(registry.names().last(), Some(&"teleport"));
}

#[test]
fn an_empty_registry_is_empty() {
    let registry = ActionRegistry::empty();
    assert!(registry.is_empty());
    assert!(registry.get("jump").is_none());
}

/// Every built-in is self-describing, which is what the docs and the hover are generated from.
#[test]
fn actions_are_self_describing() {
    let registry = ActionRegistry::builtin();
    for action in registry.names() {
        let decl = registry.get(action).unwrap();
        assert!(!decl.doc.is_empty(), "`{action}` has no doc");
        for argument in decl.args {
            assert!(!argument.doc.is_empty(), "`{action}`'s `{}`", argument.name);
        }
    }
}

/// An `action` line parses, and the checker accepts it on a button.
#[test]
fn an_action_line_parses_and_checks() {
    let source = "screen s:\n    button:\n        text \"Tell the truth\"\n        action jump(forest.confession)\n        enable_if trust > 3\n";
    let parsed = parse(FileId::from_raw(0), source);
    assert!(
        parsed.diagnostics.is_empty(),
        "{:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
    );

    let screens: Vec<&ScreenDecl> = parsed
        .program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Screen(screen) => Some(screen),
            _ => None,
        })
        .collect();
    let Some(screen) = screens.first() else {
        panic!("expected a screen");
    };
    let ScreenLine::Node(button) = &screen.body[0] else {
        panic!("expected a button");
    };
    assert_eq!(button.children.len(), 3, "text, action, enable_if");

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

/// `E5012` — a call whose name is not in the registry.
///
/// Before this, nothing consulted the registry: `action quitt()` was accepted and did nothing at all,
/// which is the kind of failure a reader blames on the engine.
#[test]
fn an_unknown_action_is_reported_with_a_suggestion() {
    let found =
        diagnostics("screen s:\n    button:\n        text \"Quit\"\n        action quitt()\n");
    let codes: Vec<&str> = found.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, vec!["E5012"]);
    let help = found[0].help.clone().unwrap_or_default();
    assert!(help.contains("quit"), "{help}");
}

/// A name nowhere near anything registered gets the code and no guess.
#[test]
fn an_unrecognisable_action_gets_no_suggestion() {
    let found =
        diagnostics("screen s:\n    button:\n        text \"Go\"\n        action teleport()\n");
    assert_eq!(found[0].code.as_str(), "E5012");
    assert!(found[0].help.is_none());
}

/// `E5013` — the right name, the wrong number of arguments.
#[test]
fn an_action_with_the_wrong_arity_is_reported() {
    assert_eq!(
        codes("screen s:\n    button:\n        text \"Go\"\n        action open_screen()\n"),
        vec!["E5013"]
    );
    assert_eq!(
        codes("screen s:\n    button:\n        text \"Go\"\n        action quit(\"now\")\n"),
        vec!["E5013"]
    );
}

/// An action *handed in* is a name, not a call, and there is nothing to check it against — the screen
/// does not know or care what the caller will pass.
#[test]
fn an_action_parameter_is_not_reported() {
    let source = "\
screen confirm(message, yes_action):
    column:
        text message
        button:
            text \"Yes\"
            action yes_action
";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

/// An action passed as the argument of a `use` is checked the same way: the mistake is the same one,
/// so it cannot depend on which position it was written in.
#[test]
fn an_action_inside_a_use_argument_is_checked() {
    let source = "\
screen row(do):
    button:
        text \"Go\"
        action do

screen menu:
    use row(quitt())
";
    assert_eq!(codes(source), vec!["E5012"]);
}

/// A loop's iterable is a *value*, not an action position.
///
/// `for i in range(6)` produces the sequence the body walks, so reading that call as an action
/// reported a producer as a misspelled one — `E5012: no action called `range`` — which is the same
/// misreading the condition walk had (`SCREENS.md §2.4`). An action written *inside* the iterable is
/// still checked, because a list's element may be one.
#[test]
fn a_loops_iterable_is_a_value_rather_than_an_action() {
    assert!(
        codes("screen s():\n    for i in range(6):\n        text \"x\"\n").is_empty(),
        "a producer is not an action"
    );
    assert_eq!(
        codes("screen s():\n    for a in [quitt()]:\n        text \"x\"\n"),
        vec!["E5012"],
        "an action written inside the iterable is still an action"
    );
}
