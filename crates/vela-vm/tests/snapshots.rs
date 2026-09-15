//! Snapshots: a machine state written down, and put back.
//!
//! `RUNTIME.md §5` and §7. A save outlives the build that wrote it, so what a frame records
//! has to be as durable as the story: the body *by name*, and the suspension by the statement
//! it is waiting at rather than by where that statement happened to land in the instruction
//! stream. Both of those are recompile-proof; the instruction index is not, and is carried
//! only as a hint.

mod common;

use common::compile;
use vela_bytecode::Module;

/// A story with a branch, so a suspended session has somewhere to go next.
const MENU_STORY: &str = "label start:\n    \"Before.\"\n    menu:\n        \"Left\":\n            jump left\n        \"Right\":\n            jump right\n\nlabel left:\n    \"Left.\"\n    return\n\nlabel right:\n    \"Right.\"\n    return\n";

/// A story with a call, so a snapshot has more than one frame.
const NESTED: &str = "label start:\n    call sub\n    \"Back.\"\n    return\n\nlabel sub:\n    \"In.\"\n    return\n";

/// A snapshot restores the machine exactly: the same answers reach the same state.
#[test]
fn a_snapshot_restores_and_resumes() {
    let module = compile("branch", MENU_STORY);

    let mut session = vela_vm::Session::start(&module, "start").expect("start");
    session.advance();
    let snapshot = session.snapshot();
    assert!(
        snapshot.current.is_some(),
        "a suspension leaves the command to re-present"
    );

    let mut restored = vela_vm::Session::restore(&module, &snapshot).expect("restore");
    assert_eq!(restored.current(), session.current());
    assert!(
        restored.log().is_empty(),
        "rollback history is not persisted"
    );

    // The two sessions answer identically and stay identical.
    assert_eq!(
        restored.answer(vela_world::Input::Ack),
        session.answer(vela_world::Input::Ack)
    );
    let answered = vela_world::Input::Choice(1);
    assert_eq!(restored.answer(answered.clone()), session.answer(answered));
    assert_eq!(restored.world(), session.world());
}

/// Frames are named, so a snapshot restores into a module compiled separately from the one
/// that wrote it.
#[test]
fn a_snapshot_names_its_frames() {
    let module = compile("nested", NESTED);
    let mut session = vela_vm::Session::start(&module, "start").expect("start");
    session.advance();
    let snapshot = session.snapshot();

    let bodies: Vec<&str> = snapshot
        .vm
        .frames
        .iter()
        .map(|frame| frame.body.as_str())
        .collect();
    assert_eq!(bodies, vec!["label:start", "label:sub"]);

    // A second compile stands in for another build. What resolves a frame is its *name*, not
    // an index into the function table, which is what lets the body be compiled differently.
    // Relocating the suspension *inside* the body is the neighbouring test's job.
    let recompiled = compile("nested", NESTED);
    let mut restored = vela_vm::Session::restore(&recompiled, &snapshot).expect("restore");
    let step = restored.answer(vela_world::Input::Ack);
    assert!(
        matches!(step, vela_vm::Step::Yield(_)),
        "expected the line after the call, got {step:?}"
    );
}

/// A story whose body shrinks when the optimizer folds the `if` away, which moves every
/// instruction after it — including the suspension the test takes.
const FOLDED: &str = "label start:\n    if 1 == 1:\n        \"Always.\"\n    \"After.\"\n    \"Last.\"\n    return\n";

/// Compiles a fixture at an optimization level.
///
/// The same source built two ways is what a *later build of the engine* looks like: the
/// instruction stream moves, the story does not.
fn compile_at(name: &str, text: &str, level: vela_mir::OptLevel) -> Module {
    let mut sources = vela_span::SourceMap::new();
    let id = sources.add(name, text);
    let parsed = vela_syntax::parse(id, text);
    assert!(parsed.diagnostics.is_empty(), "the fixture does not parse");

    let (env, _) = vela_types::Env::build(&parsed.program);
    let lowered = vela_mir::lower(&vela_hir::ModuleName::new(name), &parsed.program, &env);
    assert!(lowered.diagnostics.is_empty(), "the fixture does not lower");

    let mut ir = lowered.module;
    vela_mir::Pipeline::at(level).run(&mut ir);
    let module = vela_bytecode::compile(&ir, true);
    assert!(
        vela_bytecode::verify(&module).is_empty(),
        "the module does not verify"
    );
    module
}

