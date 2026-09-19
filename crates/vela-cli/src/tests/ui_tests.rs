//! Screens at the CLI boundary: loading, the stack, the accessibility sweep, hot reload, variables.
//!
//! None of these need a window or a GPU — loading a screen is compiling it, and the stack is
//! plain state — which is the part of the presenter that can be checked anywhere.

use std::path::Path;

use vela_ui::{Kind, Node, Value};

use super::support::{cli, temp_project, text_engine};

/// The text of the first `text` node, which is how a variable's value is visible in a laid screen.
///
/// The tree rather than the draw list, because a glyph in a draw list is an id in an atlas and not a
/// string: what a screen *says* is the tree's business (`SCREENS.md §3`).
fn first_text(node: &Node) -> Option<&str> {
    if let Kind::Text { text, .. } = &node.kind {
        return Some(text.as_str());
    }
    node.children.iter().find_map(first_text)
}

/// A screen's own variable is initialized by its `default`, written by the runtime, and drawn again
/// (`SCREENS.md §2.5`).
///
/// The whole loop in one test. The declaration gives the screen a name; a button's action arrives as a
/// *value* the screen resolved — `"mouse"`, not the word it was written as; the write lands in a store
/// the runtime holds; and the screen is laid out again with it. Keeping that store outside the screen is
/// what makes a write survive the next frame instead of being undone by the declaration that declared
/// the variable.

#[test]
fn a_write_to_a_screen_variable_lays_the_screen_out_again() {
    let source = "screen help:\n    default tab = \"keyboard\"\n    column:\n        text tab\n        button:\n            text \"Mouse\"\n            action set_screen_variable(tab, \"mouse\")\n\nlabel start:\n    \"Hi.\"\n    return\n";
    let project = temp_project("variable", source);
    let collected = crate::commands::check::collect(&project).expect("a valid project");
    let screens = crate::commands::ui::Screens::load(&collected);

    let mut text = text_engine();
    let mut stack = vela_ui::Stack::default();
    assert!(stack.open(screens.sets(), "help", &[], (1280, 720), &mut text, "sans"));
    let laid = |stack: &vela_ui::Stack| {
        let top = stack.top().expect("a screen is open");
        first_text(&top.laid.node).map(str::to_string)
    };
    assert_eq!(laid(&stack).as_deref(), Some("keyboard"));

    assert!(stack.set_variable(
        screens.sets(),
        "tab",
        Value::Str("mouse".to_string()),
        (1280, 720),
        &mut text,
        "sans",
    ));
    assert_eq!(laid(&stack).as_deref(), Some("mouse"));

    // A name the screen does not declare is not kept: the store is pruned to what the body declares, so
    // a pack that carries a stale name cannot leave a variable nothing can read (`E5017` is the static
    // half of the same rule).
    assert!(stack.set_variable(
        screens.sets(),
        "device",
        Value::Str("mouse".to_string()),
        (1280, 720),
        &mut text,
        "sans",
    ));
    assert!(
        stack
            .top()
            .expect("open")
            .laid
            .state
            .get("device")
            .is_none(),
        "an undeclared name was kept"
    );
    assert_eq!(laid(&stack).as_deref(), Some("mouse"));

    // And a write with nothing open is refused rather than silently doing nothing.
    assert_eq!(stack.close().as_deref(), Some("help"));
    assert!(!stack.set_variable(
        screens.sets(),
        "tab",
        Value::Str("keyboard".to_string()),
        (1280, 720),
        &mut text,
        "sans",
    ));
}

/// A project's screens are compiled for the presenter, and the dialogue call binds the line.
///
/// No GPU: loading a screen is compiling it, which is exactly the part that can be checked
/// without a display. Whether the pixels are right is `vela-ui`'s `paint` test.
#[test]
fn a_projects_dialogue_screen_is_loaded_and_bound() {
    let source = "screen dialogue(name: str?, line: str):\n    box at bottom:\n        text line\n\nlabel start:\n    \"Hi.\"\n    return\n";
    let project = temp_project("screens", source);
    let collected = crate::commands::check::collect(&project).expect("a valid project");

    let screens = crate::commands::ui::Screens::load(&collected);
    assert!(screens.has("dialogue"));
    assert!(!screens.has("settings"), "a screen that was not declared");

    let args = crate::commands::ui::Screens::dialogue(Some("Eileen"), "Hi.");
    assert_eq!(
        args.get("line"),
        Some(&vela_ui::Value::Str("Hi.".to_string()))
    );
    assert_eq!(
        args.get("name"),
        Some(&vela_ui::Value::Str("Eileen".to_string()))
    );
    // A narrator is `none`, which is what the screen's `if name is not none` branches on.
    assert_eq!(
        crate::commands::ui::Screens::dialogue(None, "Hi.").get("name"),
        Some(&vela_ui::Value::None)
    );
}

