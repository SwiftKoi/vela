//! Driving a VM to completion, and doing it again from a recording.
//!
//! The host answers each suspension, and every answer is recorded. That recording is what
//! makes a run reproducible: `replay` feeds the same answers back and must reach the same
//! state and the same command stream.
//!
//! This is the determinism contract in one file (`RUNTIME.md §4.1`): *given an identical
//! build and an identical input log, the runtime must reach an identical outcome.* Nothing
//! else is allowed to be a source of variation, which is why `ARCHITECTURE.md §4` bans hash
//! iteration, direct clock reads, and unrecorded host calls.

use vela_bytecode::Module;
use vela_world::{Command, Input, World};

use crate::fault::Fault;
use crate::machine::{Step, Vm};

/// What a host does when the story suspends.
pub trait Host {
    /// Answers a command.
    fn answer(&mut self, command: &Command) -> Input;
}

/// One run, and everything needed to repeat it.
#[derive(Clone, PartialEq, Debug)]
pub struct Execution {
    /// Every command, in order.
    pub commands: Vec<Command>,
    /// Every answer that was given, in order. Feeding this back reproduces the run.
    pub log: Vec<Input>,
    /// The state it left behind.
    pub world: World,
}

/// Runs a story to completion, recording what it was told.
///
/// # Errors
///
/// Fails on a VM fault, which is always a compiler bug — the verifier exists so that a
/// module which reaches here cannot produce one.
pub fn run(module: &Module, start: &str, host: &mut dyn Host) -> Result<Execution, Fault> {
    drive(module, start, &mut |command| Ok(host.answer(command)))
}

/// Repeats a recording.
///
/// # Errors
///
/// Fails if the story faults, or if it asks for more answers than the log holds. The second
/// is not a fault but a *difference*: a run that needs an answer the recording does not have
/// is not the run that was recorded, and saying so beats acknowledging silently and
/// reporting a mismatch two hundred commands later.
pub fn replay(module: &Module, start: &str, log: &[Input]) -> Result<Execution, Fault> {
    let mut cursor = 0usize;
    drive(module, start, &mut |_| {
        let answer = log
            .get(cursor)
            .cloned()
            .ok_or(Fault::LogExhausted(cursor))?;
        cursor += 1;
        Ok(answer)
    })
}

/// The step loop, shared by both.
fn drive(
    module: &Module,
    start: &str,
    answer: &mut dyn FnMut(&Command) -> Result<Input, Fault>,
) -> Result<Execution, Fault> {
    let mut vm = Vm::new(module.clone());
    vm.start(start)?;

    let mut world = World::new();
    let mut commands = Vec::new();
    let mut log = Vec::new();

    // The answer for the suspension that is waiting, if one is. `resume` returns the *next*
    // step, which is usually another `Yield` — discarding it drops a command and moves the
    // story on without it, which is exactly what a caller that looked right did the first
    // time this was written.
    let mut pending: Option<Input> = None;

    loop {
        let step = match pending.take() {
            Some(input) => {
                let value = input.resolve();
                log.push(input);
                vm.resume(&mut world, value)
            }
            None => vm.run(&mut world),
        };

        match step {
            Step::Yield(command) => {
                pending = Some(answer(&command)?);
                commands.push(*command);
            }
            Step::Continue => {}
            Step::Halt => break,
            Step::Fault(fault) => return Err(*fault),
        }
    }

    Ok(Execution {
        commands,
        log,
        world,
    })
}

/// A host that always takes the first choice, for a run nobody is watching.
pub struct TakeFirst;

impl Host for TakeFirst {
    fn answer(&mut self, command: &Command) -> Input {
        match command {
            // A menu's first entry is index 0, whatever it is called.
            Command::Menu { .. } => Input::Choice(0),
            _ => Input::Ack,
        }
    }
}

/// A host that answers from a list, for a scripted run.
pub struct Scripted {
    answers: Vec<Input>,
    cursor: usize,
}

impl Scripted {
    /// A host with a script.
    #[must_use]
    pub fn new(answers: Vec<Input>) -> Self {
        Self { answers, cursor: 0 }
    }
}

impl Host for Scripted {
    fn answer(&mut self, _command: &Command) -> Input {
        let answer = self.answers.get(self.cursor).cloned().unwrap_or(Input::Ack);
        self.cursor += 1;
        answer
    }
}
