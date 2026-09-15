//! Rollback: replaying from a snapshot, and rewinding to take another branch.
//!
//! `RUNTIME.md §7`. The exit criterion this file exists for is *"rollback to any of 20 points
//! in a session replays exactly (asserted on `World` bytes)"* — so the assertion is on the
//! serialized world, not on a field a test could be wrong about.

mod common;

use common::compile;
use vela_replay::Timeline;
use vela_vm::Step;
use vela_world::Input;

/// A story that says `count` lines, so a session has many commands to roll back through.
fn lines(count: usize) -> String {
    let mut source = String::from("label start:\n");
    for index in 0..count {
        source.push_str(&format!("    \"Line {index}.\"\n"));
    }
    source.push_str("    return\n");
    source
}

/// A story that branches at a menu.
const BRANCHING: &str = "label start:\n    \"Before.\"\n    menu:\n        \"Left\":\n            jump left\n        \"Right\":\n            jump right\n\nlabel left:\n    \"Left.\"\n    return\n\nlabel right:\n    \"Right.\"\n    return\n";

/// The world as bytes, which is what "replays exactly" is checked against.
fn world_bytes(timeline: &Timeline) -> Vec<u8> {
    serde_json::to_vec(timeline.world()).expect("the world serializes")
}

/// The command on screen, rendered.
fn command(timeline: &Timeline) -> Option<String> {
    timeline.current().map(ToString::to_string)
}

/// **The criterion.** Roll back to each of 20 points and find the same world and command.
#[test]
fn rollback_replays_every_position_exactly() {
    let module = compile("many", &lines(20));
    let mut timeline = Timeline::with_interval(&module, "start", 4, 32).expect("start");

    // Record what each position looked like on the way forward.
    let mut expected: Vec<(Vec<u8>, Option<String>)> =
        vec![(world_bytes(&timeline), command(&timeline))];
    timeline.advance();
    expected.push((world_bytes(&timeline), command(&timeline)));
    while timeline.position() < 20 {
        timeline.answer(Input::Ack);
        expected.push((world_bytes(&timeline), command(&timeline)));
    }
    assert_eq!(expected.len(), 21, "positions 0..=20");

    // Roll back through them, newest first — the direction a player rolls.
    for target in (0..=20u64).rev() {
        assert_eq!(timeline.rollback(target), target);
        assert_eq!(
            world_bytes(&timeline),
            expected[target as usize].0,
            "the world at position {target} did not replay"
        );
        assert_eq!(
            command(&timeline),
            expected[target as usize].1,
            "the command at position {target} did not replay"
        );
    }
}

/// Snapshots are taken on the interval, which is what makes a rollback cheap.
#[test]
fn snapshots_are_taken_on_the_interval() {
    let module = compile("many", &lines(20));
    let mut timeline = Timeline::with_interval(&module, "start", 4, 32).expect("start");
    timeline.advance();
    while timeline.position() < 20 {
        timeline.answer(Input::Ack);
    }
    assert_eq!(timeline.snapshot_positions(), vec![0, 4, 8, 12, 16, 20]);
}

/// Rewinding discards the tail: it is the same mechanism as taking another branch.
#[test]
fn rolling_back_discards_the_tail() {
    let module = compile("many", &lines(20));
    let mut timeline = Timeline::with_interval(&module, "start", 4, 32).expect("start");
    timeline.advance();
    while timeline.position() < 20 {
        timeline.answer(Input::Ack);
    }
    assert_eq!(timeline.answers(), 19, "the answers to commands 1..19");

    assert_eq!(timeline.rollback(5), 5);
    assert_eq!(
        timeline.answers(),
        4,
        "the answers after the target are gone"
    );
    assert_eq!(
        timeline.snapshot_positions(),
        vec![0, 4],
        "snapshots after the target are gone"
    );
}

/// Rewind and take the other branch: the *same* mechanism, one answer different.
#[test]
fn a_rebranch_takes_the_other_branch() {
    let module = compile("branch", BRANCHING);
    let mut timeline = Timeline::with_interval(&module, "start", 4, 32).expect("start");

    timeline.advance(); // "Before."
    timeline.answer(Input::Ack); // the menu
    assert_eq!(timeline.position(), 2);
    let menu_position = timeline.position();

    // Take the first branch, as a player would.
    timeline.answer(Input::Choice(0));
    let first = command(&timeline).expect("a command after the choice");
    assert!(first.contains("Left."), "{first}");

    // Rewind to the menu and take the other one.
    let step = timeline.rebranch(menu_position, Input::Choice(1));
    assert!(matches!(step, Step::Yield(_)));
    let other = command(&timeline).expect("a command after the new choice");
    assert!(other.contains("Right."), "{other}");
    assert_ne!(other, first, "the rebranch did not change the story");
    assert_eq!(timeline.position(), 3);
}

/// A rollback past the end of history stops at the start rather than faulting.
#[test]
fn rolling_back_past_the_start_stops() {
    let module = compile("many", &lines(3));
    let mut timeline = Timeline::with_interval(&module, "start", 4, 32).expect("start");
    assert!(!timeline.can_rollback(), "nothing has happened yet");

    timeline.advance();
    timeline.answer(Input::Ack);
    assert!(timeline.can_rollback());

    assert_eq!(timeline.rollback(0), 0);
    assert_eq!(timeline.rollback(0), 0, "and it stays there");
    assert!(!timeline.can_rollback());
}
