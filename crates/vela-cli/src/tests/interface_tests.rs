//! The interface Vela ships, as the CLI hands it to a runner.
//!
//! `SCREENS.md §2.7`. Two things the CLI decides and nothing else can: that the interface's set is
//! *searched* (so a screen no project file declares still opens) and that it is laid out in the game's
//! colours. Both are about the wiring rather than about the drawing, which is `vela-ui`'s, and both would
//! be invisible from inside that crate.

use vela_ui::Value;

use super::support::{temp_project, text_engine};

/// The interface draws in the *game's* colours: its set is laid out with the project's palette over
/// Vela's (`SCREENS.md §2.7`).
///
/// The frame's background is the assertion, because that is the visible half of the rule: a project whose
/// theme names `bg` gets its own colour in the engine's frame, and a token the project does not name keeps
/// Vela's — which is why this project declares one token and not three.
#[test]
fn the_interface_draws_in_the_projects_colours() {
    let source = "theme ours:\n    color bg = 0x00ff00\n\nscreen menu:\n    text \"Ours\"\n\nlabel start:\n    \"Hi.\"\n    return\n";
    let project = temp_project("interface-palette", source);
    let collected = crate::commands::check::collect(&project).expect("a valid project");
    let screens = crate::commands::ui::Screens::load(&collected);

    let mut text = text_engine();
    let mut draw = vela_render::DrawList::new();
    let mut args = vela_ui::Args::new();
    args.set("title", Value::Str("Preferences".to_string()));
    assert!(
        screens.draw(
            "game_menu",
            &args,
            (1280, 720),
            &mut text,
            "sans",
            &mut draw
        ),
        "the interface's frame is declared"
    );

    let rect = draw.rects().next().expect("the frame's background");
    assert_eq!(
        rect.color,
        vela_render::Color::rgb(0x00, 0xff, 0x00),
        "the project's `bg`, not Vela's"
    );
}
