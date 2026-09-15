//! What the data instructions do, grouped by family.
//!
//! Kept apart from the dispatch so each family is readable on its own — and so the dispatch
//! stays a list of opcodes rather than a thousand lines of arithmetic.

use vela_bytecode::{Module, Op, Operand, StringId, TypeId};
use vela_world::{Key, Value, World};

use crate::aggregate::{
    command, enum_field, enum_tag, field_get, field_set, index, index_set, list_new, map_new,
    structure, variant,
};
use crate::fault::Fault;
use crate::machine::Vm;

/// The instructions that storage.
pub(super) fn storage(
    vm: &mut Vm,
    op: Op,
    operand: &Operand,
    world: &mut World,
) -> Result<(), Fault> {
    match op {
        Op::CallEffect => effect(vm, operand, world)?,
        Op::Nop => {}
        Op::Dup => {
            let value = vm.pop()?;
            vm.push(value.clone());
            vm.push(value);
        }
        Op::Pop => {
            vm.pop()?;
        }
        Op::ConstI => match operand {
            Operand::I64(number) => vm.push(Value::Int(*number)),
            other => vm.push(Value::Int(i64::from(other.u32().unwrap_or(0)))),
        },
        Op::ConstF => match operand {
            Operand::F64(number) => vm.push(Value::Float(*number)),
            other => vm.push(Value::Float(f64::from(other.u32().unwrap_or(0)))),
        },
        Op::ConstB => vm.push(Value::Bool(operand.u32().unwrap_or(0) != 0)),
        Op::ConstNone => vm.push(Value::None),
        Op::ConstS => {
            let text = vm.string(operand.u32().unwrap_or(0))?;
            vm.push(Value::Str(text));
        }
        Op::ConstFn => vm.push(Value::Function(name_of_function(
            vm,
            operand.u32().unwrap_or(0),
        )?)),
        Op::LoadLocal => {
            let value = vm.local(operand.u32().unwrap_or(0))?;
            vm.push(value);
        }
        Op::StoreLocal => {
            let value = vm.pop()?;
            vm.set_local(operand.u32().unwrap_or(0), value)?;
        }
        Op::LoadDefault => {
            let name = vm.default_name(operand.u32().unwrap_or(0))?;
            vm.push(world.get(&name).cloned().unwrap_or(Value::None));
        }
        Op::StoreDefault => {
            let name = vm.default_name(operand.u32().unwrap_or(0))?;
            let value = vm.pop()?;
            world.set(name, value);
        }
        other => {
            return Err(Fault::BadSchema(format!(
                "{other:?} is not a storage instruction"
            )));
        }
    }
    Ok(())
}

/// The instructions that compute.
pub(super) fn compute(vm: &mut Vm, op: Op, operand: &Operand) -> Result<(), Fault> {
    match op {
        Op::AddI | Op::SubI | Op::MulI | Op::DivI | Op::ModI => arithmetic(vm, op)?,
        Op::NegI => {
            let number = vm.pop_int(op)?;
            vm.push(Value::Int(number.wrapping_neg()));
        }
        Op::AddF | Op::SubF | Op::MulF | Op::DivF => float_arithmetic(vm, op)?,
        Op::NegF => match vm.pop()? {
            Value::Float(number) => vm.push(Value::Float(-number)),
            other => return Err(wrong(op, "a float", &other)),
        },
        Op::EqI | Op::EqF | Op::EqS | Op::EqB => {
            let right = vm.pop()?;
            let left = vm.pop()?;
            vm.push(Value::Bool(left == right));
        }
        Op::LtI
        | Op::LtF
        | Op::LtS
        | Op::LeI
        | Op::LeF
        | Op::LeS
        | Op::GtI
        | Op::GtF
        | Op::GtS
        | Op::GeI
        | Op::GeF
        | Op::GeS => ordered(vm, op)?,
        Op::Not => {
            let value = vm.pop()?;
            let Value::Bool(flag) = value else {
                return Err(wrong(op, "a bool", &value));
            };
            vm.push(Value::Bool(!flag));
        }
        Op::Concat => concat(vm, operand)?,
        Op::ToStr => {
            let value = vm.pop()?;
            vm.push(Value::Str(value.to_string()));
        }
        Op::ToInt => {
            let value = vm.pop()?;
            vm.push(to_int(value));
        }
        Op::ToFloat => {
            let value = vm.pop()?;
            vm.push(to_float(value));
        }
        Op::ToBool => {
            let value = vm.pop()?;
            vm.push(to_bool(value));
        }
        Op::IsNone => {
            let value = vm.pop()?;
            vm.push(Value::Bool(value == Value::None));
        }
        Op::Unwrap => {
            let value = vm.pop()?;
            if value == Value::None {
                return Err(Fault::UnwrapNone);
            }
            vm.push(value);
        }
        Op::UnwrapOr => {
            let fallback = vm.pop()?;
            let value = vm.pop()?;
            vm.push(if value == Value::None {
                fallback
            } else {
                value
            });
        }
        other => return Err(Fault::BadSchema(format!("{other:?} is not a computation"))),
    }
    Ok(())
}