/// A project with no screens still loads — and the presenter falls back to its built-in box.
#[test]
fn a_project_without_screens_loads_nothing() {
    let project = temp_project("no-screens", "label start:\n    \"Hi.\"\n    return\n");
    let collected = crate::commands::check::collect(&project).expect("a valid project");

    let screens = crate::commands::ui::Screens::load(&collected);
    assert!(!screens.has("dialogue"));
}

/// `vela test --a11y` walks the focus order and passes a fully labelled screen.
#[test]
fn the_a11y_sweep_reports_the_focus_order() {
    let source = "screen menu:\n    box:\n        column:\n            button:\n                text \"Start\"\n                action quit()\n\nlabel start:\n    \"Hi.\"\n    return\n";
    let project = temp_project("a11y-ok", source);
    let (code, out) = cli(&["test", "--a11y", &project.to_string_lossy()]);

    assert_eq!(code, 0, "{out}");
    assert!(out.contains("screen menu: 1 focusable"), "{out}");
    assert!(out.contains("[0] button \"Start\""), "{out}");
    assert!(out.contains("0 unlabelled"), "{out}");
}

/// A focusable node with nothing to announce fails the sweep — `W4010` as a gate.
#[test]
fn the_a11y_sweep_fails_an_unlabelled_control() {
    let source = "screen menu:\n    box:\n        column:\n            button:\n                action quit()\n\nlabel start:\n    \"Hi.\"\n    return\n";
    let project = temp_project("a11y-bad", source);
    let (code, out) = cli(&["test", "--a11y", &project.to_string_lossy()]);

    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("menu (button)"),
        "the node should be named: {out}"
    );
    assert!(out.contains("1 unlabelled"), "{out}");
}

/// A directory that is not a project is refused, whichever mode was asked for.
///
/// This test used to assert that `vela test` refused to run *anything* until story tests landed. They
/// have: `vela test` runs a project's suite, and what it still refuses is a directory that is not a
/// project at all.
#[test]
fn test_outside_a_project_is_an_error() {
    let (code, out) = cli(&["test", "."]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("vela.toml"), "{out}");
}

/// Reloading after an edit picks up the new screen — what hot reload does once change is seen.
#[test]
fn a_reload_picks_up_an_edited_screen() {
    let source = "screen menu:\n    box:\n        column:\n            button:\n                text \"Start\"\n                action quit()\n\nlabel start:\n    \"Hi.\"\n    return\n";
    let project = temp_project("reload", source);
    let collected = crate::commands::check::collect(&project).expect("a valid project");
    let mut screens = crate::commands::ui::Screens::load(&collected);
    let mut text = text_engine();

    let before = vela_ui::ScreenSource::lay(
        screens.sets(),
        "menu",
        &vela_ui::Args::new(),
        &vela_ui::ScreenState::new(),
        (1280, 720),
        &mut text,
        "sans",
    )
    .expect("declared");
    assert_eq!(before.hotspots[0].action.name, "quit");

    let edited = source.replace("action quit()", "action close_screen()");
    std::fs::write(project.join("src").join("main.vela"), edited).expect("write the edit");

    let reloaded = screens.reload();
    assert_eq!(reloaded.errors, 0);
    assert_eq!(reloaded.screens, 1);
    let after = vela_ui::ScreenSource::lay(
        screens.sets(),
        "menu",
        &vela_ui::Args::new(),
        &vela_ui::ScreenState::new(),
        (1280, 720),
        &mut text,
        "sans",
    )
    .expect("still declared");
    assert_eq!(after.hotspots[0].action.name, "close_screen");
}

/// A half-written edit keeps the last good screens rather than blanking the window.
#[test]
fn a_broken_edit_keeps_the_last_good_screens() {
    let source = "screen menu:\n    box:\n        text \"Hi\"\n\nlabel start:\n    return\n";
    let project = temp_project("reload-bad", source);
    let collected = crate::commands::check::collect(&project).expect("a valid project");
    let mut screens = crate::commands::ui::Screens::load(&collected);
    assert!(screens.has("menu"));

    std::fs::write(
        project.join("src").join("main.vela"),
        "screen menu\n    this does not parse\n",
    )
    .expect("write the broken edit");

    let reloaded = screens.reload();
    assert_eq!(reloaded.errors, 1, "the broken file should be reported");
    assert!(screens.has("menu"), "the last good screens were dropped");
}

