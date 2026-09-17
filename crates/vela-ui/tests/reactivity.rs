//! Dependency sets and the layout cache.
//!
//! The exit criterion this exists for: *"a benchmark proves a static screen does not relayout
//! when unrelated state changes."* These are the assertions behind that benchmark, so a
//! regression fails as a test rather than as a timing that nobody reads.

use vela_span::FileId;
use vela_syntax::{Item, ScreenDecl, ScreenLine, parse};
use vela_ui::{Constraints, Kind, Node, Size, layout};
use vela_ui::{DepSet, ScreenCache, deps_of};

/// The body of a one-screen source.
///
/// Borrowed from the parsed program rather than owned: `ScreenLine` holds expressions, so
/// moving one out of its program would mean cloning a tree to save nothing.
fn body_of(parsed: &vela_syntax::ParseResult) -> &[ScreenLine] {
    assert!(parsed.diagnostics.is_empty(), "the fixture must parse");
    let Some(Item::Screen(screen)) = parsed.program.items.first() else {
        panic!("expected a screen");
    };
    &screen.body
}

/// Every `screen` a fixture declares, which is what `deps_of` needs to follow a `use`.
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

fn names(deps: &DepSet) -> Vec<String> {
    deps.iter().cloned().collect()
}

/// A screen that reads nothing has an empty dependency set — which is what makes it *static*.
#[test]
fn a_static_screen_depends_on_nothing() {
    let parsed = parse(
        FileId::from_raw(0),
        "screen s:\n    box:\n        text \"Hello.\"\n",
    );
    let body = body_of(&parsed);
    let deps = deps_of(&screens_of(&parsed), body);
    assert!(
        deps.is_empty(),
        "a screen with no bindings read {:?}",
        names(&deps)
    );
    assert!(
        !deps.is_hit_by(&["trust".to_string()]),
        "and nothing hits it"
    );
}

/// An interpolation is a read like any other.
///
/// `"Trust: {bind trust}"` depends on `trust`, and a walker that only looked at statements
/// would miss it — which is the stale-text bug this module exists to make impossible.
#[test]
fn an_interpolation_is_a_dependency() {
    let parsed = parse(
        FileId::from_raw(0),
        "screen s:\n    box:\n        text \"Trust: [trust]\"\n",
    );
    let body = body_of(&parsed);
    let deps = deps_of(&screens_of(&parsed), body);
    assert!(deps.contains("trust"), "{:?}", names(&deps));
}

/// A condition is read, and so is everything inside it.
#[test]
fn a_condition_and_its_body_are_both_read() {
    let parsed = parse(
        FileId::from_raw(0),
        "screen s:\n    box:\n        if show_choices:\n            text line\n",
    );
    let body = body_of(&parsed);
    let deps = deps_of(&screens_of(&parsed), body);
    assert!(deps.contains("show_choices"));
    assert!(deps.contains("line"));
}

/// A conditional's reads count even though the branch may not run: the layout has to be
/// rebuilt when the condition changes, or the branch appears with stale content.
#[test]
fn a_dependency_inside_a_conditional_still_counts() {
    let parsed = parse(
        FileId::from_raw(0),
        "screen s:\n    box:\n        if flag:\n            text \"[unused]\"\n",
    );
    let body = body_of(&parsed);
    assert!(deps_of(&screens_of(&parsed), body).contains("unused"));
}

/// A call's callee and arguments are both read.
#[test]
fn a_call_reads_its_callee_and_its_arguments() {
    let parsed = parse(
        FileId::from_raw(0),
        "screen s:\n    box:\n        text \"[format(trust)] [count]\"\n",
    );
    let body = body_of(&parsed);
    let deps = deps_of(&screens_of(&parsed), body);
    assert!(deps.contains("format"), "{:?}", names(&deps));
    assert!(deps.contains("trust"), "{:?}", names(&deps));
    assert!(deps.contains("count"), "{:?}", names(&deps));
}

/// A prop name on its own line is collected even though it usually reads nothing.
///
/// The over-approximation is deliberate: an extra name costs a relayout, and a missed read
/// shows stale text forever. `text line` and `stretch_x` are the same shape.
#[test]
fn a_bare_name_arg_is_treated_as_a_read() {
    let parsed = parse(
        FileId::from_raw(0),
        "screen s:\n    box stretch_x:\n        text line\n",
    );
    let body = body_of(&parsed);
    let deps = deps_of(&screens_of(&parsed), body);
    assert!(deps.contains("line"), "{:?}", names(&deps));
    assert!(deps.contains("stretch_x"), "{:?}", names(&deps));
}

