//! The action registry.
//!
//! `SCREENS.md §7` lists thirteen built-ins and says adding a fourteenth is a registry entry.
//! So the tests are about the *registry*: that the set is the documented one, that order is
//! stable, that a plugin's action is indistinguishable from a built-in, and that a misspelled
//! one gets a suggestion rather than silence.

use vela_span::FileId;
use vela_syntax::{Item, ScreenDecl, ScreenLine, parse};
use vela_ui::actions::ActionDecl;
use vela_ui::widgets::PropType;
use vela_ui::{ActionRegistry, WidgetRegistry, check_screen};

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
        "wait",
        "quit",
        "quick_save",
        "quick_load",
    ] {
        assert!(registry.get(name).is_some(), "`{name}` is missing");
    }
    assert_eq!(registry.len(), 13, "SCREENS.md §7 lists thirteen");
}

/// Order is registration order, for the same reason the widget registry's is: a completion
/// list, a docs page, and a golden all read this, and all three should be the same list twice.
#[test]
fn the_order_is_stable() {
    let first = ActionRegistry::builtin().names();
    assert_eq!(first, ActionRegistry::builtin().names());
    assert_eq!(first[0], "jump");
    assert_eq!(first.last(), Some(&"quick_load"));
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
    });
    assert_eq!(registry.names(), before, "a replacement moved the list");
    assert_eq!(registry.len(), 13, "a duplicate was added");
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
    });
    assert_eq!(registry.len(), 14);
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

    let diagnostics = check_screen(&screen.body, &WidgetRegistry::builtin(), &screens);
    assert!(
        diagnostics.is_empty(),
        "{:?}",
        diagnostics
            .iter()
            .map(|d| format!("{}: {}", d.code.as_str(), d.message))
            .collect::<Vec<_>>()
    );
}
