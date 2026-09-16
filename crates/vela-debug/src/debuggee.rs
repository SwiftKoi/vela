//! A story under the debugger: the machine, the rollback ring behind it, and the policy that
//! decides where to stop.
//!
//! # Why the ring is part of the story
//!
//! `RUNTIME.md §7` makes rollback snapshot-and-replay, and `vela-replay::Timeline` already keeps
//! the ring — so stepping *backwards* is not machinery this file adds, it is a rollback to an
//! earlier command. The only choice here is the interval: a debugger wants to step back one
//! command at a time, so it snapshots **every** command rather than every sixty-fourth, and the
//! ring is deep enough to cover a session's worth of stepping.
//!
//! # Why the stop policy is here and not in the machine
//!
//! `RUNTIME.md §9` keeps `vela-vm` a pull interface: it executes one instruction and answers where
//! it is. Deciding *whether that instruction is where you wanted to stop* is policy, and policy
//! belongs to the debugger — a breakpoint is a comparison, not a machine feature.

use vela_bytecode::Module;
use vela_replay::Timeline;
use vela_span::Span;
use vela_vm::{Fault, Host, Site, Step, TakeFirst};
use vela_world::Input;

use crate::breakpoints::Breakpoints;
use crate::program::Program;

/// How many snapshots the ring keeps: enough to step back through a long session.
const HISTORY: usize = 512;

/// How many instructions a single resume may run before it is a story that does not end.
const RUNAWAY: usize = 2_000_000;

/// What a resume asked the machine to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Run to the next breakpoint.
    Continue,
    /// Stop at the next statement in this frame or an outer one, skipping calls.
    Over,
    /// Stop at the next statement, wherever it is.
    Into,
    /// Stop when this frame returns.
    Out,
}

/// Why the machine stopped.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum StopReason {
    /// A breakpoint was reached.
    Breakpoint,
    /// A step finished.
    Step,
    /// The story stopped on entry, before running.
    Entry,
    /// The client asked to pause.
    Pause,
    /// A step backwards landed.
    Back,
    /// The story ended.
    Terminated,
    /// The machine faulted, which is always a compiler bug (`BYTECODE.md §4`).
    Fault(String),
    /// A resume ran too long without stopping — a story that does not end.
    Runaway,
}

/// Where a resume started from, which is what a step is measured against.
struct Origin {
    depth: usize,
    body: String,
    ip: usize,
    span: Option<Span>,
}

/// A story, its rollback ring, and the last step it was asked for.
pub struct Debuggee {
    timeline: Timeline,
    mode: Mode,
    origin: Option<Origin>,
}

impl Debuggee {
    /// Starts a story at its entry label, stopped on entry.
    ///
    /// # Errors
    ///
    /// Fails if the entry label is not in the module.
    pub fn new(module: &Module, entry: &str) -> Result<Self, Fault> {
        // Interval 1: a snapshot per command, so `step back` is a restore rather than a replay.
        let timeline = Timeline::with_interval(module, entry, 1, HISTORY)?;
        Ok(Self {
            timeline,
            mode: Mode::Continue,
            origin: None,
        })
    }

    /// The rollback ring, for reading position, world, and snapshot history.
    #[must_use]
    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }

    /// Where the machine is, if it is anywhere.
    #[must_use]
    pub fn site(&self) -> Option<Site> {
        self.timeline.site()
    }

    /// Runs until something says stop.
    ///
    /// `mode` is what this resume is for; breakpoints stop it whatever the mode, which is what
    /// makes "continue" and "step" the same loop with a different reason to stop.
    pub fn resume(
        &mut self,
        program: &Program,
        breakpoints: &Breakpoints,
        mode: Mode,
    ) -> StopReason {
        self.origin = self.site().map(|site| Origin {
            depth: site.depth,
            body: site.body,
            ip: site.ip,
            span: site.span,
        });
        self.mode = mode;
        self.run(program, breakpoints)
    }

    /// Steps back `commands` commands, restoring the world exactly as the forward run left it.
    ///
    /// The count is in commands rather than instructions because that is the granularity the ring
    /// has: `RUNTIME.md §7` snapshots at command boundaries, so this is "go back N lines of
    /// dialogue" rather than "N instructions", which is also what a person means by it.
    pub fn step_back(&mut self, commands: u64) -> StopReason {
        let target = self.timeline.position().saturating_sub(commands);
        self.timeline.rollback(target);
        self.origin = None;
        self.mode = Mode::Continue;
        StopReason::Back
    }

    /// The step loop.
    fn run(&mut self, program: &Program, breakpoints: &Breakpoints) -> StopReason {
        // A debugger has no player, so a command that needs an answer gets the one a headless run
        // would give: advance, and take the first branch at a menu (`vela run`'s `TakeFirst`).
        // Asking the client to answer a menu is a feature for later, and inventing a second answer
        // now would make a debugged run differ from the run it is standing in for.
        let mut host = TakeFirst;
        let mut pending: Option<Input> = None;

        for _ in 0..RUNAWAY {
            if let Some(site) = self.site()
                && let Some(reason) = self.stop_at(program, breakpoints, &site)
            {
                self.origin = None;
                self.mode = Mode::Continue;
                return reason;
            }

            let step = match pending.take() {
                Some(input) => self.timeline.resume_step(input),
                None => self.timeline.step(),
            };
            match step {
                Step::Yield(command) => pending = Some(host.answer(&command)),
                Step::Halt => return StopReason::Terminated,
                Step::Fault(fault) => return StopReason::Fault(fault.to_string()),
                Step::Continue => {}
            }
        }

        StopReason::Runaway
    }

    /// Whether this site is where the resume should stop.
    fn stop_at(
        &self,
        program: &Program,
        breakpoints: &Breakpoints,
        site: &Site,
    ) -> Option<StopReason> {
        // A breakpoint stops a run whatever the mode, which is what makes `continue` and `step`
        // one loop: the difference is what else can stop it.
        if let Some(span) = site.span
            && let Some(position) = program.position_of(span)
            && breakpoints.hits_line(position.file, position.line)
        {
            return Some(StopReason::Breakpoint);
        }
        if site.label && breakpoints.hits_label(&site.body) {
            return Some(StopReason::Breakpoint);
        }

        let origin = self.origin.as_ref()?;

        // "Moved" is measured in source statements when there are spans to measure, and in
        // instructions when there are not. A release module has no spans, so an instruction is the
        // finest step it can offer — and comparing spans there would compare `None == None`, which
        // is "never moved" and a step that never stops.
        let moved = match (origin.span, site.span) {
            (Some(before), Some(after)) => before != after || origin.body != site.body,
            _ => origin.ip != site.ip || origin.body != site.body,
        };

        match self.mode {
            Mode::Continue => None,
            Mode::Into => moved.then_some(StopReason::Step),
            // Stepping *over* a call means not stopping while deeper than we started.
            Mode::Over => (moved && site.depth <= origin.depth).then_some(StopReason::Step),
            Mode::Out => (site.depth < origin.depth).then_some(StopReason::Step),
        }
    }
}