/// The screen stack: open, focus, paint, close — without a window.
#[test]
fn a_screen_stack_opens_focuses_and_paints() {
    let source = "screen pause:\n    layer ui\n    box background = 0x10121a:\n        column gap 12:\n            button:\n                text \"Resume\"\n                action close_screen()\n            button:\n                text \"Settings\"\n                action open_screen(settings)\n";
    let project = temp_project("stack", source);
    let collected = crate::commands::check::collect(&project).expect("a valid project");
    let screens = crate::commands::ui::Screens::load(&collected);

    let mut text = text_engine();
    let mut stack = vela_ui::Stack::default();
    assert!(stack.is_empty());

    assert!(stack.open(screens.sets(), "pause", &[], (1280, 720), &mut text, "sans"));
    // Focus starts on the first control and wraps at the end.
    assert_eq!(
        stack.focused().map(|action| action.name.as_str()),
        Some("close_screen")
    );
    assert!(stack.move_focus(1));
    assert_eq!(
        stack.focused().map(|action| action.name.as_str()),
        Some("open_screen")
    );
    assert_eq!(
        stack.focused().and_then(|action| action.first()),
        Some("settings")
    );
    assert!(stack.move_focus(1), "focus wraps to the first control");
    assert_eq!(
        stack.focused().map(|action| action.name.as_str()),
        Some("close_screen")
    );

    // Painting draws the screen's background and the runtime's focus highlight.
    let mut draw = vela_render::DrawList::new();
    stack.paint(&mut text, "sans", &mut draw, &vela_ui::ImageTable::new());
    assert!(
        draw.rects()
            .any(|rect| rect.color == vela_render::Color::rgb(0x10, 0x12, 0x1a)),
        "the screen's background was not painted"
    );
    assert!(draw.rect_count() >= 2, "no focus highlight was drawn");
    assert!(draw.glyph_count() > 0, "the buttons' text was not painted");

    assert_eq!(stack.close().as_deref(), Some("pause"));
    assert!(stack.is_empty());
}

/// The example's pause menu lays its buttons out without overlapping.
#[test]
fn the_examples_pause_menu_buttons_do_not_overlap() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/standard");
    let collected = crate::commands::check::collect(&dir).expect("the example is a project");
    let screens = crate::commands::ui::Screens::load(&collected);
    let mut text = text_engine();
    let laid = vela_ui::ScreenSource::lay(
        screens.sets(),
        "pause",
        &vela_ui::Args::new(),
        &vela_ui::ScreenState::new(),
        (1280, 720),
        &mut text,
        "sans",
    )
    .expect("pause is declared");

    assert!(
        laid.hotspots.len() >= 3,
        "the pause menu's buttons: {}",
        laid.hotspots.len()
    );
    for pair in laid.hotspots.windows(2) {
        let (above, below) = (&pair[0], &pair[1]);
        assert!(
            below.rect.y >= above.rect.y + above.rect.height,
            "buttons overlap: {above:?} then {below:?}"
        );
    }
}

/// Opening a screen that is not declared does nothing, rather than drawing an empty frame.
#[test]
fn opening_an_undeclared_screen_does_nothing() {
    let project = temp_project("stack-missing", "label start:\n    \"Hi.\"\n    return\n");
    let collected = crate::commands::check::collect(&project).expect("a valid project");
    let screens = crate::commands::ui::Screens::load(&collected);

    let mut text = text_engine();
    let mut stack = vela_ui::Stack::default();
    // `pause` is not a name nothing declares any more — the interface has one (`SCREENS.md §2.7`), which
    // is why this asks for a name no module and no interface has.
    assert!(!stack.open(
        screens.sets(),
        "nosuchscreen",
        &[],
        (1280, 720),
        &mut text,
        "sans"
    ));
    assert!(stack.is_empty());
    assert!(stack.focused().is_none());
    assert!(!stack.move_focus(1));
}

