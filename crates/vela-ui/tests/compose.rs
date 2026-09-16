//! Screen composition: `use`, `transclude`, and the three answers they have to give.
//!
//! `SCREENS.md §2`. A screen is a function, so `use` is a call and a caller's block is an argument
//! that happens to be lines. These tests are about what the checker says about a call, what tree the
//! call produces, and what the caller thereby depends on — because a composition that draws is a
//! composition whose reads are the caller's reads too.

use vela_diag::Diagnostic;
use vela_span::FileId;
use vela_syntax::{Item, ScreenDecl, parse};
use vela_text::{Font, TextEngine};
use vela_ui::{Args, Kind, Node, ScreenSet, Value, WidgetRegistry, check_screen, deps_of};

/// The bundled face, so text measures to something real.
fn engine() -> TextEngine {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);
    text
}

/// Every `screen` a source declares.
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

/// Every diagnostic a source's screens produce, the per-file cycle check included.
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
    let screens = screens_of(&parsed);
    let registry = WidgetRegistry::builtin();
    let mut found: Vec<Diagnostic> = screens
        .iter()
        .flat_map(|screen| check_screen(&screen.body, &registry, &screens))
        .collect();
    found.extend(vela_ui::compose::check_cycles(&screens));
    found
}

/// The codes a source produces, in order.
fn codes(source: &str) -> Vec<String> {
    diagnostics(source)
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect()
}

/// A built screen's tree.
fn built(source: &str, name: &str, args: &Args) -> Node {
    let mut text = engine();
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
        .build(name, args, &mut text, "sans", 1280.0)
        .expect("the screen is declared")
}

/// The text of a built node, if it is a text leaf.
fn text_of(node: &Node) -> Option<&str> {
    match &node.kind {
        Kind::Text { text, .. } => Some(text.as_str()),
        _ => None,
    }
}

/// A wrapper that transcludes, and a page that uses it — the shape of every application screen.
const COMPOSED: &str = "\
screen wrapper(title):
    box:
        text title
        transclude

