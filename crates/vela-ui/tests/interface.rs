//! The interface Vela ships: its own screens, and the rule that lets a project take one over.
//!
//! `SCREENS.md §2.1`'s *a default interface, and a project overrides any part of it*. Three questions,
//! and each is one of the ways the mechanism can be wrong: the interface is a module the engine's own
//! suite checks (nobody else can — it is not one of the project's files), a project module can *use* its
//! screens, and a project's own declaration of a name is the one that is found.
//!
//! What an override is *not* is a rebinding: the project's declaration takes over the name, and a screen
//! the interface uses internally keeps using its own. That follows from §2.1's "a screen is a pure
//! function of its arguments" — a screen that drew differently depending on what the project happened to
//! declare is exactly the coupling the screen language rules out everywhere else — and it is why
//! `find` resolves the *file* first and the interface second, once, in one place.

use vela_span::FileId;
use vela_syntax::{Item, ScreenDecl, parse};
use vela_text::{Font, TextEngine};
use vela_ui::{
    ActionRegistry, Args, Kind, Node, ScreenSet, SemanticActions, WidgetRegistry, check_screen,
};

/// The bundled face, so text measures to something real.
fn engine() -> TextEngine {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);
    text
}

/// Every screen a parsed source declares.
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

/// A parsed fixture.
fn parsed(source: &str) -> vela_syntax::ParseResult {
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
    parsed
}

/// Every word a built node draws, in tree order.
fn words(node: &Node, out: &mut Vec<String>) {
    if let Kind::Text { text, .. } = &node.kind {
        out.push(text.clone());
    }
    for child in &node.children {
        words(child, out);
    }
}

/// The interface's own source is a checked module: `vela-ui` is the only layer that can see it.
///
/// Nothing else checks it. A project's files go through `vela check`, and the interface is not one of
/// them — so without this, a typo in Vela's own interface would be a runtime surprise in every game.
#[test]
fn the_interfaces_own_screens_pass_the_checker() {
    let screens = vela_ui::interface::decls();
    assert!(!screens.is_empty(), "the interface declares nothing");

    let registry = WidgetRegistry::builtin();
    let actions = ActionRegistry::builtin();
    let mut found = Vec::new();
    for screen in screens {
        found.extend(check_screen(
            &screen.body,
            &screen.params,
            &registry,
            screens,
            &actions,
            &SemanticActions::builtin(),
        ));
    }
    found.extend(vela_ui::compose::check_cycles(screens));
    assert!(
        found.is_empty(),
        "the interface does not check: {:?}",
        found
            .iter()
            .map(|diagnostic| (diagnostic.code.as_str(), diagnostic.message.clone()))
            .collect::<Vec<_>>()
    );
}

/// A project module can `use` a screen the interface declares.
///
/// The call is a *call*: the arguments are checked against the interface's declaration, the block lands
/// where the frame writes `transclude`, and the words it draws prove the composition happened. Nothing in
/// the project's file declares `game_menu`, so an implementation that looked only at the file would
/// report `E5009` here and draw nothing.
#[test]
fn a_project_screen_can_use_an_interface_screen() {
    let source = "\
screen preferences:
    use game_menu(\"Preferences\"):
        column:
            text \"Text speed\"
";
    let parsed = parsed(source);
    let screens = screens_of(&parsed);

    let mut found = Vec::new();
    let registry = WidgetRegistry::builtin();
    let actions = ActionRegistry::builtin();
    for screen in &screens {
        found.extend(check_screen(
            &screen.body,
            &screen.params,
            &registry,
            &screens,
            &actions,
            &SemanticActions::builtin(),
        ));
    }
    assert!(
        found.is_empty(),
        "`use game_menu` was not resolved: {:?}",
        found
            .iter()
            .map(|diagnostic| (diagnostic.code.as_str(), diagnostic.message.clone()))
            .collect::<Vec<_>>()
    );

    let mut text = engine();
    let node = ScreenSet::from_items(&parsed.program.items)
        .build(
            "preferences",
            &Args::new(),
            &mut Default::default(),
            &mut text,
            "sans",
            1280.0,
        )
        .expect("the screen is declared");
    let mut drawn = Vec::new();
    words(&node, &mut drawn);
    assert_eq!(
        drawn,
        ["Preferences", "Text speed", "Return"],
        "the frame's title, the caller's block, and the frame's own button"
    );
}

/// A mistyped interface name is a suggestion rather than silence.
#[test]
fn a_mistyped_interface_name_is_suggested() {
    let parsed = parsed("screen s():\n    use game_men(\"Title\")\n");
    let screens = screens_of(&parsed);
    let found = check_screen(
        &screens[0].body,
        &screens[0].params,
        &WidgetRegistry::builtin(),
        &screens,
        &ActionRegistry::builtin(),
        &SemanticActions::builtin(),
    );

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code.as_str(), "E5009");
    assert_eq!(found[0].help.as_deref(), Some("did you mean `game_menu`?"));
}

/// A project's own declaration of an interface name is the one found, and the interface's is not.
///
/// The order of the sets *is* the override rule (`vela-cli`'s `Screens` puts the project's first), so a
/// test that searched the other way round would prove nothing about overriding.
#[test]
fn a_projects_declaration_takes_the_name_over() {
    let source = "screen game_menu(title):\n    text \"Ours\"\n    text title\n";
    let parsed = parsed(source);
    let project = ScreenSet::from_items(&parsed.program.items);
    let sets = vec![project, vela_ui::interface::set()];

    let mut args = Args::new();
    args.set("title", vela_ui::Value::Str("Title".to_string()));

    let mut text = engine();
    let laid = vela_ui::ScreenSource::lay(
        sets.as_slice(),
        "game_menu",
        &args,
        &Default::default(),
        (1280, 720),
        &mut text,
        "sans",
    )
    .expect("a `game_menu` is found");
    let mut drawn = Vec::new();
    words(&laid.node, &mut drawn);
    assert_eq!(drawn, ["Ours", "Title"], "the project's frame, not Vela's");
}
