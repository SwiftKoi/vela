//! Stepping a story one instruction at a time, and what a debugger can see from there.
//!
//! `RUNTIME.md §9` says the machine executes one instruction on request and answers where it is
//! and what its frames hold. These tests are about the two promises that make a debugger
//! possible: that a stop *between* instructions is a real position with a source span, and that
//! a release module — which carries no spans — says so rather than inventing a line.

mod common;

use common::{compile, compile_with};
use vela_vm::{Session, Step};
use vela_world::{Input, Value};

/// A story with a local, a call, and a line of dialogue, so every shape of stop appears.
const STORY: &str = "\
fn twice(n: int) -> int:
    return n + n

label start:
    var a: int = 3
    \"A line.\"
    var b: int = twice(a)
    return
";

/// Every position the machine passes through, from the first instruction to the last.
fn sites(module: &vela_bytecode::Module) -> Vec<vela_vm::Site> {
    let mut session = Session::start(module, "start").expect("the label exists");
    let mut seen = Vec::new();

    for _ in 0..10_000 {
        if let Some(site) = session.site() {
            seen.push(site);
        }
        if matches!(session.step(), Step::Halt) {
            return seen;
        }
    }
    panic!("the story did not end");
}

/// A stop between instructions is a real position: it names the body, says whether it is a
/// label, and carries the source span the instruction came from.
#[test]
fn a_stop_between_instructions_is_a_position_on_the_stack() {
    let module = compile("main", STORY);
    let seen = sites(&module);

    assert!(
        seen.iter()
            .any(|site| site.body == "start" && site.label && site.depth == 0),
        "the label was never seen: {:?}",
        seen.iter()
            .map(|site| (site.body.clone(), site.depth))
            .collect::<Vec<_>>()
    );
    assert!(
        seen.iter()
            .any(|site| site.body == "twice" && !site.label && site.depth == 1),
        "the call was never seen from inside: {:?}",
        seen.iter()
            .map(|site| (site.body.clone(), site.depth))
            .collect::<Vec<_>>()
    );
    assert!(
        seen.iter().all(|site| site.span.is_some()),
        "a debug build carries a span for every instruction"
    );
}

/// The call stack is every frame, outermost first, with the depth each one sits at — which is
/// what "step over" and "step out" are defined against.
#[test]
fn the_call_stack_names_every_frame() {
    let module = compile("main", STORY);
    let mut session = Session::start(&module, "start").expect("the label exists");

    for _ in 0..10_000 {
        if session.site().is_some_and(|site| site.body == "twice") {
            let stack = session.call_stack();
            assert_eq!(stack.len(), 2, "a call is two frames: {stack:?}");
            assert_eq!(stack[0].body, "start");
            assert_eq!(stack[1].body, "twice");
            assert_eq!(stack[0].depth, 0);
            assert_eq!(stack[1].depth, 1);
            assert!(stack[0].label, "the outer frame is a label");
            return;
        }
        if matches!(session.step(), Step::Halt) {
            break;
        }
    }
    panic!("the story never entered `twice`");
}

/// A slot is shown under the name the author gave it, with the value it holds right now — the
/// half of `BYTECODE.md §5`'s debug info that makes a variable view readable.
#[test]
fn a_frames_slots_are_shown_by_name() {
    let module = compile("main", STORY);
    let mut session = Session::start(&module, "start").expect("the label exists");

    for _ in 0..10_000 {
        if session.site().is_some_and(|site| site.body == "twice") {
            let parameter = session
                .locals()
                .into_iter()
                .find(|local| local.name == "n")
                .expect("the parameter `n` is a named slot");
            assert_eq!(parameter.value, Value::Int(3));
            return;
        }
        if matches!(session.step(), Step::Halt) {
            break;
        }
    }
    panic!("the story never entered `twice`");
}

/// A release module knows *where* it is but not *what line* that was: the body and the
/// instruction are there, the span is not. That is what "trace hooks are compiled out" means —
/// a line breakpoint has nothing to match.
#[test]
fn a_module_without_debug_info_has_positions_but_no_spans() {
    let module = compile_with("main", STORY, false);
    assert!(
        !module.header.has_debug(),
        "`vela build --release` clears `Header::FLAG_DEBUG`"
    );

    let seen = sites(&module);
    assert!(!seen.is_empty(), "it still runs");
    assert!(
        seen.iter().all(|site| site.span.is_none()),
        "a release module reports no source map"
    );
    assert!(
        seen.iter()
            .any(|site| site.body == "start" && site.label && site.depth == 0),
        "and still names the body it is in"
    );

    // The slot names are debug info too, so they are withheld with the spans.
    let mut session = Session::start(&module, "start").expect("the label exists");
    for _ in 0..10_000 {
        if session.site().is_some_and(|site| site.body == "twice") {
            assert!(
                session.locals().iter().all(|local| local.name.is_empty()),
                "a release module carries no slot names"
            );
            return;
        }
        if matches!(session.step(), Step::Halt) {
            break;
        }
    }
    panic!("the story never entered `twice`");
}

/// Stepping to the end is the same run as driving to the end: the commands and the final state
/// are identical, so a debugged story is the story.
#[test]
fn stepping_is_the_run_it_stands_in_for() {
    let module = compile("main", STORY);

    let mut stepped = Session::start(&module, "start").expect("the label exists");
    let stepped_commands = drive_stepwise(&mut stepped);

    let mut driven = Session::start(&module, "start").expect("the label exists");
    let driven_commands = drive_commands(&mut driven);

    assert_eq!(stepped_commands, driven_commands);
    assert_eq!(
        stepped.world().snapshot(),
        driven.world().snapshot(),
        "stepping leaves the same world behind"
    );
}

/// Yields every command, executing one instruction at a time and answering each suspension.
fn drive_stepwise(session: &mut Session) -> Vec<String> {
    let mut commands = Vec::new();
    let mut answered = false;

    for _ in 0..10_000 {
        let step = if answered {
            answered = false;
            session.resume_step(Input::Ack)
        } else {
            session.step()
        };
        match step {
            Step::Yield(command) => {
                commands.push(command.to_string());
                answered = true;
            }
            Step::Halt => return commands,
            Step::Fault(fault) => panic!("the story faulted: {fault}"),
            Step::Continue => {}
        }
    }
    panic!("the story did not end");
}

/// Yields every command through the ordinary run-to-the-next-command API.
fn drive_commands(session: &mut Session) -> Vec<String> {
    let mut commands = Vec::new();
    let mut step = session.advance();

    for _ in 0..10_000 {
        match step {
            Step::Yield(command) => {
                commands.push(command.to_string());
                step = session.answer(Input::Ack);
            }
            Step::Halt => return commands,
            Step::Fault(fault) => panic!("the story faulted: {fault}"),
            Step::Continue => step = session.advance(),
        }
    }
    panic!("the story did not end");
}
