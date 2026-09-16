//! Reading a value out of a suspended story.
//!
//! `Vm::call` exists for one caller — `vela test`'s `expect` — and it is the only way to ask a question
//! about the world *as it is part way through a run* without moving the story. The tests here are about
//! the two halves of that promise: the value is the world's, and the story does not move.
//!
//! The label list is the other half of what the runner needs: `cover labels` is a question about a run,
//! and the machine is the only thing that knows which labels a playthrough went through.

mod common;

use common::compile;
use vela_vm::{Session, Step};
use vela_world::Input;

/// A story with a world variable, a helper function, and a suspension in the middle.
const STORY: &str = "\
default trust: int = 0

fn doubled() -> int:
    return trust + trust

label start:
    \"First.\"
    trust = trust + 1
    \"Second.\"
    return
";

/// A module compiled from `STORY`.
fn story() -> vela_bytecode::Module {
    compile("main", STORY)
}

/// A session started at `start` and advanced past its first command.
fn suspended() -> Session {
    let mut session = Session::start(&story(), "start").expect("the label exists");
    let step = session.advance();
    assert!(
        matches!(step, Step::Yield(_)),
        "the story says something first"
    );
    session
}

/// A function read from a suspended story sees the world as it is, not as it started.
#[test]
fn a_value_read_mid_story_sees_the_world_as_it_is() {
    let mut session = suspended();

    // `trust` is still zero: the line that increments it has not run yet, because the story is
    // waiting on the host to advance the first line of dialogue.
    assert_eq!(
        session.call("doubled").expect("the function is readable"),
        vela_world::Value::Int(0)
    );

    // Answer, and the increment runs before the second command.
    assert!(matches!(session.answer(Input::Ack), Step::Yield(_)));
    assert_eq!(
        session.call("doubled").expect("the function is readable"),
        vela_world::Value::Int(2),
        "the world moved, and reading it follows"
    );
}

/// Reading a value does not move the story: the command it was suspended on is still the one on
/// screen, and answering it is what moves on.
#[test]
fn reading_a_value_leaves_the_story_where_it_was() {
    let mut session = suspended();
    let before = session.current().cloned();
    assert_eq!(
        before.as_ref().map(ToString::to_string),
        Some("say \"First.\"".to_string()),
        "the helper left the first command on screen"
    );

    for _ in 0..3 {
        session.call("doubled").expect("the function is readable");
    }

    assert_eq!(
        session.current().cloned(),
        before,
        "a read consumed the suspension, which drops a command from the run"
    );

    // And the story still moves when it is answered, from where it was.
    assert!(matches!(session.answer(Input::Ack), Step::Yield(_)));
    assert_eq!(
        session.current().map(ToString::to_string),
        Some("say \"Second.\"".to_string())
    );
}

/// The function a read runs cannot be a piece of story: one that suspends is refused rather than
/// presented to a host that is not there.
#[test]
fn a_function_that_suspends_is_refused() {
    let module = compile(
        "main",
        "fn speaks() -> int:\n    \"a line\"\n    return 1\n\nlabel start:\n    return\n",
    );
    let mut session = Session::start(&module, "start").expect("the label exists");

    let fault = session.call("speaks").expect_err("a story is not a value");
    assert!(
        fault.to_string().contains("suspends"),
        "the message says why: {fault}"
    );
}

/// And one that takes parameters is refused too, rather than reading whatever happens to be on the
/// stack where its arguments should be.
#[test]
fn a_function_that_takes_parameters_is_refused() {
    let module = compile(
        "main",
        "fn add(n: int) -> int:\n    return n + 1\n\nlabel start:\n    return\n",
    );
    let mut session = Session::start(&module, "start").expect("the label exists");

    let fault = session.call("add").expect_err("it needs an argument");
    assert!(
        fault.to_string().contains("parameters"),
        "the message says why: {fault}"
    );
}

/// A name that is not in the module is a fault rather than a zero, so a runner that mistypes the
/// function it compiled learns immediately.
#[test]
fn a_name_that_is_not_there_is_refused() {
    let mut session = suspended();

    let fault = session.call("nope").expect_err("there is no such function");
    assert!(
        fault.to_string().contains("no such function"),
        "the message says why: {fault}"
    );
}

/// Every label the run goes through is reported once, in the order it was first entered.
#[test]
fn the_labels_a_run_went_through_are_reported() {
    let module = compile(
        "main",
        "label start:\n    jump middle\n\n\
         label middle:\n    jump end\n\n\
         label end:\n    return\n",
    );
    let mut session = Session::start(&module, "start").expect("the label exists");
    session.advance();

    assert_eq!(
        session.entered_labels(),
        vec!["start".to_string(), "middle".to_string(), "end".to_string()]
    );
}
