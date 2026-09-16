//! What could be typed at a position: `crate::completion`.
//!
//! Three lists with three rules, and the rules are what these tests are about rather than the contents:
//! a screen line wants widgets, a `jump` wants labels, and everywhere else wants what is in scope. The
//! one that matters most is the second — a list of *every* label in a project would pass a naive test
//! and be exactly the flat namespace the language exists to avoid.

use vela_compile::Session;

use crate::completion::{Item_, at};

/// A project whose three lists are all reachable from one file.
const MAIN: &str = "\
use chapters.forest as forest

struct Route:
    name: str

default trust: int = 0

screen pause:
    text \"hi\"

label start:
    var r = Route
    \"Hello.\"
    jump forest.clearing
";

const FOREST: &str = "\
label clearing:
    return
";

/// A label nothing imports: offering it anywhere would be offering a name that cannot be written.
const EXTRA: &str = "\
label hidden:
    return
";

/// The session, with `main` first.
fn project() -> Session {
    let mut session = Session::new();
    session.set_file("main.vela", MAIN);
    session.set_file("chapters/forest.vela", FOREST);
    session.set_file("extra.vela", EXTRA);
    session
}

/// The completions at the offset of `needle` in `main`.
fn at_needle(needle: &str) -> Vec<Item_> {
    let mut session = project();
    let file = session
        .file_named("main.vela")
        .expect("`main.vela` is in the session");
    let offset = MAIN.find(needle).expect("the fixture contains it") as u32;

    at(&mut session, file, offset)
}

/// The labels in a list, without their details.
fn labels(items: &[Item_]) -> Vec<&str> {
    items.iter().map(|item| item.label.as_str()).collect()
}

/// A screen body's line begins with a widget and nothing else, so that is what is offered there.
#[test]
fn a_screen_line_offers_widgets() {
    let items = at_needle("    text \"hi\"");

    assert!(
        labels(&items).contains(&"text"),
        "the widget vocabulary: {:?}",
        labels(&items)
    );
    assert!(
        items.iter().all(|item| item.detail == "widget"),
        "nothing that is not a widget: {:?}",
        labels(&items)
    );
    assert!(
        labels(&items).contains(&"column"),
        "and the ones this fixture does not use: {:?}",
        labels(&items)
    );
}

/// After a `jump`, what can be written is a label — this module's, and the imported ones qualified the
/// way a reference has to be written.
///
/// `hidden` is the assertion that matters: it is a real label in a real file of this project, and
/// offering it would be offering something the checker then refuses.
#[test]
fn a_transfer_target_offers_reachable_labels_only() {
    let items = at_needle("jump forest.clearing");
    let listed = labels(&items);

    assert!(listed.contains(&"start"), "this module's own: {listed:?}");
    assert!(
        listed.contains(&"forest.clearing"),
        "qualified through the alias, because that is how it must be written: {listed:?}"
    );
    assert!(
        listed.contains(&"forest"),
        "and the alias itself: {listed:?}"
    );
    assert!(
        !listed.contains(&"hidden"),
        "a label in a module nothing imports is not reachable: {listed:?}"
    );
}

/// Everywhere else, the names in scope — by the checker's rule, so the list and the diagnostics agree —
/// and the names the module declares.
#[test]
fn a_body_offers_its_locals_and_the_modules_declarations() {
    let items = at_needle("    \"Hello.\"");
    let listed = labels(&items);

    assert!(listed.contains(&"r"), "a local declared above: {listed:?}");
    assert!(listed.contains(&"Route"), "a declared struct: {listed:?}");
    assert!(listed.contains(&"trust"), "a declared default: {listed:?}");
    assert!(
        listed.contains(&"start"),
        "and a declared label: {listed:?}"
    );

    let detail = |name: &str| {
        items
            .iter()
            .find(|item| item.label == name)
            .map(|item| item.detail.as_str())
    };
    assert_eq!(detail("r"), Some("Route"), "with its type");
    assert_eq!(detail("trust"), Some("default"), "and with its kind");
}

/// The list is ordered by name, so an editor's popup does not reshuffle between keystrokes.
#[test]
fn the_list_is_ordered_by_name() {
    let items = at_needle("    \"Hello.\"");
    let listed = labels(&items);
    let mut sorted = listed.clone();
    sorted.sort_unstable();

    assert_eq!(listed, sorted, "{listed:?}");
}

/// A position that names nothing still has an answer: the names in scope there.
#[test]
fn a_position_without_context_offers_the_scope() {
    let items = at_needle("label start:");

    assert!(
        labels(&items).contains(&"trust"),
        "module declarations are visible from the top of the file: {:?}",
        labels(&items)
    );
}

/// `jump ` with nothing after it yet does not parse, and that is when an editor asks what can go there.
///
/// The fixture is deliberately a file that does not compile: the author is halfway through writing a
/// transfer, which is the state a completion list exists for.
#[test]
fn an_unfinished_transfer_still_offers_labels() {
    let source = "use chapters.forest\n\nlabel start:\n    jump \n";
    let mut session = Session::new();
    session.set_file("main.vela", source);
    session.set_file("chapters/forest.vela", FOREST);
    let file = session
        .file_named("main.vela")
        .expect("`main.vela` is in the session");

    let items = at(&mut session, file, offset_in(source, "jump ") + 5);
    let listed = labels(&items);

    assert!(
        listed.contains(&"chapters.forest.clearing"),
        "the module this file imports, qualified the way it has to be written: {listed:?}"
    );
    assert!(
        listed.contains(&"start"),
        "and this module's own labels: {listed:?}"
    );
}

/// The offset of `needle` in `text`.
fn offset_in(text: &str, needle: &str) -> u32 {
    text.find(needle).expect("the fixture contains it") as u32
}
