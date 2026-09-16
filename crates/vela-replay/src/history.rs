//! Rollback: snapshots over command boundaries, and replay from the nearest one.
//!
//! `RUNTIME.md §7.1`: *"rollback is snapshot-and-replay, not undo-log inversion."* A world is
//! plain data, so copying it is cheap and obviously correct; inverting every operation is
//! neither. A snapshot is taken every few commands, and a rollback restores the nearest one at
//! or before the target and replays the answers in between.
//!
//! ```text
//! position:  0      1      2      3      4  ...  j
//! commands:         #1     #2     #3     #4       #j   <- a command per position
//! snapshots: S0            S2            S4            <- every `every` positions
//! ```
//!
//! **Rewind-and-rebranch is the same mechanism.** Rolling back discards the answers after the
//! target; the next answer is a *new* one, which is how "take the other branch" and "roll
//! back" are one thing rather than two.

use std::collections::VecDeque;

use vela_bytecode::Module;
use vela_vm::{DebugLocal, Fault, FrameInfo, Session, Site, Snapshot, Step};
use vela_world::{Command, Input, World};

/// How often a snapshot is taken, per `RUNTIME.md §7.2`.
pub const DEFAULT_INTERVAL: u64 = 64;

/// How many snapshots to keep.
///
/// `RUNTIME.md §7.2` calls for a ring: rollback history is bounded, and a player who rolls back
/// further than it reaches simply stops at the oldest point rather than faulting. At the
/// default interval this is roughly 2,000 commands of history.
pub const DEFAULT_DEPTH: usize = 32;

/// A session with a rollback history.
pub struct Timeline {
    module: Module,
    session: Session,
    /// How many commands between snapshots.
    every: u64,
    /// The most recent snapshots, oldest first, each with the position it was taken at.
    snapshots: VecDeque<(u64, Snapshot)>,
    /// The answers given, `answers[i]` answering command `i + 1`.
    answers: Vec<Input>,
    /// How many commands have been produced.
    position: u64,
    /// How many snapshots to keep.
    depth: usize,
}

impl Timeline {
    /// Starts a story, taking a snapshot of the state before its first command.
    ///
    /// # Errors
    ///
    /// Fails if the label does not exist.
    pub fn start(module: &Module, label: &str) -> Result<Self, Fault> {
        Self::with_interval(module, label, DEFAULT_INTERVAL, DEFAULT_DEPTH)
    }

    /// Starts a story with a chosen snapshot interval and ring depth.
    ///
    /// # Errors
    ///
    /// Fails if the label does not exist.
    pub fn with_interval(
        module: &Module,
        label: &str,
        every: u64,
        depth: usize,
    ) -> Result<Self, Fault> {
        let session = Session::start(module, label)?;
        let first = session.snapshot();
        let mut snapshots = VecDeque::with_capacity(depth.max(1));
        snapshots.push_back((0, first));
        Ok(Self {
            module: module.clone(),
            session,
            every: every.max(1),
            snapshots,
            answers: Vec::new(),
            position: 0,
            depth: depth.max(1),
        })
    }

    /// Resumes a timeline from a snapshot, with an empty history.
    ///
    /// This is what a load does: the state is restored and the command the snapshot was
    /// waiting on is presented again, but the rollback history starts empty (`RUNTIME.md
    /// §7.3`) and refills as the player continues.
    ///
    /// # Errors
    ///
    /// Fails if the snapshot's frames name bodies this module does not have.
    pub fn resume(module: &Module, snapshot: &Snapshot) -> Result<Self, Fault> {
        Self::resume_with(module, snapshot, DEFAULT_INTERVAL, DEFAULT_DEPTH)
    }

    /// Resumes a timeline with a chosen interval and ring depth.
    ///
    /// # Errors
    ///
    /// Fails if the snapshot's frames name bodies this module does not have.
    pub fn resume_with(
        module: &Module,
        snapshot: &Snapshot,
        every: u64,
        depth: usize,
    ) -> Result<Self, Fault> {
        let session = Session::restore(module, snapshot)?;
        let mut snapshots = VecDeque::with_capacity(depth.max(1));
        snapshots.push_back((0, session.snapshot()));
        Ok(Self {
            module: module.clone(),
            session,
            every: every.max(1),
            snapshots,
            answers: Vec::new(),
            position: 0,
            depth: depth.max(1),
        })
    }

