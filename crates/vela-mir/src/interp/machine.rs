//! The machine: the frame stack and the driver.
//!
//! Control flow lives here; reading and writing state lives in `exec`.

use super::exec::constant;
use vela_world::{Value as WorldValue, World};

use crate::ir::{BlockId, Body, Module, Place};

/// What the host does when the story suspends.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Answer {
    /// Acknowledge: advance the dialogue, finish the wait.
    Ack,
    /// Pick a menu entry, by its position in the offered list.
    Choice(usize),
}

/// How a run ended.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The story reached the end of its start label, or ran out of labels to return to.
    Halted,
    /// A fault: always a compiler bug, never a story the author could have written
    /// (`BYTECODE.md §4`).
    Fault(String),
}

/// One run of a module.
#[derive(Clone, PartialEq, Debug)]
pub struct Execution {
    /// Every command the story emitted, in order.
    pub commands: Vec<vela_world::Command>,
    /// The state it left behind.
    pub world: World,
    /// How it ended.
    pub outcome: Outcome,
}

impl Execution {
    /// Runs a module from a label.
    ///
    /// Answers are consumed in order; a run that needs more than it is given is
    /// acknowledged rather than faulted, so that a corpus entry only has to script the
    /// suspensions it cares about.
    #[must_use]
    pub fn run(module: &Module, start: &str, answers: &[Answer]) -> Self {
        let mut machine = Machine::new(module, answers);
        machine.enter_label(start);
        machine.drive();
        Self {
            commands: machine.commands,
            world: machine.world,
            outcome: machine.finished.unwrap_or(Outcome::Halted),
        }
    }
}

/// Which body a frame is running.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum BodyRef {
    /// A label, by index into `Module::labels`.
    Label(usize),
    /// A function, by index into `Module::fns`.
    Function(usize),
}

/// Where control goes when the frame finishes.
#[derive(Clone, Debug)]
pub(super) struct Continuation {
    pub(super) block: BlockId,
    pub(super) statement: usize,
    pub(super) destination: Option<Place>,
}

/// One frame.
#[derive(Clone, Debug)]
pub(super) struct Frame {
    pub(super) body: BodyRef,
    pub(super) block: BlockId,
    pub(super) statement: usize,
    pub(super) slots: Vec<WorldValue>,
    pub(super) ret: Option<Continuation>,
}

/// What executing one thing did.
pub(super) enum Step {
    /// Carry on in this block.
    Continue,
    /// A frame was pushed; the driver re-reads the stack.
    Entered,
}

/// A running machine.
pub(super) struct Machine<'a> {
    pub(super) module: &'a Module,
    pub(super) world: World,
    pub(super) commands: Vec<vela_world::Command>,
    pub(super) frames: Vec<Frame>,
    /// The arguments of the last `Cmd`, waiting for the `Yield` that hands them over.
    pub(super) pending: Vec<WorldValue>,
    pub(super) answers: &'a [Answer],
    pub(super) cursor: usize,
    pub(super) finished: Option<Outcome>,
}

impl<'a> Machine<'a> {
    /// A machine with its world initialised from the module's `default`s.
    pub(crate) fn new(module: &'a Module, answers: &'a [Answer]) -> Self {
        let mut world = World::new();
        for definition in &module.defaults {
            world.set(definition.name.clone(), constant(module, definition.init));
        }

        Self {
            module,
            world,
            commands: Vec::new(),
            frames: Vec::new(),
            pending: Vec::new(),
            answers,
            cursor: 0,
            finished: None,
        }
    }

    /// Starts at a label.
    pub(crate) fn enter_label(&mut self, name: &str) {
        let Some(index) = self
            .module
            .labels
            .iter()
            .position(|body| body.name.as_str() == name)
        else {
            self.finished = Some(Outcome::Fault(format!("no label `{name}`")));
            return;
        };
        let slots = self.fresh_slots(index);
        self.frames.push(Frame {
            body: BodyRef::Label(index),
            block: BlockId::ENTRY,
            statement: 0,
            slots,
            ret: None,
        });
    }

    /// The slot table a body starts with.
    pub(crate) fn fresh_slots(&self, index: usize) -> Vec<WorldValue> {
        let count = self
            .module
            .labels
            .get(index)
            .map_or(0, |body| body.locals.len());
        vec![WorldValue::None; count]
    }

    /// The slots a function starts with.
    pub(crate) fn fresh_function_slots(&self, index: usize) -> Vec<WorldValue> {
        let count = self
            .module
            .fns
            .get(index)
            .map_or(0, |body| body.locals.len());
        vec![WorldValue::None; count]
    }

    /// A body by reference.
    pub(crate) fn body(&self, reference: BodyRef) -> Option<&'a Body> {
        match reference {
            BodyRef::Label(index) => self.module.labels.get(index),
            BodyRef::Function(index) => self.module.fns.get(index),
        }
    }

    /// Runs until the story stops.
    pub(crate) fn drive(&mut self) {
        while self.finished.is_none() && !self.frames.is_empty() {
            if !self.step() {
                return;
            }
        }
    }

    /// Runs the current block to its terminator, or until a frame is pushed.
    pub(crate) fn step(&mut self) -> bool {
        loop {
            let Some(frame) = self.frames.last() else {
                return false;
            };
            let (reference, block_id, start) = (frame.body, frame.block, frame.statement);

            let Some(body) = self.body(reference) else {
                return self.fault(format!("body {reference:?} does not exist"));
            };
            let Some(block) = body.block(block_id) else {
                return self.fault(format!("block b{} does not exist", block_id.0));
            };

            let mut index = start;
            while index < block.stmts.len() {
                let stmt = &block.stmts[index];
                index += 1;
                if let Some(frame) = self.frames.last_mut() {
                    frame.statement = index;
                }
                match self.statement(&stmt.kind) {
                    Ok(Step::Continue) => {}
                    Ok(Step::Entered) => return true,
                    Err(fault) => return self.fault(fault),
                }
            }

            match self.terminator(&block.term) {
                Ok(Step::Continue) => {}
                Ok(Step::Entered) => return true,
                Err(fault) => return self.fault(fault),
            }
        }
    }

    /// Records a fault and stops.
    pub(crate) fn fault(&mut self, message: String) -> bool {
        self.finished = Some(Outcome::Fault(message));
        false
    }

    /// Pushes a frame for a function call.
    pub(crate) fn enter_function(
        &mut self,
        index: u32,
        args: Vec<WorldValue>,
        destination: &Option<Place>,
    ) -> Result<(), String> {
        let Some(body) = self.module.fns.get(index as usize) else {
            return Err(format!("no function #{index}"));
        };
        if args.len() != body.params.len() {
            return Err(format!(
                "`{}` takes {} argument(s), {} given",
                body.name,
                body.params.len(),
                args.len()
            ));
        }

        let mut slots = self.fresh_function_slots(index as usize);
        for (param, value) in body.params.iter().zip(args) {
            if let Some(slot) = slots.get_mut(param.0 as usize) {
                *slot = value;
            }
        }

        let continuation = self.frames.last().map(|frame| Continuation {
            block: frame.block,
            statement: frame.statement,
            destination: destination.clone(),
        });

        self.frames.push(Frame {
            body: BodyRef::Function(index as usize),
            block: BlockId::ENTRY,
            statement: 0,
            slots,
            ret: continuation,
        });
        Ok(())
    }
}
