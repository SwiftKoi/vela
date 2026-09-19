//! The player's settings have a lifetime of their own (`RUNTIME.md §2.1`).
//!
//! Four ways a setting could quietly become the story's, and each is a rule here: a **save** that
//! carried one would hand it to whoever loads the file, a **snapshot** that carried one would undo a
//! change on rollback, a **rollback** that reset them would lose them mid-session, and a **load**
//! that took them from the file would overwrite the player's own with the choices of whoever wrote
//! the save. The assertions are on the store rather than on a message, because the rule is about
//! what the values are, not about what a failure says.

mod common;

use common::compile;
use vela_replay::{Save, Timeline};
use vela_world::{Preferences, Value};

/// A story with enough commands to roll back through.
const STORY: &str =
    "label start:\n    \"One.\"\n    \"Two.\"\n    \"Three.\"\n    \"Four.\"\n    return\n";

/// A timeline with a setting set, the way a settings screen sets one.
fn played() -> (vela_bytecode::Module, Timeline) {
    let module = compile("settings", STORY);
    let mut timeline = Timeline::start(&module, "start").expect("start");
    timeline.advance();
    timeline.advance();
    timeline.preferences_mut().set("text_speed", Value::Int(30));
    (module, timeline)
}

/// A save is the story's, so the setting is not in the file — not in the decoded world, and not in
/// the bytes a player can look at.
#[test]
fn a_save_carries_no_settings() {
    let (_, timeline) = played();
    let bytes = Save::new(timeline.snapshot(), [0u8; 32], "quick")
        .to_bytes()
        .expect("encode");

    let loaded = Save::from_bytes(&bytes).expect("decode");
    assert!(
        loaded.snapshot.world.preferences.is_empty(),
        "a save carries no settings"
    );
    assert!(
        !String::from_utf8_lossy(&bytes).contains("text_speed"),
        "the setting is not in the file either"
    );
}

/// A snapshot is a rollback point, and a rollback must not undo a setting — so a snapshot has none.
#[test]
fn a_snapshot_carries_no_settings() {
    let (_, timeline) = played();

    assert!(
        timeline.snapshot().world.preferences.is_empty(),
        "a snapshot is the story's state"
    );
}

/// A rollback rewinds the story and leaves the settings where the player left them: the settings are
/// taken from the session it is replacing, not from the snapshot it restores.
#[test]
fn a_rollback_keeps_the_settings() {
    let (_, mut timeline) = played();

    assert_eq!(timeline.rollback(1), 1, "back to the second command");
    assert_eq!(
        timeline.world().preferences.get("text_speed"),
        Some(&Value::Int(30)),
        "a rollback is not an undo for the player's choices"
    );
}

/// A load restores the *story* from the file and takes the settings from whoever the load is for.
#[test]
fn a_resume_takes_the_players_settings() {
    let (module, timeline) = played();
    let snapshot = timeline.snapshot();

    // Another player of the same build: their own settings, and the story where the save was.
    let mut theirs = Preferences::new();
    theirs.set("text_speed", Value::Int(10));
    let resumed = Timeline::resume(&module, &snapshot, theirs).expect("resume");

    assert_eq!(
        resumed.world().preferences.get("text_speed"),
        Some(&Value::Int(10)),
        "the settings are the player's, not the save's"
    );
    assert_eq!(
        resumed.current(),
        timeline.current(),
        "and the story is where the snapshot left it"
    );
}
