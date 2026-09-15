//! A save of a real run, loaded and continued.
//!
//! The unit tests above check the container against hand-built state. This compiles a story,
//! runs it to a suspension, saves it, loads it, and asserts the loaded session goes exactly
//! where the original did — the claim the whole format exists to support.

mod common;

use common::compile;
use vela_replay::Save;
use vela_vm::{Session, Step};
use vela_world::Input;

/// A story that suspends, branches, and then speaks again.
const STORY: &str = "label start:\n    \"Before.\"\n    menu:\n        \"Left\":\n            jump left\n        \"Right\":\n            jump right\n\nlabel left:\n    \"Left.\"\n    return\n\nlabel right:\n    \"Right.\"\n    return\n";

/// Saves at a suspension, loads the file, and continues — the same answers, the same state.
#[test]
fn a_saved_session_loads_and_continues() {
    let module = compile("branch", STORY);

    // Run to the menu, then save.
    let mut session = Session::start(&module, "start").expect("start");
    assert!(
        matches!(session.advance(), Step::Yield(_)),
        "the first line"
    );
    let bytes = Save::new(session.snapshot(), [0u8; 32], "quick")
        .to_bytes()
        .expect("encode");

    // Load the file into a fresh session.
    let loaded = Save::from_bytes(&bytes).expect("decode");
    let mut restored = Session::restore(&module, &loaded.snapshot).expect("restore");
    assert_eq!(
        restored.current(),
        session.current(),
        "the loaded session is waiting on the same command"
    );

    // Both answer the menu the same way and reach the same command, then the same world.
    assert_eq!(
        restored.answer(Input::Ack),
        session.answer(Input::Ack),
        "advancing past the first line"
    );
    assert_eq!(
        restored.answer(Input::Choice(1)),
        session.answer(Input::Choice(1)),
        "choosing the second option"
    );
    assert_eq!(restored.world(), session.world());
}
