//! What the checker knows about a position: `vela_types::at`.
//!
//! The question a hover asks, and the reason it is answered by the checker's own walk rather than by a
//! second one: the scope rules — a `var` is visible from where it is written to the end of the body —
//! are the part that would drift, and a hover that called a name unknown where the checker accepts it
//! would contradict the diagnostics in the same window.

use vela_span::FileId;
use vela_syntax::parse;

use crate::{Env, Found, at};

/// The fixture: a module with a local, a parameter, and expressions of several shapes.
const SOURCE: &str = "\
struct Route:
    name: str

fn spend(trust: int, name: str) -> int:
    var bonus = trust + 1
    return bonus

default saved: int = 0

label start:
    var r = Route
    var title: str = r.name
    var total = spend(saved, title)
    \"total is [total]\"
    return
";

/// What is at the offset of `needle` in the fixture.
///
/// `find` returns the first occurrence, so the tests below assert about the *start* of what they name —
/// which matters more than it sounds: pointing at `r` in `r.name` answers about `r`, because the
/// innermost expression containing that offset is `r` and not the field access that contains *it*.
fn found(needle: &str) -> Option<Found> {
    at_offset(offset_of(needle))
}

/// What is at an offset into the fixture.
fn at_offset(offset: u32) -> Option<Found> {
    let parsed = parse(FileId::from_raw(0), SOURCE);
    let (env, _) = Env::build(&parsed.program);
    at(&parsed.program, &env, offset)
}

/// The offset of `needle` in the fixture.
fn offset_of(needle: &str) -> u32 {
    SOURCE.find(needle).expect("the fixture contains it") as u32
}

/// A local's type is inferred from its initialiser when it was not written down — which is the answer
/// that needs the checker, since nothing in the tree says what `spend(saved, title)` returns.
#[test]
fn a_local_reports_its_inferred_type() {
    let found = found("total = spend").expect("a local");
    assert_eq!(found.describe(), "total: int");
}

#[test]
fn a_local_with_a_written_type_reports_that() {
    let found = found("title: str").expect("a local with an annotation");
    assert_eq!(found.describe(), "title: str");
}

/// Hovering the initialiser answers about the *expression*, not about the local it is stored in.
#[test]
fn an_initialiser_is_its_own_answer() {
    // `r.name`: the caret on `r` is about `r` — a `Route` — and the caret inside `name` is about the
    // field, which is a `str`. Two characters to the right is a different answer, which is the point.
    assert_eq!(found("r.name").expect("a name").describe(), "Route");
    assert_eq!(
        at_offset(offset_of("r.name") + 2)
            .expect("a field")
            .describe(),
        "str"
    );
}

#[test]
fn a_parameter_reports_its_declared_type() {
    let found = found("trust: int, name").expect("a parameter");
    assert_eq!(found.describe(), "trust: int");
}

/// The innermost expression containing the offset wins, which is what a reader pointing at a word
/// means: `spend` is a function value, and the parenthesis after it belongs to the call.
#[test]
fn the_innermost_expression_wins() {
    assert_eq!(
        found("spend(saved").expect("a callee").describe(),
        "fn(int, str) -> int",
        "a caret on the name is about the function"
    );
    assert_eq!(
        at_offset(offset_of("spend(saved") + 5)
            .expect("a call")
            .describe(),
        "int",
        "a caret on the parenthesis is about the call"
    );
}

/// A `default` is a declaration, not a body, so the checker has nothing to say about it — and saying
/// nothing is the right answer rather than a guess. The language server answers for those from the
/// symbol index, which knows what kind of thing a name is.
#[test]
fn a_declaration_outside_a_body_has_no_type_answer() {
    assert_eq!(found("saved: int = 0"), None);
}

/// The names a completion may offer at an offset: everything a body has introduced *so far*.
///
/// "So far" is the whole point. A list that included a `var` declared further down would offer a name
/// the checker has not introduced yet, and the author's first lesson would be that the list lies.
#[test]
fn the_scope_at_an_offset_holds_what_was_declared_before_it() {
    let names = scope_at_offset(offset_of("var total = spend"));

    let listed: Vec<&str> = names.iter().map(|(name, _)| name.as_str()).collect();
    assert!(listed.contains(&"r"), "declared earlier: {listed:?}");
    assert!(listed.contains(&"title"), "declared earlier: {listed:?}");
    assert!(
        !listed.contains(&"total"),
        "not declared yet where it is being written: {listed:?}"
    );
}

/// A parameter is in scope from the start of the body, and a local of the *same* name as a later
/// statement is not there yet.
#[test]
fn a_parameter_is_in_scope_and_carries_its_type() {
    let names = scope_at_offset(offset_of("return bonus"));

    let bonus = names.iter().find(|(name, _)| name == "bonus");
    assert_eq!(bonus.map(|(_, ty)| ty.to_string()).as_deref(), Some("int"));
    let trust = names.iter().find(|(name, _)| name == "trust");
    assert_eq!(trust.map(|(_, ty)| ty.to_string()).as_deref(), Some("int"));
}

/// Names come back in a fixed order, so an editor's list does not reshuffle between keystrokes.
#[test]
fn the_scope_is_ordered_by_name() {
    let names = scope_at_offset(offset_of("return bonus"));
    let listed: Vec<&str> = names.iter().map(|(name, _)| name.as_str()).collect();
    let mut sorted = listed.clone();
    sorted.sort_unstable();

    assert_eq!(listed, sorted, "{listed:?}");
}

/// What is in scope at an offset.
fn scope_at_offset(offset: u32) -> Vec<(String, crate::Ty)> {
    let parsed = parse(FileId::from_raw(0), SOURCE);
    let (env, _) = Env::build(&parsed.program);
    crate::scope_at(&parsed.program, &env, offset)
}
