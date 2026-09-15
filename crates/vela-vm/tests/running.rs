//! Running a story headless.
//!
//! `M05-vm.md`'s first exit criterion is that `examples/hello` runs to a halt and prints its
//! commands. This is that, plus the two properties the milestone turns on: the command
//! stream is the *only* contact with the outside world, and a run is reproducible.
//!
//! Snapshotting and restoring a machine is `snapshots.rs`, next door.

mod common;

use std::path::{Path, PathBuf};

use common::compile;
use vela_bytecode::Module;
use vela_vm::{Host, TakeFirst, run};

/// Where the shipped examples live.
fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// Compiles one of the shipped examples.
fn example(name: &str, source: &str) -> Module {
    let path = examples().join(name).join("src").join(source);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    compile("main", &text)
}

#[test]
fn hello_runs_headless_to_a_halt() {
    let module = example("hello", "main.vela");
    let mut host = TakeFirst;
    let execution = run(&module, "start", &mut host).expect("hello faulted");

    let commands: Vec<String> = execution.commands.iter().map(ToString::to_string).collect();
    assert_eq!(
        commands,
        vec!["say \"Hello, world.\"".to_string()],
        "the command stream is not what the story says"
    );
}

#[test]
fn a_menu_offers_its_choices_and_takes_the_answer() {
    let module = compile(
        "menu",
        "label start:\n\
         \x20   menu \"What now?\":\n\
         \x20       \"Walk\":\n\
         \x20           jump walking\n\
         \x20       \"Wait\":\n\
         \x20           jump waiting\n\
         \x20   return\n\n\
         label walking:\n\
         \x20   \"You walk.\"\n\
         \x20   return\n\n\
         label waiting:\n\
         \x20   \"You wait.\"\n\
         \x20   return\n",
    );

    let mut host = TakeFirst;
    let execution = run(&module, "start", &mut host).expect("the menu faulted");
    let commands: Vec<String> = execution.commands.iter().map(ToString::to_string).collect();

    assert!(commands[0].starts_with("menu"), "{commands:?}");
    assert!(commands[0].contains("[0] Walk"), "{commands:?}");
    assert!(commands[0].contains("[1] Wait"), "{commands:?}");
    assert_eq!(
        commands[1], "say \"You walk.\"",
        "the first choice was not taken"
    );
}

#[test]
fn a_call_returns_to_where_it_was_made() {
    let module = compile(
        "call",
        "label start:\n\
         \x20   \"Before.\"\n\
         \x20   call middle\n\
         \x20   \"After.\"\n\
         \x20   return\n\n\
         label middle:\n\
         \x20   \"Middle.\"\n\
         \x20   return\n",
    );

    let mut host = TakeFirst;
    let execution = run(&module, "start", &mut host).expect("the call faulted");
    let commands: Vec<String> = execution.commands.iter().map(ToString::to_string).collect();

    assert_eq!(
        commands,
        vec![
            "say \"Before.\"".to_string(),
            "say \"Middle.\"".to_string(),
            "say \"After.\"".to_string()
        ]
    );
}

/// Which choice a host takes is the *only* thing that differs between two runs.
#[test]
fn the_host_decides_the_branch() {
    struct Second;
    impl Host for Second {
        fn answer(&mut self, command: &vela_world::Command) -> vela_world::Input {
            match command {
                vela_world::Command::Menu { .. } => vela_world::Input::Choice(1),
                _ => vela_world::Input::Ack,
            }
        }
    }

    let module = compile(
        "menu",
        "label start:\n\
         \x20   menu:\n\
         \x20       \"First\":\n\
         \x20           \"One.\"\n\
         \x20       \"Second\":\n\
         \x20           \"Two.\"\n\
         \x20   return\n",
    );

    let first = run(&module, "start", &mut TakeFirst).expect("faulted");
    let second = run(&module, "start", &mut Second).expect("faulted");

    let describe = |execution: &vela_vm::Execution| -> Vec<String> {
        execution.commands.iter().map(ToString::to_string).collect()
    };

    assert_eq!(describe(&first)[1], "say \"One.\"");
    assert_eq!(describe(&second)[1], "say \"Two.\"");
}

/// A multi-branch fixture: three choices, each with its own path through the story.
const BRANCHING: &str = "\
default trust: int = 0