/// **The criterion.** A static screen is not laid out again however much unrelated state moves.
#[test]
fn a_static_screen_never_relays_out() {
    let parsed = parse(
        FileId::from_raw(0),
        "screen s:\n    box:\n        text \"Hello.\"\n",
    );
    let body = body_of(&parsed);
    let node = Node::new(Kind::Box, vec![Node::measured(Size::new(10.0, 10.0))]);
    let mut cache = ScreenCache::new();

    for frame in 0..60 {
        let changed = vec![format!("field_{frame}")];
        cache.get_or_build(
            &screens_of(&parsed),
            body,
            &node,
            Constraints::new(100.0, 100.0),
            &changed,
            || layout(&node, Constraints::new(100.0, 100.0)),
        );
    }

    assert_eq!(
        cache.layouts(),
        1,
        "a static screen was laid out {} times over 60 frames",
        cache.layouts()
    );
}

/// A screen that reads a field *is* laid out again when that field changes — and only then.
#[test]
fn a_bound_screen_relays_out_only_for_its_own_field() {
    let parsed = parse(
        FileId::from_raw(0),
        "screen s:\n    box:\n        text \"[trust]\"\n",
    );
    let body = body_of(&parsed);
    let node = Node::new(Kind::Box, vec![Node::measured(Size::new(10.0, 10.0))]);
    let mut cache = ScreenCache::new();
    let constraints = Constraints::new(100.0, 100.0);

    let build = |cache: &mut ScreenCache, changed: &[String]| {
        cache.get_or_build(
            &screens_of(&parsed),
            body,
            &node,
            constraints,
            changed,
            || layout(&node, constraints),
        );
    };

    build(&mut cache, &[]);
    assert_eq!(cache.layouts(), 1);

    build(&mut cache, &["unrelated".to_string()]);
    assert_eq!(cache.layouts(), 1, "an unrelated change caused a layout");

    build(&mut cache, &["trust".to_string()]);
    assert_eq!(cache.layouts(), 2, "the bound field did not cause a layout");
}

/// A resize invalidates regardless of dependencies: the screen reads nothing, but the space
/// it is laid out in has changed.
#[test]
fn invalidating_forces_a_relayout() {
    let parsed = parse(
        FileId::from_raw(0),
        "screen s:\n    box:\n        text \"Hello.\"\n",
    );
    let body = body_of(&parsed);
    let node = Node::new(Kind::Box, vec![Node::measured(Size::new(10.0, 10.0))]);
    let constraints = Constraints::new(100.0, 100.0);
    let mut cache = ScreenCache::new();

    cache.get_or_build(&screens_of(&parsed), body, &node, constraints, &[], || {
        layout(&node, constraints)
    });
    assert_eq!(cache.layouts(), 1);
    cache.invalidate();
    assert!(cache.frame().is_none());
    cache.get_or_build(&screens_of(&parsed), body, &node, constraints, &[], || {
        layout(&node, constraints)
    });
    assert_eq!(cache.layouts(), 2);
}

/// `get` reports whether it reused, so a caller does not have to infer it from a count.
#[test]
fn a_hit_is_reported_as_one() {
    let parsed = parse(
        FileId::from_raw(0),
        "screen s:\n    box:\n        text \"[trust]\"\n",
    );
    let body = body_of(&parsed);
    let node = Node::new(Kind::Box, vec![Node::measured(Size::new(10.0, 10.0))]);
    let constraints = Constraints::new(100.0, 100.0);
    let mut cache = ScreenCache::new();
    cache.get_or_build(&screens_of(&parsed), body, &node, constraints, &[], || {
        layout(&node, constraints)
    });

    assert!(cache.get(&[]).is_some_and(|(_, reused)| reused));
    assert!(cache.get(&["trust".to_string()]).is_none());
}

/// An empty screen is a legal screen and depends on nothing.
#[test]
fn an_empty_screen_is_static() {
    let parsed = parse(FileId::from_raw(0), "screen s:\n    pass\n");
    let body = body_of(&parsed);
    assert!(deps_of(&screens_of(&parsed), body).is_empty());
}

/// A name read only in an `elif` or an `else` is a dependency like any other.
///
/// `SCREENS.md §8.2`: the set decides whether a screen relays out, and a walk that stopped at the
/// `then` arm would leave the screen showing stale text in the arms beside it — the failure this whole
/// module exists to make impossible.
#[test]
fn a_name_read_only_in_an_elif_is_a_dependency() {
    let parsed = parse(
        FileId::from_raw(0),
        "screen s:\n    if a:\n        text \"A\"\n    elif b:\n        text \"B\"\n    else:\n        text c\n",
    );
    let deps = deps_of(&screens_of(&parsed), body_of(&parsed));
    let read = names(&deps);
    assert!(deps.contains("a"), "the `if` condition: {read:?}");
    assert!(deps.contains("b"), "the `elif` condition: {read:?}");
    assert!(deps.contains("c"), "a read in the `else`: {read:?}");
}