    /// A snapshot of the current state, for a save.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        self.session.snapshot()
    }

    /// Whether the story has ended.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.session.is_finished()
    }

    /// Runs to the next command, or to the end.
    pub fn advance(&mut self) -> Step {
        let step = self.session.advance();
        self.record(&step);
        step
    }

    /// Answers the command on screen and runs to the next one.
    pub fn answer(&mut self, input: Input) -> Step {
        self.answers.push(input.clone());
        let step = self.session.answer(input);
        self.record(&step);
        step
    }

    /// Executes exactly one instruction, for a debugger stopping between instructions.
    ///
    /// The command count only moves when a `Yield` does (`record`), so stepping through a single
    /// command leaves the rollback positions where they were rather than inventing one per
    /// instruction.
    pub fn step(&mut self) -> Step {
        let step = self.session.step();
        self.record(&step);
        step
    }

    /// Answers the command on screen and executes exactly one instruction.
    pub fn resume_step(&mut self, input: Input) -> Step {
        self.answers.push(input.clone());
        let step = self.session.resume_step(input);
        self.record(&step);
        step
    }

    /// Where the machine is, for a debugger.
    #[must_use]
    pub fn site(&self) -> Option<Site> {
        self.session.site()
    }

    /// The call stack, outermost frame first.
    #[must_use]
    pub fn call_stack(&self) -> Vec<FrameInfo> {
        self.session.call_stack()
    }

    /// The running frame's slots that hold a value, by name.
    #[must_use]
    pub fn locals(&self) -> Vec<DebugLocal> {
        self.session.locals()
    }

    /// One frame's slots that hold a value, by name, zero being the outermost.
    #[must_use]
    pub fn locals_at(&self, frame: usize) -> Vec<DebugLocal> {
        self.session.locals_at(frame)
    }

    /// Rolls back to `target`, replaying from the nearest snapshot.
    ///
    /// The answers after `target` are discarded, which is the rewind half of rewind-and-
    /// rebranch. A target beyond the current position, or before the oldest snapshot the ring
    /// still holds, is clamped — a rollback wheel stops at the end of the history rather than
    /// faulting.
    ///
    /// Returns the position actually reached.
    pub fn rollback(&mut self, target: u64) -> u64 {
        let target = target.min(self.position);
        let Some(start) = self
            .snapshots
            .iter()
            .rev()
            .map(|(position, _)| *position)
            .find(|position| *position <= target)
        else {
            return self.position;
        };
        let snapshot = self
            .snapshots
            .iter()
            .find(|(position, _)| *position == start)
            .map(|(_, snapshot)| snapshot.clone())
            .expect("the position was just found");

        // A snapshot of this module always restores: the frames name bodies that are in it.
        let mut session =
            Session::restore(&self.module, &snapshot).expect("a snapshot of this module");
        if start == 0 && target >= 1 {
            // Position 0 is before the first command; every later position is `target - 1`
            // answers past it.
            session.advance();
            for command in 1..target {
                session.answer(self.answers[(command - 1) as usize].clone());
            }
        } else if start >= 1 {
            for command in start..target {
                session.answer(self.answers[(command - 1) as usize].clone());
            }
        }

        self.session = session;
        self.position = target;
        self.answers.truncate(target.saturating_sub(1) as usize);
        self.snapshots.retain(|(position, _)| *position <= target);
        target
    }

    /// Rewinds to `target` and takes a different answer.
    pub fn rebranch(&mut self, target: u64, input: Input) -> Step {
        self.rollback(target);
        self.answer(input)
    }

    /// Takes a snapshot if this position is due one.
    fn record(&mut self, step: &Step) {
        if matches!(step, Step::Yield(_)) {
            self.position += 1;
        }
        if self.position % self.every == 0
            && self.snapshots.back().map(|(position, _)| *position) != Some(self.position)
        {
            self.snapshots
                .push_back((self.position, self.session.snapshot()));
            while self.snapshots.len() > self.depth {
                self.snapshots.pop_front();
            }
        }
    }

    /// How many commands have been produced.
    #[must_use]
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Whether there is anywhere to roll back to.
    #[must_use]
    pub fn can_rollback(&self) -> bool {
        self.position > 0
    }

    /// The command on screen, if there is one.
    #[must_use]
    pub fn current(&self) -> Option<&Command> {
        self.session.current()
    }

    /// The state the story has left behind.
    #[must_use]
    pub fn world(&self) -> &World {
        self.session.world()
    }

    /// The positions the ring currently holds a snapshot at, oldest first.
    #[must_use]
    pub fn snapshot_positions(&self) -> Vec<u64> {
        self.snapshots
            .iter()
            .map(|(position, _)| *position)
            .collect()
    }

    /// How many answers are still held (the ones after the current position are discarded).
    #[must_use]
    pub fn answers(&self) -> usize {
        self.answers.len()
    }
}
