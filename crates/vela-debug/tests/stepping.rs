//! Stopping where a debugger was told to, and going back.
//!
//! `RUNTIME.md §9` puts the stop *policy* in the debugger and the machine pull-based, so these
//! tests are about that policy: which site a label or a line matches, what over/into/out mean
//! across a frame, and whether walking backwards lands on the state the forward run had.

mod common;

use common::program;
use vela_debug::{Breakpoints, Debuggee, Mode, StopReason};
use vela_span::FileId;
use vela_world::World;

/// A story with a local, a call, and three lines of dialogue across two labels.
const STORY: &str = "\
default trust: int = 0

fn bump(n: int) -> int:
    return n + 1

label start:
    var a: int = 1
    trust = bump(trust)
    \"First.\"
    trust = trust + 10
    \"Second.\"
    jump later

label later:
    \"Third.\"
    return
";

/// A story with enough commands to step back ten of them.
fn many() -> String {
    let mut story = String::from("default n: int = 0\n\nlabel start:\n");
    for line in 1..=14 {
        story.push_str(&format!("    n = n + 1\n    \"Line {line:02}.\"\n"));
    }
    story.push_str("    return\n");
    story
}

/// The 0-based line of the first line containing `needle`.
fn line_of(text: &str, needle: &str) -> u32 {
    text.lines()
        .position(|line| line.contains(needle))
        .expect("the needle is in the fixture") as u32
}

/// A label breakpoint stops the story where the label begins.
#[test]
fn a_label_breakpoint_stops_at_that_label() {
    let program = program(STORY);
    let mut debuggee = Debuggee::new(program.module(), program.entry()).expect("the entry exists");

    let mut breakpoints = Breakpoints::new();
    breakpoints.set_labels(["later".to_string()]);

    assert_eq!(
        debuggee.resume(&program, &breakpoints, Mode::Continue),
        StopReason::Breakpoint
    );
    assert_eq!(debuggee.site().expect("a site").body, "main.later");
}

/// A line breakpoint stops on the line it names, and no other.
#[test]
fn a_line_breakpoint_stops_on_that_line() {
    let program = program(STORY);
    let mut debuggee = Debuggee::new(program.module(), program.entry()).expect("the entry exists");

    let file = FileId::from_raw(0);
    let wanted = line_of(STORY, "\"Second.\"");
    let mut breakpoints = Breakpoints::new();
    breakpoints.set_lines(file, [wanted]);

    assert_eq!(
        debuggee.resume(&program, &breakpoints, Mode::Continue),
        StopReason::Breakpoint
    );
    let site = debuggee.site().expect("a site");
    let position = program
        .position_of(site.span.expect("a debug build has spans"))
        .expect("a position");
    assert_eq!(position.line, wanted);
}

/// Stepping over a call does not stop inside it.
#[test]
fn stepping_over_does_not_enter_a_call() {
    let program = program(STORY);
    let mut debuggee = Debuggee::new(program.module(), program.entry()).expect("the entry exists");
    let breakpoints = Breakpoints::new();

    let mut bodies = Vec::new();
    for _ in 0..20 {
        let reason = debuggee.resume(&program, &breakpoints, Mode::Over);
        if !matches!(reason, StopReason::Step) {
            break;
        }
        bodies.push(debuggee.site().expect("a site").body);
    }

    assert!(bodies.iter().any(|body| body == "main.start"), "{bodies:?}");
    assert!(
        !bodies.iter().any(|body| body == "main.bump"),
        "`next` must not descend into a call: {bodies:?}"
    );
}

/// Stepping into a call stops inside it, and stepping out leaves it.
#[test]
fn stepping_into_enters_a_call_and_out_leaves_it() {
    let program = program(STORY);
    let mut debuggee = Debuggee::new(program.module(), program.entry()).expect("the entry exists");
    let breakpoints = Breakpoints::new();

    // Walk until the frame we are in is the function.
    let mut inside = false;
    for _ in 0..30 {
        if debuggee.resume(&program, &breakpoints, Mode::Into) != StopReason::Step {
            break;
        }
        if debuggee.site().is_some_and(|site| site.body == "main.bump") {
            inside = true;
            break;
        }
    }
    assert!(inside, "stepping into the call never reached it");

    // One `out`, and we are back in the label that called it.
    assert_eq!(
        debuggee.resume(&program, &breakpoints, Mode::Out),
        StopReason::Step
    );
    assert_eq!(debuggee.site().expect("a site").body, "main.start");
}

/// Stepping back ten commands lands on exactly the state the forward run had there.
///
/// This is the M11 criterion, and it holds because rollback is *restore*, not undo
/// (`RUNTIME.md §7.1`): the ring keeps a snapshot per command, so going back is putting one
/// back rather than reversing the operations that came after it.
#[test]
fn stepping_back_reaches_the_state_the_forward_run_had() {
    let program = program(&many());
    let mut debuggee = Debuggee::new(program.module(), program.entry()).expect("the entry exists");
    let breakpoints = Breakpoints::new();

    let mut history: Vec<(u64, World)> = Vec::new();
    for _ in 0..60 {
        let reason = debuggee.resume(&program, &breakpoints, Mode::Over);
        if !matches!(reason, StopReason::Step) {
            break;
        }
        let position = debuggee.timeline().position();
        let world = debuggee.timeline().world().snapshot();
        if history.last().map(|(seen, _)| *seen) != Some(position) {
            history.push((position, world));
        }
    }

    assert!(
        history.len() >= 12,
        "the fixture has too few stops: {}",
        history.len()
    );

    // The position the story actually reached, which may be one past the last stop: the run that
    // ends by halting yields its final command on the way out.
    let last = debuggee.timeline().position();
    assert_eq!(debuggee.step_back(10), StopReason::Back);
    assert_eq!(
        debuggee.timeline().position(),
        last - 10,
        "ten commands backwards is where it should be"
    );

    let expected = history
        .iter()
        .rev()
        .find(|(position, _)| *position == last - 10)
        .expect("the forward run recorded this command")
        .1
        .clone();
    assert_eq!(
        debuggee.timeline().world().snapshot(),
        expected,
        "the restored world is byte-identical to the one the forward run had"
    );
}
