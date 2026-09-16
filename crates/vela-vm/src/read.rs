//! Reading a value out of a running story.
//!
//! One caller, one method, and a whole file for it, because it is the only place the machine is asked a
//! question *about* a run rather than asked to continue one. `vela test` compiles each `expect` as a
//! nullary function and reads it here, in the world as it is part way through the story.
//!
//! It lives beside `machine.rs` rather than inside it for the reason the size budget exists: the step
//! loop is one screen and this is another, and a reader looking for "how does a call work" should not
//! have to find it among the reading.

use vela_world::{Value, World};

use crate::fault::Fault;
use crate::machine::{BodyRef, Vm};

impl Vm {
    /// Calls a function and returns what it produced, leaving the story where it was.
    ///
    /// This is the runner's primitive: a test asserts something about the world *as it is* in the
    /// middle of a run, so the assertion has to be a value the module produces rather than a second
    /// interpreter's opinion about what the expression means. `vela-test` compiles each `expect` as a
    /// nullary function and reads it here.
    ///
    /// Runs until the call returns and no further. [`Vm::run`] would carry on into the caller, which
    /// is the story — and the story is suspended on a command the host has not answered yet, so
    /// running it would present a command twice.
    ///
    /// # Errors
    ///
    /// Fails if there is no such function, if it takes parameters (there is nothing to pass them), if
    /// it suspends (it is a piece of story rather than an expression), or if it faults.
    pub fn call(&mut self, world: &mut World, name: &str) -> Result<Value, Fault> {
        let found = self
            .module
            .fns
            .iter()
            .position(|body| self.module.strings.get(body.name) == Some(name));

        let Some(index) = found else {
            return Err(Fault::NotAValue {
                name: name.to_string(),
                why: "there is no such function",
            });
        };
        if self
            .module
            .fns
            .get(index)
            .is_some_and(|body| !body.params.is_empty())
        {
            return Err(Fault::NotAValue {
                name: name.to_string(),
                why: "it takes parameters, and there is nothing to pass it",
            });
        }

        let depth = self.frames.len();
        let stack = self.stack.len();
        self.enter(BodyRef::Function(u32::try_from(index).unwrap_or(u32::MAX)))?;

        while self.frames.len() > depth {
            if self.execute(world)?.is_some() {
                return Err(Fault::NotAValue {
                    name: name.to_string(),
                    why: "it suspends, so it is a piece of story rather than an expression",
                });
            }
        }

        let value = self.pop()?;
        // A call leaves the stack exactly as it found it, plus its result — which is what the frame
        // discipline in this file exists to guarantee, and is worth asserting rather than assuming
        // because a caller reading a value out of a suspended story cannot tell a leak from a local.
        self.stack.truncate(stack);
        Ok(value)
    }

    /// The labels entered so far, in the order they were first entered.
    #[must_use]
    pub fn entered_labels(&self) -> Vec<String> {
        self.entered
            .iter()
            .filter_map(|index| self.module.labels.get(*index as usize))
            .map(|label| {
                self.module
                    .strings
                    .get(label.name)
                    .unwrap_or("")
                    .to_string()
            })
            .collect()
    }

    /// Notes a label was entered, once.
    pub(crate) fn record_label(&mut self, index: u32) {
        if !self.entered.contains(&index) {
            self.entered.push(index);
        }
    }
}
