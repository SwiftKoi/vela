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

/// A screen's `jump` begins the game, which is how the interface's `Start` works (`SCREENS.md §2.7`).
///
/// The whole path, through a real player: the interface writes `jump(start)`, the dispatcher hands it to
/// the host (the stack says `NotOurs`, because a label is the program's), and `Player::jump` starts the
/// timeline again at that label — closing the screens that were open, or a menu would stay over the first
/// line of the game it just started. A label the program does not have answers `false`, which is the
/// difference between a button that says it could not and one that does nothing.
#[test]
fn a_screen_can_start_the_game() {
    let source = "screen menu:\n    column:\n        button:\n            text \"Start\"\n            action jump(start)\n\nlabel start:\n    \"One.\"\n    return\n";
    let project = temp_project("interface-jump", source);
    let collected = crate::commands::check::collect(&project).expect("a valid project");
    let (module, label, _) =
        crate::commands::run::compile_project(&collected, &[], &mut Vec::new())
            .expect("the project compiles");
    let screens = crate::commands::ui::Screens::load(&collected);
    let schema = crate::commands::ui::schema(&collected.files);

    let mut player = crate::commands::play::Player::new(
        &module,
        &label,
        (1280, 720),
        screens,
        project.join("saves"),
        schema,
        Vec::new(),
    )
    .expect("a player, which needs no window");

    assert!(player.jump("start"), "the label the game begins at");
    assert!(
        !player.jump("nosuchlabel"),
        "and a label no module declares is answered rather than assumed"
    );
}
