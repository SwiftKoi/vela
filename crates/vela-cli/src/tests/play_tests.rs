//! What a run waits for, which is what a player's keypresses land on.
//!
//! The visual check that found this was a series of frames: `vela run --capture-dir` writes one image
//! per command that changes the picture, and the sample's first frames were the backdrop, then the
//! scene, then the line — three presses to read one sentence. A stage direction is not something to
//! read, and the rule is not a property of the window: it is what "present" means, so it is asserted
//! here rather than through a window.

use crate::commands::play::waiting::waits_for_the_player;
use vela_world::Command;

#[test]
fn a_line_a_choice_and_a_click_wait_for_the_player() {
    assert!(waits_for_the_player(&Command::Say {
        speaker: None,
        attributes: Vec::new(),
        text: "Hello.".to_string(),
        options: Vec::new(),
        transition: None,
    }));
    assert!(waits_for_the_player(&Command::Menu {
        prompt: None,
        choices: Vec::new(),
    }));
    assert!(waits_for_the_player(&Command::WaitClick));
    assert!(waits_for_the_player(&Command::Pause { seconds: None }));
}

/// Everything else happens *now* — a scene, a sprite, a transition, a cue, and a timed pause.
///
/// The timed pause is the one worth naming: it is the clock's, and waiting for a key on it would make
/// `pause 2.0` a `pause` that also needs a click.
#[test]
fn a_stage_direction_does_not() {
    assert!(!waits_for_the_player(&Command::Stage {
        kind: vela_world::Stage::Scene,
        image: "bg.room".to_string(),
        attributes: Vec::new(),
        transforms: Vec::new(),
        transition: None,
    }));
    assert!(!waits_for_the_player(&Command::Stage {
        kind: vela_world::Stage::Show,
        image: "sylvie.green.normal".to_string(),
        attributes: Vec::new(),
        transforms: Vec::new(),
        transition: None,
    }));
    assert!(!waits_for_the_player(&Command::Stage {
        kind: vela_world::Stage::Hide,
        image: "sylvie".to_string(),
        attributes: Vec::new(),
        transforms: Vec::new(),
        transition: None,
    }));
    assert!(!waits_for_the_player(&Command::Transition {
        name: "fade".to_string(),
    }));
    assert!(!waits_for_the_player(&Command::Pause {
        seconds: Some(2.0),
    }));
    assert!(!waits_for_the_player(&Command::Audio {
        kind: vela_world::Audio::Play,
        channel: "music".to_string(),
        source: Some("illurock.opus".to_string()),
        looping: true,
        fade: None,
    }));
}