/// The instructions that aggregate.
pub(super) fn aggregate(vm: &mut Vm, op: Op, operand: &Operand) -> Result<(), Fault> {
    match op {
        Op::ListNew => list_new(vm, operand)?,
        Op::MapNew => map_new(vm, operand)?,
        Op::ListGet | Op::MapGet => index(vm, op)?,
        Op::MapHas => {
            let key = vm.pop()?;
            let Value::Map(entries) = vm.pop()? else {
                return Err(Fault::WrongType {
                    op: op.name(),
                    expected: "a map",
                    found: "something else",
                });
            };
            vm.push(Value::Bool(
                Key::of(&key).is_some_and(|key| entries.contains_key(&key)),
            ));
        }
        Op::ListSet | Op::MapSet => index_set(vm, op)?,
        Op::ListLen => {
            let value = vm.pop()?;
            let length = match &value {
                Value::List(items) => items.len(),
                Value::Map(entries) => entries.len(),
                Value::Str(text) => text.chars().count(),
                other => return Err(wrong(op, "a list", other)),
            };
            vm.push(Value::Int(length as i64));
        }
        Op::StructNew => structure(vm, operand)?,
        Op::FieldGet => field_get(vm, operand)?,
        Op::FieldSet => field_set(vm, operand)?,
        Op::EnumNew => variant(vm, operand)?,
        Op::EnumTag => enum_tag(vm, operand)?,
        Op::EnumField => enum_field(vm, operand)?,
        Op::Cmd => command(vm, operand)?,
        other => {
            return Err(Fault::BadSchema(format!(
                "{other:?} is not a data instruction"
            )));
        }
    }
    Ok(())
}
fn arithmetic(vm: &mut Vm, op: Op) -> Result<(), Fault> {
    let right = vm.pop_int(op)?;
    let left = vm.pop_int(op)?;
    let value = match op {
        Op::AddI => left.wrapping_add(right),
        Op::SubI => left.wrapping_sub(right),
        Op::MulI => left.wrapping_mul(right),
        Op::DivI => left.checked_div(right).ok_or(Fault::DivisionByZero)?,
        _ => left.checked_rem(right).ok_or(Fault::DivisionByZero)?,
    };
    vm.push(Value::Int(value));
    Ok(())
}

/// Float arithmetic.
fn float_arithmetic(vm: &mut Vm, op: Op) -> Result<(), Fault> {
    let right = as_float(vm.pop()?, op)?;
    let left = as_float(vm.pop()?, op)?;
    vm.push(Value::Float(match op {
        Op::AddF => left + right,
        Op::SubF => left - right,
        Op::MulF => left * right,
        _ => left / right,
    }));
    Ok(())
}

/// Ordering, which each type does with its own comparison.
fn ordered(vm: &mut Vm, op: Op) -> Result<(), Fault> {
    let right = vm.pop()?;
    let left = vm.pop()?;

    let ordering = match (&left, &right) {
        (Value::Int(a), Value::Int(b)) => a.partial_cmp(b),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
        (Value::Str(a), Value::Str(b)) => Some(a.cmp(b)),
        _ => return Err(wrong(op, "two values of one type", &left)),
    };

    let Some(ordering) = ordering else {
        // `nan` compares with nothing, including itself. `false` is what a story wants from
        // `x < nan`, rather than a fault.
        vm.push(Value::Bool(false));
        return Ok(());
    };

    let result = match op {
        Op::LtI | Op::LtF | Op::LtS => ordering.is_lt(),
        Op::LeI | Op::LeF | Op::LeS => ordering.is_le(),
        Op::GtI | Op::GtF | Op::GtS => ordering.is_gt(),
        _ => ordering.is_ge(),
    };
    vm.push(Value::Bool(result));
    Ok(())
}