screen page:
    use wrapper(\"Settings\"):
        text \"Hello.\"
";

/// The composition checks do not cry wolf on the thing they exist for.
#[test]
fn a_valid_composition_is_clean() {
    assert!(codes(COMPOSED).is_empty(), "{:?}", codes(COMPOSED));
}

/// `E5009` — a `use` naming a screen that is not declared, with the name it probably meant.
#[test]
fn using_an_undeclared_screen_is_reported() {
    let source = "screen settings:\n    text \"x\"\n\nscreen page:\n    use settigns\n";
    let found = diagnostics(source);
    assert_eq!(
        found.iter().map(|d| d.code.as_str()).collect::<Vec<_>>(),
        vec!["E5009"]
    );
    let help = found[0].help.clone().unwrap_or_default();
    assert!(help.contains("settings"), "{help}");
}

/// A name nowhere near anything declared gets the code and no guess.
#[test]
fn an_unrecognisable_screen_gets_no_suggestion() {
    let found = diagnostics("screen page:\n    use nowhere\n");
    assert_eq!(found[0].code.as_str(), "E5009");
    assert!(found[0].help.is_none());
}

/// `E5010` — more positional arguments than the screen has parameters.
#[test]
fn too_many_arguments_are_reported() {
    let source = "screen wrapper(a):\n    text a\n\nscreen page:\n    use wrapper(\"x\", \"y\")\n";
    assert_eq!(codes(source), vec!["E5010"]);
}

/// `E5010` — a named argument that is not a parameter of the used screen.
#[test]
fn an_unknown_parameter_name_is_reported() {
    let source = "screen wrapper(a):\n    text a\n\nscreen page:\n    use wrapper(title = \"x\")\n";
    assert_eq!(codes(source), vec!["E5010"]);
}

/// `W4012` — a block handed to a screen that never places it.
///
/// A warning rather than an error: the block is content the author wrote and the screen cannot draw
/// it, which is worth saying and not worth refusing a build over.
#[test]
fn a_block_that_goes_nowhere_is_reported() {
    let source = "\
screen wrapper(title):
    text title

screen page:
    use wrapper(\"x\"):
        text \"lost\"
";
    assert_eq!(codes(source), vec!["W4012"]);
}

/// A `transclude` with nothing passed draws nothing, and there is nothing to report: the author wrote
/// no content, so no content was lost.
#[test]
fn a_transclude_with_no_block_is_clean() {
    let source =
        "screen wrapper:\n    column:\n        transclude\n\nscreen page:\n    use wrapper\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

/// `E5011` — a screen that uses itself, which would draw forever.
#[test]
fn a_screen_that_uses_itself_is_reported() {
    assert_eq!(codes("screen a:\n    use a\n"), vec!["E5011"]);
}

/// `E5011` — and one that reaches itself the long way round, reported once per screen in the loop.
#[test]
fn an_indirect_cycle_is_reported() {
    let source = "screen a:\n    use b\n\nscreen b:\n    use a\n";
    assert_eq!(codes(source), vec!["E5011", "E5011"]);
}

/// The criterion for `use`: the used screen's body is drawn where the call is.
#[test]
fn a_used_screen_is_expanded() {
    let source =
        "screen inner:\n    text \"INNER\"\n\nscreen outer:\n    column:\n        use inner\n";
    let root = built(source, "outer", &Args::new());
    let column = &root.children[0];
    assert!(matches!(column.kind, Kind::Column));
    assert_eq!(column.children.len(), 1, "the used screen's one line");
    assert_eq!(text_of(&column.children[0]), Some("INNER"));
}

/// The criterion for `transclude`: the block lands exactly where the used screen places it, in
/// document order — not at the start or the end of the used screen's tree.
#[test]
fn a_block_lands_where_the_screen_transcludes() {
    let source = "\
screen wrapper:
    column:
        text \"TOP\"
        transclude
        text \"BOTTOM\"

screen page:
    use wrapper:
        button:
            text \"CLICK\"
";
    let root = built(source, "page", &Args::new());
    let column = &root.children[0];
    assert_eq!(column.children.len(), 3, "top, block, bottom");
    assert_eq!(text_of(&column.children[0]), Some("TOP"));
    assert!(matches!(column.children[1].kind, Kind::Box), "the block");
    assert_eq!(text_of(&column.children[2]), Some("BOTTOM"));
}

/// A block reads the scope it was *written* in, which is the caller's — otherwise a wrapper could not
/// hand its caller's data to the content it wraps.
#[test]
fn a_block_reads_the_callers_arguments() {
    let source = "\
screen wrapper:
    box:
        transclude

screen page(label):
    use wrapper:
        text label
";
    let mut args = Args::new();
    args.set("label", Value::Str("HELLO".to_string()));
    let root = built(source, "page", &args);
    let box_node = &root.children[0];
    assert_eq!(
        text_of(&box_node.children[0]),
        Some("HELLO"),
        "the block did not see the caller's argument"
    );
}

/// A `use`'s arguments bind positionally, and by name, exactly as the call reads.
#[test]
fn arguments_bind_positionally_and_by_name() {
    let source = "\
screen inner(a, b):
    column:
        text a
        text b

screen outer:
    use inner(\"first\", b = \"second\")
";
    let root = built(source, "outer", &Args::new());
    let column = &root.children[0];
    assert_eq!(text_of(&column.children[0]), Some("first"));
    assert_eq!(text_of(&column.children[1]), Some("second"));
}

/// A parameter the call does not mention keeps its default, evaluated on the callee's side.
#[test]
fn a_parameter_the_call_omits_keeps_its_default() {
    let source = "\
screen inner(a = \"DEFAULT\"):
    text a

screen outer:
    use inner
";
    let root = built(source, "outer", &Args::new());
    assert_eq!(text_of(&root.children[0]), Some("DEFAULT"));
}

/// What a used screen reads, the caller reads: a change to it makes the caller's frame stale.
#[test]
fn a_used_screens_reads_become_the_callers() {
    let source = "screen inner:\n    text \"[trust]\"\n\nscreen outer:\n    use inner\n";
    let parsed = parse(FileId::from_raw(0), source);
    let screens = screens_of(&parsed);
    let outer = screens
        .iter()
        .find(|screen| screen.name == "outer")
        .expect("the outer screen");
    let deps = deps_of(&screens, &outer.body);
    assert!(
        deps.contains("trust"),
        "a used screen's read was not the caller's: {:?}",
        deps.iter().collect::<Vec<_>>()
    );
}