label start:
    \"You arrive.\"
    menu \"What now?\":
        \"Trust\":
            trust = 1
            jump trusting
        \"Leave\":
            trust = 0
            jump leaving
        \"Wait\":
            trust = 0
            jump waiting
    return

label trusting:
    \"You stay.\"
    return

label leaving:
    \"You go.\"
    return

label waiting:
    \"You wait.\"
    return
";

#[test]
fn a_run_replays_to_the_same_state_and_commands() {
    let module = compile("branch", BRANCHING);

    // One answer for the say, one for the menu.
    let mut host =
        vela_vm::Scripted::new(vec![vela_world::Input::Ack, vela_world::Input::Choice(1)]);
    let original = run(&module, "start", &mut host).expect("the run faulted");
    let repeated = vela_vm::replay(&module, "start", &original.log).expect("the replay faulted");

    assert_eq!(
        original.commands, repeated.commands,
        "the command stream differed on replay"
    );
    assert_eq!(
        original.world, repeated.world,
        "the world differed on replay"
    );
    assert_eq!(
        original.log, repeated.log,
        "the recording differed from itself"
    );
}

/// Which choice is made is the *only* thing that differs — so the recording is
/// load-bearing, not decoration.
///
/// The script is two answers rather than three: `Scripted` acknowledges anything it was not
/// told about, so a test says which *decision* it is making and lets the dialogue advance by
/// itself. Counting suspensions by hand is how this test failed three times while saying
/// nothing about the engine.
#[test]
fn a_different_recording_reaches_a_different_branch() {
    let module = compile("branch", BRANCHING);

    let run_with = |choice: usize| -> vela_vm::Execution {
        let mut host = vela_vm::Scripted::new(vec![
            vela_world::Input::Ack,
            vela_world::Input::Choice(choice),
        ]);
        run(&module, "start", &mut host).expect("the run faulted")
    };

    let describe = |execution: &vela_vm::Execution| -> Vec<String> {
        execution.commands.iter().map(ToString::to_string).collect()
    };

    assert_eq!(describe(&run_with(0)).last().unwrap(), "say \"You stay.\"");
    assert_eq!(describe(&run_with(2)).last().unwrap(), "say \"You wait.\"");

    // And each run replays to itself, which is the property the recording is for.
    for choice in [0, 2] {
        let original = run_with(choice);
        let repeated =
            vela_vm::replay(&module, "start", &original.log).expect("the replay faulted");
        assert_eq!(original, repeated, "choice {choice} did not replay");
    }
}

/// A recording that runs out is not the run that was recorded, and says so rather than
/// quietly acknowledging.
#[test]
fn a_recording_that_ends_early_is_reported() {
    let module = compile("branch", BRANCHING);
    let error = vela_vm::replay(&module, "start", &[]).expect_err("an empty log was accepted");

    assert!(
        matches!(error, vela_vm::Fault::LogExhausted(_)),
        "expected a log exhaustion, got {error:?}"
    );
}

/// The `rand` path the replay criterion asks for: an effect drawn from `World`'s generator.
const ROLLING: &str = "\
effect rand.int(low: int, high: int) -> int
effect time.now() -> float

label start:
    var roll = rand.int(1, 6)
    if roll > 3:
        jump high
    jump low

label high:
    \"High.\"
    return

label low:
    \"Low.\"
    return
";

#[test]
fn an_effect_is_drawn_from_the_world() {
    let module = compile("roll", ROLLING);
    let mut host = vela_vm::TakeFirst;
    let execution = run(&module, "start", &mut host).expect("the roll faulted");

    let last = execution.commands.last().expect("no command");
    let text = last.to_string();
    assert!(
        text == "say \"High.\"" || text == "say \"Low.\"",
        "the roll produced {text}"
    );
}

/// Replay has to reproduce the *draw*, not merely the branch: the generator lives in `World`,
/// so an identical log gives an identical number.
#[test]
fn a_rand_path_replays_identically() {
    let module = compile("roll", ROLLING);
    let mut host = vela_vm::TakeFirst;
    let original = run(&module, "start", &mut host).expect("the roll faulted");

    let repeated = vela_vm::replay(&module, "start", &original.log).expect("the replay faulted");

    assert_eq!(original.commands, repeated.commands);
    assert_eq!(original.world, repeated.world);
}

