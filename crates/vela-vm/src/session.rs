//! Stepping a story under someone else's control.
//!
//! [`crate::run`] drives a story to completion, which is what a test or a headless run wants.
//! A *player* wants the opposite: run until the story asks for something, wait for a person,
//! answer, continue. The difference matters because the answer decides what happens next — a
//! menu chosen by a player is not a recording, and a run that auto-acknowledges and then
//! replays cannot represent one.
//!
//! The suspension model is already exactly this shape (`ARCHITECTURE.md §7`): the VM builds a
//! command and stops; the host presents it and answers. This type is the loop around that,
//! with the two halves handed to the caller instead of buried.

use serde::{Deserialize, Serialize};
use vela_bytecode::Module;
use vela_world::{Command, Input, Preferences, Value, World};

use crate::debug::{DebugLocal, FrameInfo, Site};
use crate::fault::Fault;
use crate::machine::{Step, Vm};
use crate::state::VmState;

/// A snapshot of a running story: the world and the machine that is running it.
///
/// The world alone is not enough to resume. The machine holds where execution *is* — the
/// call stack and the operand stack — and the command that was on screen when the snapshot
/// was taken, which the host presents again on load so it can be answered.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    /// The state the story has left behind.
    pub world: World,
    /// The machine.
    pub vm: VmState,
    /// The command awaiting an answer when the snapshot was taken.
    pub current: Option<Command>,
}

/// A story, part-way through.
pub struct Session {
    vm: Vm,
    world: World,
    log: Vec<Input>,
    /// The command the story is waiting on, so a snapshot can carry it and a load can present
    /// it again. The VM has already handed it over; someone has to remember it.
    current: Option<Command>,
    finished: bool,
    /// The label this session started at, when it started rather than restored. A presenter
    /// built over a loaded session needs it; a restored one does not know and does not need to.
    entry: Option<String>,
}

impl Session {
    /// Starts a story at `label`.
    ///
    /// # Errors
    ///
    /// Fails if the label does not exist, which is a caller's mistake rather than a fault —
    /// the compiler resolves labels, so reaching here with a bad one means the caller invented
    /// it.
    pub fn start(module: &Module, label: &str) -> Result<Self, Fault> {
        let mut vm = Vm::new(module.clone());
        vm.start(label)?;

        // The world a story declares for itself, before anything runs (`RUNTIME.md §2`).
        let mut world = World::new();
        vm.seed(&mut world)?;

        Ok(Self {
            vm,
            world,
            log: Vec::new(),
            current: None,
            finished: false,
            entry: Some(label.to_string()),
        })
    }