/// String concatenation.
fn concat(vm: &mut Vm, operand: &Operand) -> Result<(), Fault> {
    let count = operand.count().max(2);
    let mut parts = Vec::with_capacity(count);
    for _ in 0..count {
        let value = vm.pop()?;
        let Value::Str(text) = value else {
            return Err(Fault::WrongType {
                op: "concat",
                expected: "a str",
                found: value.type_name(),
            });
        };
        parts.push(text);
    }
    parts.reverse();
    vm.push(Value::Str(parts.concat()));
    Ok(())
}
/// Calls a host capability.
///
/// Dispatched by *name*, because that is the only thing a separately built module and engine
/// can agree on. An effect the host does not implement is `Fault::CapabilityDenied` rather
/// than a silently wrong answer — `RUNTIME.md §3`: a capability a script did not declare is
/// not callable, and neither is one the host does not provide.
fn effect(vm: &mut Vm, operand: &Operand, world: &mut World) -> Result<(), Fault> {
    let (index, arity) = match operand {
        Operand::Pair(index, arity) => (*index, *arity as usize),
        _ => return Err(Fault::NoEffect(operand.u32().unwrap_or(0))),
    };

    let name = vm
        .module()
        .effects
        .get(index as usize)
        .map(|effect| effect.name)
        .ok_or(Fault::NoEffect(index))?;
    let name = string_of(vm.module(), name)?;

    let mut args = Vec::with_capacity(arity);
    for _ in 0..arity {
        args.push(vm.pop()?);
    }
    args.reverse();

    let value = match (name.as_str(), args.as_slice()) {
        // The RNG lives in `World`, so `rand` needs no host at all (`RUNTIME.md §3`) — and
        // because it is state rather than a service, a replay with the same log draws the
        // same numbers.
        ("rand.int", [Value::Int(low), Value::Int(high)]) => Value::Int(world.rng.int(*low, *high)),
        ("rand.float", []) => Value::Float(world.rng.float()),
        // The clock is injected and advanced by explicit ticks, never read from the
        // operating system (`ARCHITECTURE.md §4`).
        ("time.now", []) => Value::Float(world.clock.0 as f64),
        _ => return Err(Fault::CapabilityDenied(name)),
    };

    vm.push(value);
    Ok(())
}

/// The name of a function, by index.
fn name_of_function(vm: &Vm, index: u32) -> Result<String, Fault> {
    let body = vm
        .module()
        .fns
        .get(index as usize)
        .ok_or(Fault::BadFunction(index))?;
    string_of(vm.module(), body.name)
}

/// A string from the table.
pub(super) fn string_of(module: &Module, id: StringId) -> Result<String, Fault> {
    module
        .strings
        .get(id)
        .map(ToString::to_string)
        .ok_or(Fault::BadString(id.0))
}

/// A fault for an operand of the wrong type.
pub(super) fn wrong(op: Op, expected: &'static str, found: &Value) -> Fault {
    Fault::WrongType {
        op: op.name(),
        expected,
        found: found.type_name(),
    }
}

/// A value that has to be a float.
fn as_float(value: Value, op: Op) -> Result<f64, Fault> {
    match value {
        Value::Float(number) => Ok(number),
        Value::Int(number) => Ok(number as f64),
        other => Err(wrong(op, "a float", &other)),
    }
}

/// A conversion that cannot fail.
fn to_int(value: Value) -> Value {
    match value {
        Value::Int(_) => value,
        Value::Float(number) => Value::Int(number as i64),
        Value::Bool(flag) => Value::Int(i64::from(flag)),
        Value::Str(text) => Value::Int(text.trim().parse().unwrap_or(0)),
        _ => Value::Int(0),
    }
}

/// A conversion that cannot fail.
fn to_float(value: Value) -> Value {
    match value {
        Value::Float(_) => value,
        Value::Int(number) => Value::Float(number as f64),
        Value::Str(text) => Value::Float(text.trim().parse().unwrap_or(0.0)),
        _ => Value::Float(0.0),
    }
}

/// A conversion that cannot fail.
fn to_bool(value: Value) -> Value {
    match value {
        Value::Bool(_) => value,
        Value::Int(number) => Value::Bool(number != 0),
        Value::None => Value::Bool(false),
        _ => Value::Bool(true),
    }
}

/// A type id, for a fault message.
#[allow(dead_code)]
fn type_label(module: &Module, ty: TypeId) -> String {
    module
        .types
        .get(ty)
        .map_or_else(|| "?".to_string(), |ty| format!("{ty:?}"))
}
