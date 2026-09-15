//! Building and reading the aggregates.
//!
//! Lists, maps, structs, enums, and the commands that carry them. Grouped because they share
//! one property the arithmetic does not: **an aggregate is a value**, so writing into one
//! produces a new one for the compiler to store back rather than mutating in place.

use std::collections::BTreeMap;

use vela_bytecode::{Op, Operand};
use vela_world::{Key, Value};

use crate::fault::Fault;
use crate::machine::Vm;
use crate::ops::{string_of, wrong};

/// A list from `n` values.
pub(super) fn list_new(vm: &mut Vm, operand: &Operand) -> Result<(), Fault> {
    let count = operand.count();
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        items.push(vm.pop()?);
    }
    items.reverse();
    vm.push(Value::List(items));
    Ok(())
}

/// A map from `n` key-value pairs.
pub(super) fn map_new(vm: &mut Vm, operand: &Operand) -> Result<(), Fault> {
    let count = operand.count();
    let mut entries = BTreeMap::new();
    for _ in 0..count {
        let value = vm.pop()?;
        let key = vm.pop()?;
        let key = Key::of(&key).ok_or_else(|| Fault::WrongType {
            op: "map.new",
            expected: "an int, a str, or a bool",
            found: key.type_name(),
        })?;
        entries.insert(key, value);
    }
    vm.push(Value::Map(entries));
    Ok(())
}

/// Reading an element or a key.
pub(super) fn index(vm: &mut Vm, op: Op) -> Result<(), Fault> {
    let subscript = vm.pop()?;
    let aggregate = vm.pop()?;

    let value = match (&aggregate, &subscript) {
        (Value::List(items), Value::Int(position)) => usize::try_from(*position)
            .ok()
            .and_then(|position| items.get(position).cloned())
            .unwrap_or(Value::None),
        (Value::Map(entries), _) => Key::of(&subscript)
            .and_then(|key| entries.get(&key).cloned())
            .unwrap_or(Value::None),
        (Value::Str(text), Value::Int(position)) => usize::try_from(*position)
            .ok()
            .and_then(|position| text.chars().nth(position))
            .map(|character| Value::Str(character.to_string()))
            .unwrap_or(Value::None),
        _ => return Err(wrong(op, "a list or a map", &aggregate)),
    };
    vm.push(value);
    Ok(())
}

/// Writing an element or a key, which rebuilds the aggregate.
///
/// Aggregates are values, not references, so this leaves the *updated* aggregate on the
/// stack for the compiler to store back where it came from. An in-place update would need
/// the runtime to know where the aggregate was read from, which is exactly the aliasing a
/// story engine should not have.
pub(super) fn index_set(vm: &mut Vm, op: Op) -> Result<(), Fault> {
    let value = vm.pop()?;
    let subscript = vm.pop()?;
    let mut aggregate = vm.pop()?;

    match (&mut aggregate, &subscript) {
        (Value::List(items), Value::Int(position)) => {
            if let Some(cell) = usize::try_from(*position)
                .ok()
                .and_then(|position| items.get_mut(position))
            {
                *cell = value;
            }
        }
        (Value::Map(entries), _) => {
            if let Some(key) = Key::of(&subscript) {
                entries.insert(key, value);
            }
        }
        _ => return Err(wrong(op, "a list or a map", &aggregate)),
    }

    vm.push(aggregate);
    Ok(())
}

/// A struct from `n` field values.
pub(super) fn structure(vm: &mut Vm, operand: &Operand) -> Result<(), Fault> {
    // The operand is the struct id and the field count; the *names* come from the schema the
    // module carries, so nothing here has to know what a struct's fields are called.
    let (struct_id, count) = match operand {
        Operand::Pair(id, count) => (*id, *count as usize),
        other => (other.u32().unwrap_or(0), 0),
    };

    let (name, names) = {
        let definition = vm
            .module()
            .structs
            .get(struct_id as usize)
            .ok_or(Fault::BadSchema(format!("struct #{struct_id}")))?;
        let name = string_of(vm.module(), definition.name)?;
        let names: Vec<String> = definition
            .fields
            .iter()
            .map(|field| string_of(vm.module(), field.name).unwrap_or_default())
            .collect();
        (name, names)
    };

    let mut fields = Vec::with_capacity(count);
    for _ in 0..count {
        fields.push(vm.pop()?);
    }
    fields.reverse();

    let named: Vec<(String, Value)> = names.into_iter().zip(fields).collect();
    vm.push(Value::Struct {
        name,
        fields: named,
    });
    Ok(())
}

