//! Screens at the CLI boundary: loading, the stack, the accessibility sweep, hot reload.
//!
//! None of these need a window or a GPU — loading a screen is compiling it, and the stack is
//! plain state — which is the part of the presenter that can be checked anywhere.

use std::path::Path;

use super::support::{cli, temp_project, text_engine};

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

/// Without a test mode, `vela test` says where story tests are, rather than pretending.
#[test]
fn test_without_a_mode_is_a_usage_error() {
    let (code, out) = cli(&["test", "."]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("M10"), "{out}");
}

/// Reloading after an edit picks up the new screen — what hot reload does once change is seen.
#[test]
fn a_reload_picks_up_an_edited_screen() {
    let source = "screen menu:\n    box:\n        column:\n            button:\n                text \"Start\"\n                action quit()\n\nlabel start:\n    \"Hi.\"\n    return\n";
    let project = temp_project("reload", source);
    let collected = crate::commands::check::collect(&project).expect("a valid project");
    let mut screens = crate::commands::ui::Screens::load(&collected);
    let mut text = text_engine();

    let before = screens
        .lay(
            "menu",
            &vela_ui::Args::new(),
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
    let after = screens
        .lay(
            "menu",
            &vela_ui::Args::new(),
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
    let mut stack = crate::commands::ui::Stack::default();
    assert!(stack.is_empty());

    assert!(stack.open(&screens, "pause", (1280, 720), &mut text, "sans"));
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
    stack.paint(&mut text, "sans", &mut draw);
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
    let laid = screens
        .lay(
            "pause",
            &vela_ui::Args::new(),
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
    let mut stack = crate::commands::ui::Stack::default();
    assert!(!stack.open(&screens, "pause", (1280, 720), &mut text, "sans"));
    assert!(stack.is_empty());
    assert!(stack.focused().is_none());
    assert!(!stack.move_focus(1));
}