    /// Loads a story from a built bundle or a compiled module.
    ///
    /// This is the startup path `ARCHITECTURE.md §8` describes — *"bytecode loads without
    /// recompilation"* — and the API `RUNTIME.md §8` shows. Given a bundle directory it reads the
    /// entry point from the manifest and the module from `scripts/`; given a `.velac` it starts at
    /// the module's first label. No source is read and no compiler is involved, which is what
    /// makes a bundle something a player runs rather than an artifact nothing consumes.
    ///
    /// # Errors
    ///
    /// Fails if the path cannot be read, a directory is not a bundle, the bundle names no entry,
    /// the module does not decode, or the entry label is not in it.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, crate::LoadError> {
        let (module, label) = crate::load::read(path.as_ref())?;
        Self::start(&module, &label).map_err(crate::LoadError::Fault)
    }

    /// Resumes a story from a snapshot, with the settings of the player it is resumed for.
    ///
    /// The input log starts empty: `RUNTIME.md §7.3` — *"rollback history is not persisted"* —
    /// so a load is independent of how long the previous session ran. The command the snapshot
    /// was waiting on is restored, and is answered by the caller, not by `advance`.
    ///
    /// `preferences` are *taken* rather than read out of the snapshot, which carries none: a setting
    /// belongs to the player and not to the playthrough, so a resume that wants the player's own has
    /// to be told whose they are (`RUNTIME.md §2.1`).
    ///
    /// # Errors
    ///
    /// Fails if a frame names a body this module does not have.
    pub fn restore(
        module: &Module,
        snapshot: &Snapshot,
        preferences: Preferences,
    ) -> Result<Self, Fault> {
        let mut vm = Vm::new(module.clone());
        vm.restore_state(&snapshot.vm)?;
        Ok(Self {
            vm,
            world: {
                let mut world = snapshot.world.clone();
                world.preferences = preferences;
                world
            },
            log: Vec::new(),
            current: snapshot.current.clone(),
            finished: snapshot.vm.finished,
            entry: None,
        })
    }

    /// Puts *this* session back to a snapshot, with the settings of the player it is resumed for.
    ///
    /// The mirror of [`Session::restore`] for a caller that is already holding a session: a resume needs
    /// the module only to rebuild the machine's frames, and a live session's machine has one. What it
    /// does not keep is the run: the input log starts empty, because rollback history is not persisted
    /// (`RUNTIME.md §7.3`), and the command the snapshot was waiting on is restored for the caller to
    /// answer.
    ///
    /// # Errors
    ///
    /// Fails if a frame names a body this module does not have, which leaves the session as it was.
    pub fn resume(&mut self, snapshot: &Snapshot, preferences: Preferences) -> Result<(), Fault> {
        // The machine first, and only then the state around it: a frame that cannot be found is a
        // refusal rather than a half-resumed session.
        let mut vm = Vm::new(self.vm.module().clone());
        vm.restore_state(&snapshot.vm)?;
        self.vm = vm;
        self.world = snapshot.world.clone();
        self.world.preferences = preferences;
        self.log = Vec::new();
        self.current = snapshot.current.clone();
        self.finished = snapshot.vm.finished;
        Ok(())
    }

    /// Runs until the story suspends, ends, or faults.
    pub fn advance(&mut self) -> Step {
        if self.finished {
            return Step::Halt;
        }
        let step = self.vm.run(&mut self.world);
        self.record(&step);
        step
    }

    /// Answers the pending suspension and runs to the next one.
    ///
    /// The answer is recorded, so a session can be replayed later — the same log that makes a
    /// headless run reproducible makes a played one reproducible.
    pub fn answer(&mut self, input: Input) -> Step {
        let value = input.resolve();
        self.log.push(input);
        let step = self.vm.resume(&mut self.world, value);
        self.record(&step);
        step
    }

    /// Executes exactly one instruction.
    ///
    /// The debugger's step primitive: where [`Session::advance`] runs to the next command, this
    /// stops as soon as one instruction has run, so a caller can pause on a line, a label, or a
    /// single step and look around before deciding to run on.
    pub fn step(&mut self) -> Step {
        if self.finished {
            return Step::Halt;
        }
        let step = self.vm.step(&mut self.world);
        self.record(&step);
        step
    }

    /// Answers the pending suspension and executes exactly one instruction.
    ///
    /// The answer is recorded, exactly as [`Session::answer`] records it, so a debugged run is
    /// still a reproducible one.
    pub fn resume_step(&mut self, input: Input) -> Step {
        let value = input.resolve();
        self.log.push(input);
        let step = self.vm.resume_step(&mut self.world, value);
        self.record(&step);
        step
    }

    /// Where the machine is, for a debugger.
    #[must_use]
    pub fn site(&self) -> Option<Site> {
        self.vm.site()
    }

    /// The call stack, outermost frame first.
    #[must_use]
    pub fn call_stack(&self) -> Vec<FrameInfo> {
        self.vm.call_stack()
    }

    /// The running frame's slots that hold a value, by name.
    #[must_use]
    pub fn locals(&self) -> Vec<DebugLocal> {
        self.vm.locals()
    }

    /// One frame's slots that hold a value, by name, zero being the outermost.
    #[must_use]
    pub fn locals_at(&self, frame: usize) -> Vec<DebugLocal> {
        self.vm.locals_at(frame)
    }

    /// Notices what a step left behind.
    fn record(&mut self, step: &Step) {
        self.finished = matches!(step, Step::Halt);
        if let Step::Yield(command) = step {
            self.current = Some((**command).clone());
        }
    }

    /// The command the story is waiting on, if it is waiting.
    #[must_use]
    pub fn current(&self) -> Option<&Command> {
        self.current.as_ref()
    }

    /// A snapshot of the world and the machine, for a save or a rollback point.
    ///
    /// The world in it is the *story* state (`World::snapshot`): the player's settings are not carried,
    /// because a snapshot is restored into a session that already has a player's own (`§2.1`).
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            world: self.world.snapshot(),
            vm: self.vm.state(),
            current: self.current.clone(),
        }
    }

    /// Whether the story has ended.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// The state the story has left behind.
    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The player's settings, to write one.
    ///
    /// The write path a settings screen ends at, and a method rather than a public way to the whole
    /// world, because a setting is the only thing a *host* writes directly: everything else the story
    /// writes to is written by the story (`RUNTIME.md §2.1`).
    pub fn preferences_mut(&mut self) -> &mut Preferences {
        &mut self.world.preferences
    }

    /// Calls a function and returns what it produced, in the world as it is right now.
    ///
    /// The story is untouched: this is for asking a question *about* a suspended story rather than
    /// moving it, which is what a test's `expect` is. See [`Vm::call`] for the one thing it refuses to
    /// do, which is read a piece of story as if it were a value.
    ///
    /// # Errors
    ///
    /// Fails if there is no such function, or if it cannot be read as a value.
    pub fn call(&mut self, name: &str) -> Result<Value, Fault> {
        self.vm.call(&mut self.world, name)
    }

    /// The labels this session has entered, in the order it first entered them.
    #[must_use]
    pub fn entered_labels(&self) -> Vec<String> {
        self.vm.entered_labels()
    }

    /// The module it is running.
    ///
    /// Exposed for a host that builds its own presenter over a loaded session — the windowed
    /// player constructs a `Timeline` from the same module a bundle was loaded with, rather than
    /// recompiling one.
    #[must_use]
    pub fn module(&self) -> &Module {
        self.vm.module()
    }

    /// The label it started at, if it started rather than resumed from a snapshot.
    #[must_use]
    pub fn entry(&self) -> Option<&str> {
        self.entry.as_deref()
    }

    /// Every answer given so far.
    #[must_use]
    pub fn log(&self) -> &[Input] {
        &self.log
    }
}