/// A variant from its payload.
pub(super) fn variant(vm: &mut Vm, operand: &Operand) -> Result<(), Fault> {
    let (enum_id, variant_id) = match operand {
        Operand::Pair(enum_id, variant) => (*enum_id, *variant),
        _ => return Err(Fault::BadSchema("enum.new".to_string())),
    };

    let (name, variant_name, arity) = {
        let definition = vm
            .module()
            .enums
            .get(enum_id as usize)
            .ok_or(Fault::BadSchema(format!("enum #{enum_id}")))?;
        let variant = definition
            .variants
            .get(variant_id as usize)
            .ok_or(Fault::BadSchema(format!("variant #{variant_id}")))?;
        (
            string_of(vm.module(), definition.name)?,
            string_of(vm.module(), variant.name)?,
            variant.fields.len(),
        )
    };

    let mut fields = Vec::with_capacity(arity);
    for _ in 0..arity {
        fields.push(vm.pop()?);
    }
    fields.reverse();

    vm.push(Value::Enum {
        name,
        variant: variant_name,
        fields,
    });
    Ok(())
}

/// An enum's tag, as a position rather than a name.
///
/// The dispatch table is indexed, so a name would need a lookup per dispatch. Doing it once
/// here is what keeps a `match` O(1) at run time.
pub(super) fn enum_tag(vm: &mut Vm, operand: &Operand) -> Result<(), Fault> {
    let enum_id = operand.u32().unwrap_or(0);
    let value = vm.pop()?;
    let Value::Enum { variant, .. } = value else {
        return Err(wrong(Op::EnumTag, "an enum", &value));
    };
    let index = vm.variant_index(enum_id, &variant)?;
    vm.push(Value::Int(index as i64));
    Ok(())
}

/// A variant's payload, by position.
pub(super) fn enum_field(vm: &mut Vm, operand: &Operand) -> Result<(), Fault> {
    let position = operand.u32().unwrap_or(0) as usize;
    let value = vm.pop()?;
    let Value::Enum { fields, .. } = value else {
        return Err(wrong(Op::EnumField, "an enum", &value));
    };
    vm.push(fields.get(position).cloned().unwrap_or(Value::None));
    Ok(())
}

/// A struct's field, by name.
pub(super) fn field_get(vm: &mut Vm, operand: &Operand) -> Result<(), Fault> {
    let Operand::Str(id) = operand else {
        return Err(Fault::BadSchema("field.get".to_string()));
    };
    let field = vm.string(*id)?;
    let value = vm.pop()?;
    let Value::Struct { fields, .. } = value else {
        return Err(wrong(Op::FieldGet, "a struct", &value));
    };
    vm.push(
        fields
            .into_iter()
            .find(|(name, _)| *name == field)
            .map(|(_, value)| value)
            .unwrap_or(Value::None),
    );
    Ok(())
}

/// Writes a struct's field, leaving the updated struct on the stack.
pub(super) fn field_set(vm: &mut Vm, operand: &Operand) -> Result<(), Fault> {
    let Operand::Str(id) = operand else {
        return Err(Fault::BadSchema("field.set".to_string()));
    };
    let field = vm.string(*id)?;
    let value = vm.pop()?;
    let mut object = vm.pop()?;

    let Value::Struct { fields, .. } = &mut object else {
        return Err(wrong(Op::FieldSet, "a struct", &object));
    };
    if let Some(entry) = fields.iter_mut().find(|(name, _)| *name == field) {
        entry.1 = value;
    }

    vm.push(object);
    Ok(())
}

/// Builds a command for the next `Yield`.
pub(super) fn command(vm: &mut Vm, operand: &Operand) -> Result<(), Fault> {
    let Operand::Pair(variant, count) = operand else {
        return Err(Fault::BadSchema("cmd".to_string()));
    };

    let mut args = Vec::with_capacity(*count as usize);
    for _ in 0..*count {
        args.push(vm.pop()?);
    }
    args.reverse();

    let command = crate::command_schema::build_command(vm.module(), *variant, args)?;
    vm.set_pending(command);
    Ok(())
}