/// A screen answers its `key` bindings while it is open — and only the top screen is asked.
///
/// `SCREENS.md §2.3`: input arrives as a semantic action and a screen may answer one itself, which is
/// what makes a modal screen modal. A binding under another screen is behind it, like everything else
/// about that screen — the same rule focus follows.
#[test]
fn the_top_screen_answers_its_key_bindings() {
    let source = "\
screen confirm(message):
    column:
        text message
        button:
            text \"OK\"
            action close_screen()
    key cancel action close_screen()

screen credits:
    box:
        text \"Credits\"
    key cancel action quit()

label start:
    \"Hi.\"
    return
";
    let project = temp_project("keys", source);
    let collected = crate::commands::check::collect(&project).expect("a valid project");
    let screens = crate::commands::ui::Screens::load(&collected);
    let mut text = text_engine();
    let mut stack = vela_ui::Stack::default();

    assert!(stack.key_action("cancel").is_none(), "nothing is open");
    assert!(stack.open(
        screens.sets(),
        "confirm",
        &[],
        (1280, 720),
        &mut text,
        "sans"
    ));
    assert_eq!(
        stack
            .key_action("cancel")
            .map(|action| action.name.as_str()),
        Some("close_screen")
    );
    assert!(
        stack.key_action("advance").is_none(),
        "an action the screen does not bind was answered"
    );

    // The screen underneath is not asked: its binding is behind the one on top.
    assert!(stack.open(
        screens.sets(),
        "credits",
        &[],
        (1280, 720),
        &mut text,
        "sans"
    ));
    assert_eq!(
        stack
            .key_action("cancel")
            .map(|action| action.name.as_str()),
        Some("quit")
    );
    assert_eq!(stack.close().as_deref(), Some("credits"));
    assert_eq!(
        stack
            .key_action("cancel")
            .map(|action| action.name.as_str()),
        Some("close_screen"),
        "closing the top screen did not uncover the one under it"
    );
}

/// The pictures a player can draw are the ones the platform actually uploaded.
///
/// A build says how big a picture is and the platform says which texture it became, and neither knows
/// about the other — so the joining is where a picture can go missing. A name with no texture is left
/// out rather than given id zero, because id zero is a real texture and would draw the wrong picture
/// confidently (`SCREENS.md §3`).
#[test]
fn the_image_table_holds_only_uploaded_pictures() {
    let sizes = [
        ("bg.room".to_string(), 320, 180),
        ("art.cave".to_string(), 64, 48),
    ];
    // The platform uploaded one of the two: the other is a name the build knows and nothing draws.
    let table =
        crate::commands::play::images::table(&sizes, |name| (name == "bg.room").then_some(7));

    assert_eq!(table.len(), 1, "{:?}", table.names());
    let picture = table.get("bg.room").expect("the uploaded picture");
    assert_eq!(picture.texture, 7);
    assert_eq!((picture.width, picture.height), (320, 180));
    assert!(
        table.get("art.cave").is_none(),
        "a picture nobody uploaded entered the table"
    );
}

/// The semantic-action vocabulary is the host's, name for name.
///
/// `vela-ui` cannot depend on `vela-host` — a screen checker that pulled a windowing library in to
/// validate a name is the worse trade — so the list exists twice. This is the one crate that can see
/// both, so the assertion is what keeps them from drifting: a `key` the editor accepts and the window
/// cannot deliver is a binding that never fires, which is the failure the check exists to prevent.
#[test]
fn the_semantic_actions_match_the_hosts() {
    let ours = vela_ui::SemanticActions::builtin();
    let host: Vec<&str> = vela_host::Action::all()
        .iter()
        .map(|action| action.as_str())
        .collect();
    assert_eq!(ours.names(), host);
}

/// A setting a player chose is written beside their saves, and read back by the next session.
///
/// The exit criterion this exists for is *"a setting changed in one session is still set in the next,
/// and is not in the save file"* (`docs/roadmap/M12.2-game-interface.md`), and these are its two halves:
/// the write a player's session makes when a screen changes something, and the read a later one does at
/// startup. The *save* half — that the value is in no save and in no snapshot — is asserted in
/// `crates/vela-replay/tests/preferences.rs`, where the two lifetimes are visible.
#[test]
fn a_setting_survives_a_session() {
    let dir = std::env::temp_dir().join(format!("vela-settings-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let mut chosen = vela_world::Preferences::new();
    chosen.set("text_speed", vela_world::Value::Int(30));
    crate::commands::play::settings::write_settings(&dir, chosen.clone()).expect("write");

    let read = crate::commands::play::settings::read_settings(&dir).expect("read");
    assert_eq!(read.preferences, chosen, "the next session's settings");
    assert_eq!(read.version, vela_replay::SETTINGS_VERSION);

    // And a game whose player has never chosen anything simply has no file, which is not a failure.
    let _ = std::fs::remove_dir_all(&dir);
    assert!(crate::commands::play::settings::read_settings(&dir).is_none());
}