/// A save written before the body moved under it still resumes where it was waiting.
///
/// This is the recompile that naming a frame does *not* save you from: the name still
/// resolves, and the index the save recorded now points into the middle of something else.
/// Folding `if 1 == 1` away drops the block the constant branch was in, so the same source
/// built at two optimization levels lays its instructions out differently — which is what a
/// later build of the engine looks like. What finds the suspension is the statement's source
/// range, and no amount of emitting differently changes a statement.
#[test]
fn a_save_survives_the_body_moving_under_it() {
    let built = compile_at("folded", FOLDED, vela_mir::OptLevel::None);
    let rebuilt = compile_at("folded", FOLDED, vela_mir::OptLevel::O2);

    // Suspend at the third line: the folded block is emitted before it, so its index moves.
    let mut session = vela_vm::Session::start(&built, "start").expect("start");
    session.advance();
    session.answer(vela_world::Input::Ack);
    let snapshot = session.snapshot();
    let frame = &snapshot.vm.frames[0];
    let recorded = frame.ip as usize;
    let anchor = frame
        .resume
        .expect("a suspension is anchored to its statement");

    // The premise, asserted rather than assumed. In the second build the recorded index is
    // not a suspension, so an index-only restore would resume in the middle of the folded
    // branch and run a different story from the same save.
    let body = &rebuilt.labels[0];
    assert_eq!(
        rebuilt.strings.get(body.name),
        Some("start"),
        "the fixture has one label"
    );
    assert_ne!(
        built.labels[0].code.len(),
        body.code.len(),
        "the two builds must differ, or this test proves nothing"
    );
    assert_ne!(
        body.code.get(recorded - 1).map(|instr| instr.op),
        Some(vela_bytecode::Op::Yield),
        "the recorded index must not be a suspension in the new build"
    );

    let mut restored = vela_vm::Session::restore(&rebuilt, &snapshot).expect("restore");
    assert_eq!(
        restored.current(),
        session.current(),
        "the same line re-presents"
    );

    let vela_vm::Step::Yield(command) = restored.answer(vela_world::Input::Ack) else {
        panic!("expected the last line");
    };
    assert!(
        command.to_string().contains("Last."),
        "the anchored restore resumed at the wrong statement: {command} \
         (anchored at {}..{})",
        anchor.start,
        anchor.end
    );
}

/// A save whose story changed under it is refused, not resumed at whatever moved into place.
///
/// A line added before the suspension is the edit this is for: the instruction moves *and* the
/// statement's source range moves, so there is nothing to anchor to and no honest way to guess
/// which statement the save meant.
#[test]
fn a_save_for_a_changed_story_is_refused() {
    let before = compile(
        "edited",
        "label start:\n    \"One.\"\n    \"Two.\"\n    return\n",
    );
    let after = compile(
        "edited",
        "label start:\n    \"Zero.\"\n    \"One.\"\n    \"Two.\"\n    return\n",
    );

    let mut session = vela_vm::Session::start(&before, "start").expect("start");
    session.advance();
    let snapshot = session.snapshot();
    assert!(
        snapshot.vm.frames[0].resume.is_some(),
        "the suspension is anchored"
    );

    let error = match vela_vm::Session::restore(&after, &snapshot) {
        Ok(_) => panic!("a save for a changed story restored"),
        Err(error) => error,
    };
    assert!(
        matches!(error, vela_vm::Fault::StaleFrame { ref body } if body == "label:start"),
        "got {error:?}"
    );
}

/// A save written before suspensions were anchored restores from its index, as it always did.
///
/// The anchor is additive, so a file that predates it still loads; what it cannot do is notice
/// a body that moved. `tests/golden/saves/` holds two such files on purpose.
#[test]
fn a_save_without_an_anchor_restores_from_its_index() {
    let module = compile("branch", MENU_STORY);
    let mut session = vela_vm::Session::start(&module, "start").expect("start");
    session.advance();
    let mut snapshot = session.snapshot();
    for frame in &mut snapshot.vm.frames {
        frame.resume = None;
    }

    let restored = vela_vm::Session::restore(&module, &snapshot).expect("restore");
    assert_eq!(restored.current(), session.current());
}

/// ... but one whose body no longer reaches that far is refused rather than run off the end.
#[test]
fn a_save_that_outruns_its_body_is_refused() {
    let module = compile("branch", MENU_STORY);
    let mut session = vela_vm::Session::start(&module, "start").expect("start");
    session.advance();
    let mut snapshot = session.snapshot();
    for frame in &mut snapshot.vm.frames {
        frame.resume = None;
    }
    snapshot.vm.frames[0].ip = 10_000;

    let error = match vela_vm::Session::restore(&module, &snapshot) {
        Ok(_) => panic!("a save past the end of its body restored"),
        Err(error) => error,
    };
    assert!(
        matches!(error, vela_vm::Fault::StaleFrame { .. }),
        "got {error:?}"
    );
}

/// A frame naming a body the module does not have is refused, not resumed into nothing.
#[test]
fn a_frame_that_names_nothing_is_refused() {
    let module = compile("nested", NESTED);
    let mut session = vela_vm::Session::start(&module, "start").expect("start");
    session.advance();
    let mut snapshot = session.snapshot();
    snapshot.vm.frames[0].body = "label:vanished".to_string();

    let error = match vela_vm::Session::restore(&module, &snapshot) {
        Ok(_) => panic!("a snapshot with a missing body restored"),
        Err(error) => error,
    };
    assert!(
        matches!(error, vela_vm::Fault::NoLabel(ref name) if name == "label:vanished"),
        "got {error:?}"
    );
}

/// A session hands the caller the suspension instead of answering it, which is what makes a
/// player-authored answer possible.
#[test]
fn a_session_suspends_and_resumes_under_the_callers_control() {
    let source = "label start:\n    \"A line.\"\n    return\n";
    let module = compile("hello", source);
    let mut session = vela_vm::Session::start(&module, "start").expect("start");

    let first = session.advance();
    let vela_vm::Step::Yield(command) = first else {
        panic!("expected a suspension, got {first:?}");
    };
    assert!(matches!(*command, vela_world::Command::Say { .. }));

    // The log is what the player did, and it is recorded as they do it.
    let step = session.answer(vela_world::Input::Ack);
    assert!(matches!(step, vela_vm::Step::Halt), "got {step:?}");
    assert_eq!(session.log(), &[vela_world::Input::Ack]);
    assert!(session.is_finished());
}