/// A loop whose body presents a command verifies and runs.
///
/// It did neither: `concat` was declared as a count-carrying instruction while the compiler wrote
/// no count and the machine assumed two, so the verifier counted a value left on the stack and the
/// loop's back edge arrived at a different depth than its entry (`E6001`). Nothing in the corpus
/// had a command inside a loop, so nothing caught it.
#[test]
fn a_loop_that_presents_a_command_runs() {
    let module = compile(
        "loop",
        "label start:\n\
         \x20   var i = 0\n\
         \x20   while i < 2:\n\
         \x20       i += 1\n\
         \x20       \"i is [i].\"\n\
         \x20   return\n",
    );

    let execution = run(&module, "start", &mut TakeFirst).expect("the loop faulted");
    let commands: Vec<String> = execution.commands.iter().map(ToString::to_string).collect();
    assert_eq!(commands, vec!["say \"i is 1.\"", "say \"i is 2.\""]);
}

/// A `default` is worth what it was declared with, before anything writes it.
///
/// Reading one used to hand back `none` for any slot the world had not been told about, so
/// `trust + 1` was `add.i` given `none` — on the first line that touched it.
#[test]
fn a_default_reads_as_its_declaration() {
    let module = compile(
        "defaults",
        "default trust: int = 2\n\nlabel start:\n    \"trust is [trust].\"\n    return\n",
    );

    let execution = run(&module, "start", &mut TakeFirst).expect("the default faulted");
    let commands: Vec<String> = execution.commands.iter().map(ToString::to_string).collect();
    assert_eq!(commands, vec!["say \"trust is 2.\""]);
    // And the world a fresh run starts from *holds* it, which is what `RUNTIME.md §2` says the
    // world's defaults are.
    assert_eq!(
        execution.world.get("trust"),
        Some(&vela_world::Value::Int(2)),
        "the declared default was not seeded into the world"
    );
}

/// A function sees the arguments it was called with.
///
/// The callee's frame base was `stack.len()` — the *top* of the stack — so every parameter read
/// found nothing and answered `none`: `n == 1` was false for every `n`, and the function took the
/// wrong branch rather than failing.
#[test]
fn a_function_reads_its_arguments() {
    let module = compile(
        "arguments",
        "fn word(n: int) -> str:\n\
         \x20   if n == 1:\n\
         \x20       return \"one\"\n\
         \x20   return \"many\"\n\n\
         label start:\n\
         \x20   \"n=1 is [word(1)].\"\n\
         \x20   \"n=2 is [word(2)].\"\n\
         \x20   return\n",
    );

    let execution = run(&module, "start", &mut TakeFirst).expect("the call faulted");
    let commands: Vec<String> = execution.commands.iter().map(ToString::to_string).collect();
    assert_eq!(
        commands,
        vec!["say \"n=1 is one.\"", "say \"n=2 is many.\""]
    );
}

/// A variant with no payload, used as a value, is the variant it names.
///
/// The constant form emitted `(variant, 0)` while `EnumNew` reads `(enum, variant)` everywhere
/// else, so `Ending.cold` built whichever enum sat at index `cold` — a fault, or a value of the
/// wrong type.
#[test]
fn a_constant_variant_is_the_variant_it_names() {
    let module = compile(
        "variants",
        "enum Ending:\n\
         \x20   warm\n\
         \x20   cold\n\n\
         label start:\n\
         \x20   var ending: Ending = Ending.cold\n\
         \x20   match ending:\n\
         \x20       when Ending.warm:\n\
         \x20           \"warm\"\n\
         \x20       when Ending.cold:\n\
         \x20           \"cold\"\n\
         \x20   return\n",
    );

    let execution = run(&module, "start", &mut TakeFirst).expect("the variant faulted");
    let commands: Vec<String> = execution.commands.iter().map(ToString::to_string).collect();
    assert_eq!(commands, vec!["say \"cold\""]);
}

/// A capability the host does not provide is refused rather than answered wrongly.
#[test]
fn an_unimplemented_effect_is_refused() {
    let module = compile(
        "unhosted",
        "effect fs.read(path: str) -> str\n\n\
         label start:\n    var text = fs.read(\"save\")\n    \"After.\"\n    return\n",
    );

    let mut host = vela_vm::TakeFirst;
    let fault = run(&module, "start", &mut host).expect_err("an unhosted effect ran");

    assert!(
        matches!(fault, vela_vm::Fault::CapabilityDenied(ref name) if name == "fs.read"),
        "expected a capability denial, got {fault:?}"
    );
}
