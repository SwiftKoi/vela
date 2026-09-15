//! Reaching into the machine's stack and the module's tables.
//!
//! Kept apart from the step loop because it is a different kind of question: the loop
//! decides *where control goes*, and everything here decides *what a value is*.

use vela_bytecode::{FuncDef, Op};
use vela_world::{Command, Value};

use crate::fault::Fault;
use crate::machine::{BodyRef, Vm};

impl Vm {
    /// Records the command a `Cmd` built, for the `Yield` that follows it.
    pub(crate) fn set_pending(&mut self, command: Command) {
        self.pending = Some(command);
    }

    /// Pushes a value.
    pub(crate) fn push(&mut self, value: Value) {
        self.stack.push(value);
    }

    /// Pops a value.
    pub(crate) fn pop(&mut self) -> Result<Value, Fault> {
        self.stack.pop().ok_or(Fault::EmptyStack { op: "pop" })
    }

    /// Pops a value that has to be an integer.
    pub(crate) fn pop_int(&mut self, op: Op) -> Result<i64, Fault> {
        match self.pop()? {
            Value::Int(number) => Ok(number),
            other => Err(Fault::WrongType {
                op: op.name(),
                expected: "an int",
                found: other.type_name(),
            }),
        }
    }

    /// Pops a value that has to be a boolean.
    pub(crate) fn pop_bool(&mut self, op: Op) -> Result<bool, Fault> {
        match self.pop()? {
            Value::Bool(flag) => Ok(flag),
            other => Err(Fault::WrongType {
                op: op.name(),
                expected: "a bool",
                found: other.type_name(),
            }),
        }
    }

    /// Reads a slot in the frame being run.
    pub(crate) fn local(&self, slot: u32) -> Result<Value, Fault> {
        let Some(frame) = self.frames.last() else {
            return Err(Fault::Finished);
        };
        Ok(self
            .stack
            .get(frame.base + slot as usize)
            .cloned()
            .unwrap_or(Value::None))
    }

    /// Writes a slot in the frame being run.
    pub(crate) fn set_local(&mut self, slot: u32, value: Value) -> Result<(), Fault> {
        let Some(frame) = self.frames.last() else {
            return Err(Fault::Finished);
        };
        let at = frame.base + slot as usize;
        while self.stack.len() <= at {
            self.stack.push(Value::None);
        }
        self.stack[at] = value;
        Ok(())
    }

    /// A string from the table.
    pub(crate) fn string(&self, id: u32) -> Result<String, Fault> {
        self.module
            .strings
            .get(vela_bytecode::StringId(id))
            .map(ToString::to_string)
            .ok_or(Fault::BadString(id))
    }

    /// The name of a `default`, by its index in the module.
    pub(crate) fn default_name(&self, index: u32) -> Result<String, Fault> {
        let definition = self
            .module
            .defaults
            .get(index as usize)
            .ok_or(Fault::BadConstant(index))?;
        self.string(definition.name.0)
    }

    /// A variant's position within its enum.
    pub(crate) fn variant_index(&self, enum_id: u32, variant: &str) -> Result<usize, Fault> {
        let definition = self
            .module
            .enums
            .get(enum_id as usize)
            .ok_or(Fault::BadConstant(enum_id))?;
        definition
            .variants
            .iter()
            .position(|candidate| self.module.strings.get(candidate.name) == Some(variant))
            .ok_or_else(|| Fault::BadSchema(variant.to_string()))
    }

    /// The body a frame is running.
    pub(crate) fn body(&self, reference: BodyRef) -> Option<&FuncDef> {
        match reference {
            BodyRef::Function(index) => self.module.fns.get(index as usize),
            BodyRef::Label(index) => self.module.labels.get(index as usize),
        }
    }
}
