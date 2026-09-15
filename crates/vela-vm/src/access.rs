//! Reaching into the machine's stack and the module's tables.
//!
//! Kept apart from the step loop because it is a different kind of question: the loop
//! decides *where control goes*, and everything here decides *what a value is*.

use vela_bytecode::{ByteConst, FuncDef, Op};
use vela_world::{Command, Value, World};

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
    ///
    /// A slot that is not there is a fault rather than `none`: the verifier's rule 4 requires
    /// every read to be dominated by a write, so this can only mean the frame's base is wrong —
    /// and `none` is exactly the value that made a wrong base look like a story bug.
    pub(crate) fn local(&self, slot: u32) -> Result<Value, Fault> {
        let Some(frame) = self.frames.last() else {
            return Err(Fault::Finished);
        };
        self.stack
            .get(frame.base + slot as usize)
            .cloned()
            .ok_or(Fault::BadLocal(slot))
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

    /// The value a `default` was declared with.
    ///
    /// A `default` is *declared* with a value, not born holding `none`: `default trust: int = 0`
    /// means `trust` is zero until something writes it. Reading one used to hand back `none` for
    /// any slot the world had not been told about yet, which is how `trust + 1` became
    /// `add.i` given `none` on the first line that touched it.
    pub(crate) fn initial_value(&self, index: u32) -> Result<Value, Fault> {
        let definition = self
            .module
            .defaults
            .get(index as usize)
            .ok_or(Fault::BadConstant(index))?;
        self.const_value(definition.init.0)
    }

    /// Puts every `default` in the module into `world` at its declared value.
    ///
    /// What a *fresh* world starts with (`RUNTIME.md §2`: the world's defaults come from the
    /// `default` declarations). A restored world is not seeded — it holds what the save held —
    /// which is why `initial_value` is also the fallback for a name the world does not have.
    pub(crate) fn seed(&self, world: &mut World) -> Result<(), Fault> {
        for index in 0..self.module.defaults.len() {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            let name = self.default_name(index)?;
            let value = self.initial_value(index)?;
            world.set(name, value);
        }
        Ok(())
    }

    /// A pooled constant, as a world value.
    fn const_value(&self, index: u32) -> Result<Value, Fault> {
        let constant = self
            .module
            .consts
            .get(vela_bytecode::ConstId(index))
            .ok_or(Fault::BadConstant(index))?;

        match constant {
            ByteConst::None => Ok(Value::None),
            ByteConst::Bool(flag) => Ok(Value::Bool(*flag)),
            ByteConst::Int(number) => Ok(Value::Int(*number)),
            ByteConst::Float(number) => Ok(Value::Float(*number)),
            ByteConst::Str(text) => Ok(Value::Str(self.string(text.0)?)),
            // A constant variant has no payload — the constant pool cannot hold one — so its
            // fields are empty and the value is the tag alone.
            ByteConst::Variant { enum_id, variant } => {
                let definition = self
                    .module
                    .enums
                    .get(*enum_id as usize)
                    .ok_or(Fault::BadConstant(*enum_id))?;
                let declared = definition
                    .variants
                    .get(*variant as usize)
                    .ok_or(Fault::BadConstant(*variant))?;
                Ok(Value::Enum {
                    name: self.string(definition.name.0)?,
                    variant: self.string(declared.name.0)?,
                    fields: Vec::new(),
                })
            }
            ByteConst::Function(index) => {
                Ok(Value::Function(crate::ops::name_of_function(self, *index)?))
            }
        }
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
