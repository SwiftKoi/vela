//! The widget registry, tested against its own contract.
//!
//! The point of a registry is that a caller can ask it things — does this widget exist, does
//! it take this prop, what is it called — and get answers the checker, the completion list,
//! and the docs all agree on. So these assert those answers, not that the built-ins happen to
//! be listed.

use vela_ui::widgets::PropType;
use vela_ui::widgets::schema::{COMMON, CONTAINER, PropDecl};
use vela_ui::{Category, Widget, WidgetRegistry};

#[test]
fn the_default_set_has_the_documented_widgets() {
    let registry = WidgetRegistry::builtin();
    for name in [
        "box", "row", "column", "stack", "flow", "grid", "absolute", "text", "image", "spacer",
        "button", "bar", "input",
    ] {
        assert!(registry.get(name).is_some(), "`{name}` is missing");
    }
    assert_eq!(registry.len(), 13);
}

/// The order is the registration order, not a hash order: a completion list, a docs page, and
/// a golden all read this, and all three should be the same list twice.
#[test]
fn the_order_is_stable() {
    let first = WidgetRegistry::builtin().names();
    let second = WidgetRegistry::builtin().names();
    assert_eq!(first, second);
    assert_eq!(first[0], "box");
    assert_eq!(first.last(), Some(&"input"));
}

#[test]
fn a_container_takes_children_and_a_leaf_does_not() {
    let registry = WidgetRegistry::builtin();
    assert!(registry.get("row").unwrap().category.accepts_children());
    assert!(registry.get("button").unwrap().category.accepts_children());
    assert!(!registry.get("text").unwrap().category.accepts_children());
    assert!(!registry.get("spacer").unwrap().category.accepts_children());
}

/// `gap` is a linear container's prop. Asking a leaf is how `E5006` is decided.
#[test]
fn props_are_answered_by_the_widget_that_declares_them() {
    let registry = WidgetRegistry::builtin();
    assert!(registry.get("row").unwrap().accepts("gap"));
    assert!(registry.get("grid").unwrap().accepts("columns"));
    assert!(!registry.get("text").unwrap().accepts("gap"));
    assert!(!registry.get("box").unwrap().accepts("columns"));
    // Shared props are accepted everywhere.
    for name in registry.names() {
        assert!(registry.get(name).unwrap().accepts("grow"), "{name}");
        assert!(registry.get(name).unwrap().accepts("anchor"), "{name}");
    }
}

/// A misspelled widget gets the nearest name, and an unrelated one gets nothing.
///
/// A wrong suggestion is worse than none: it sends someone looking in the wrong place.
#[test]
fn a_misspelled_widget_gets_a_suggestion() {
    let registry = WidgetRegistry::builtin();
    assert_eq!(registry.closest("roww"), Some("row"));
    assert_eq!(registry.closest("txet"), None, "too far to be a typo");
}

#[test]
fn an_unknown_widget_is_not_found() {
    let registry = WidgetRegistry::builtin();
    assert!(registry.get("canvas").is_none());
    assert!(registry.closest("canvas").is_none());
}

/// Registering a name twice replaces it, in place.
///
/// A project overriding `text` with its own is a legitimate thing to want, and a duplicate is
/// the only way to say it. Replacement must not reorder the list, or a style override would
/// silently move everything after it in the docs.
#[test]
fn a_replacement_keeps_its_place() {
    let mut registry = WidgetRegistry::builtin();
    let before = registry.names();

    registry.register(Widget {
        name: "text",
        category: Category::Leaf,
        single_child: false,
        common: COMMON,
        own: &[&[PropDecl {
            name: "font",
            ty: PropType::Word,
            required: false,
            doc: "A registered face.",
        }]],
    });

    assert_eq!(registry.names(), before, "the order changed");
    assert_eq!(registry.len(), 13, "a duplicate was added");
    assert!(registry.get("text").unwrap().accepts("font"));
    assert!(!registry.get("image").unwrap().accepts("font"));
}

/// A new widget appears where it was registered, and the built-ins do not move.
#[test]
fn a_new_widget_is_appended() {
    let mut registry = WidgetRegistry::builtin();
    registry.register(Widget {
        name: "live2d",
        category: Category::Leaf,
        single_child: false,
        common: COMMON,
        own: &[],
    });

    assert_eq!(registry.len(), 14);
    assert_eq!(registry.names().last(), Some(&"live2d"));
    assert!(registry.get("live2d").is_some());
}

/// Every prop a widget declares is reachable through the widget, and every declared type is
/// described — the two things the docs and the completion list are generated from.
#[test]
fn declared_props_are_self_describing() {
    let registry = WidgetRegistry::builtin();
    for widget in registry.iter() {
        for prop in widget.props() {
            assert!(!prop.name.is_empty(), "{}", widget.name);
            assert!(
                !prop.doc.is_empty(),
                "{}'s `{}` has no doc",
                widget.name,
                prop.name
            );
            assert!(!prop.ty.describe().is_empty());
        }
    }
}

/// `single_child` is what the checker rejects a second child with, so it has to be right for
/// the two widgets where it applies.
#[test]
fn only_the_single_child_containers_are_marked() {
    let registry = WidgetRegistry::builtin();
    assert!(registry.get("box").unwrap().single_child);
    assert!(registry.get("button").unwrap().single_child);
    assert!(!registry.get("row").unwrap().single_child);
    assert!(!registry.get("stack").unwrap().single_child);
}

#[test]
fn an_empty_registry_is_empty() {
    let registry = WidgetRegistry::empty();
    assert!(registry.is_empty());
    assert!(registry.get("row").is_none());
}

/// The shared container props are declared once and reachable from a container.
#[test]
fn container_props_are_shared_not_copied() {
    let registry = WidgetRegistry::builtin();
    let row = registry.get("row").unwrap();
    assert!(
        row.own
            .iter()
            .any(|group| group.iter().any(|prop| prop.name == "gap"))
    );
    assert!(CONTAINER.iter().any(|prop| prop.name == "pad"));
    assert!(
        row.accepts("pad"),
        "a container inherits the container props"
    );
}
