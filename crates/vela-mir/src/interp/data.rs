//! Reading and writing values.
//!
//! A *place* is somewhere a value can be written; an *operand* is somewhere one can be read
//! from. They are different questions and the MIR types say so, which is why the code that
//! answers them is not the code that decides where control goes next.

use vela_world::{Key, Value as WorldValue};

use super::exec::constant;
use super::machine::Machine;
use crate::ir::{Operand, Place, Slot, Value};

impl Machine<'_> {
    /// Reads a value.
    pub(super) fn read(&self, value: Value) -> WorldValue {
        match value {
            Value::Slot(slot) => self.slot(slot),
            Value::Const(id) => constant(self.module, id),
        }
    }

    /// Reads a slot in the frame being run.
    pub(super) fn slot(&self, slot: Slot) -> WorldValue {
        self.frames
            .last()
            .and_then(|frame| frame.slots.get(slot.0 as usize))
            .cloned()
            .unwrap_or(WorldValue::None)
    }

    /// Reads where an operand points.
    pub(super) fn operand(&self, operand: &Operand) -> Result<WorldValue, String> {
        match operand {
            Operand::Value(value) => Ok(self.read(*value)),
            Operand::Default(id) => Ok(self
                .module
                .defaults
                .get(id.0 as usize)
                .and_then(|definition| self.world.get(&definition.name).cloned())
                .unwrap_or(WorldValue::None)),
            Operand::Field { base, field } => {
                let base = self.read(*base);
                match base {
                    WorldValue::Struct { fields, .. } => Ok(fields
                        .iter()
                        .find(|(name, _)| name == field)
                        .map(|(_, value)| value.clone())
                        .unwrap_or(WorldValue::None)),
                    _ => Err(format!("`{field}` was read from something without fields")),
                }
            }
            Operand::Index { base, index } => {
                let base = self.read(*base);
                let index = self.read(*index);
                Ok(match base {
                    WorldValue::List(items) => match index {
                        WorldValue::Int(position) => usize::try_from(position)
                            .ok()
                            .and_then(|position| items.get(position).cloned())
                            .unwrap_or(WorldValue::None),
                        _ => WorldValue::None,
                    },
                    WorldValue::Map(entries) => Key::of(&index)
                        .and_then(|key| entries.get(&key).cloned())
                        .unwrap_or(WorldValue::None),
                    WorldValue::Str(text) => match index {
                        WorldValue::Int(position) => usize::try_from(position)
                            .ok()
                            .and_then(|position| text.chars().nth(position))
                            .map(|character| WorldValue::Str(character.to_string()))
                            .unwrap_or(WorldValue::None),
                        _ => WorldValue::None,
                    },
                    _ => WorldValue::None,
                })
            }
            Operand::Len { base } => Ok(match self.read(*base) {
                WorldValue::List(items) => WorldValue::Int(items.len() as i64),
                WorldValue::Map(entries) => WorldValue::Int(entries.len() as i64),
                WorldValue::Str(text) => WorldValue::Int(text.chars().count() as i64),
                _ => return Err("`len` was taken of something without a length".to_string()),
            }),
        }
    }

    /// Writes to a place.
    pub(super) fn write(&mut self, place: &Place, value: WorldValue) {
        match place {
            Place::Local(slot) => {
                if let Some(frame) = self.frames.last_mut()
                    && let Some(cell) = frame.slots.get_mut(slot.0 as usize)
                {
                    *cell = value;
                }
            }
            Place::Default(id) => {
                if let Some(definition) = self.module.defaults.get(id.0 as usize) {
                    self.world.set(definition.name.clone(), value);
                }
            }
            Place::Field { base, field } => {
                let mut container = self.read_place(base);
                if let WorldValue::Struct { fields, .. } = &mut container
                    && let Some(entry) = fields.iter_mut().find(|(name, _)| name == field)
                {
                    entry.1 = value;
                }
                self.write(base, container);
            }
            Place::Index { base, index } => {
                let mut container = self.read_place(base);
                let subscript = self.read(*index);
                match &mut container {
                    WorldValue::List(items) => {
                        if let WorldValue::Int(position) = subscript
                            && let Some(cell) = usize::try_from(position)
                                .ok()
                                .and_then(|position| items.get_mut(position))
                        {
                            *cell = value;
                        }
                    }
                    WorldValue::Map(entries) => {
                        if let Some(key) = Key::of(&subscript) {
                            entries.insert(key, value);
                        }
                    }
                    _ => {}
                }
                self.write(base, container);
            }
        }
    }

    /// Reads where a place points.
    pub(super) fn read_place(&self, place: &Place) -> WorldValue {
        match place {
            Place::Local(slot) => self.slot(*slot),
            Place::Default(id) => self
                .operand(&Operand::Default(*id))
                .unwrap_or(WorldValue::None),
            Place::Field { base, field } => self
                .operand(&Operand::Field {
                    base: self.read_place_value(base),
                    field: field.clone(),
                })
                .unwrap_or(WorldValue::None),
            Place::Index { base, index } => self
                .operand(&Operand::Index {
                    base: self.read_place_value(base),
                    index: *index,
                })
                .unwrap_or(WorldValue::None),
        }
    }

    /// A place as a readable value, for nesting one read inside another.
    ///
    /// Only a local can be read directly; anything else has to be materialised, which the
    /// caller does by writing it to a slot first.
    pub(super) fn read_place_value(&self, place: &Place) -> Value {
        match place {
            Place::Local(slot) => Value::Slot(*slot),
            _ => Value::Slot(Slot(u32::MAX)),
        }
    }
}
