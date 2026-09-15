//! Operators, conversions, and the command schema.
//!
//! Kept apart from the machine because none of it needs a frame, a block, or a world: it
//! is the *meaning* of a MIR operation, which is exactly what a reference interpreter is
//! supposed to pin down.

use vela_syntax::{BinOp, UnOp};
use vela_world::{Audio, Choice, Command, CommandKind, Stage, Value};

/// Applies a binary operator.
#[must_use]
pub fn binary(op: BinOp, a: Value, b: Value) -> Option<Value> {
    use BinOp::{Add, And, Div, Eq, Ge, Gt, In, Is, IsNot, Le, Lt, Mul, Ne, NotIn, Or, Rem, Sub};

    // `LANGUAGE.md §5.7` forbids mixing numbers implicitly, so an int/float pair reaching
    // here is a checker bug rather than a case to coerce.
    match op {
        Add | Sub | Mul | Div | Rem => match (a, b) {
            (Value::Int(x), Value::Int(y)) => match op {
                Sub => Some(Value::Int(x.wrapping_sub(y))),
                Mul => Some(Value::Int(x.wrapping_mul(y))),
                Div if y != 0 => Some(Value::Int(x / y)),
                Rem if y != 0 => Some(Value::Int(x % y)),
                Add => Some(Value::Int(x.wrapping_add(y))),
                _ => None,
            },
            (Value::Float(x), Value::Float(y)) => Some(Value::Float(match op {
                Add => x + y,
                Sub => x - y,
                Mul => x * y,
                Div => x / y,
                _ => return None,
            })),
            // `+` on two strings is concatenation, which is how interpolation lowers.
            (Value::Str(x), Value::Str(y)) if op == Add => Some(Value::Str(format!("{x}{y}"))),
            _ => None,
        },

        Eq => Some(Value::Bool(a == b)),
        Ne => Some(Value::Bool(a != b)),
        Lt | Le | Gt | Ge => compare(op, a, b).map(Value::Bool),

        And => Some(Value::Bool(a.as_bool()? && b.as_bool()?)),
        Or => Some(Value::Bool(a.as_bool()? || b.as_bool()?)),

        Is => Some(Value::Bool(a == b)),
        IsNot => Some(Value::Bool(a != b)),
        In => Some(Value::Bool(contains(&b, &a))),
        NotIn => Some(Value::Bool(!contains(&b, &a))),

        // `??` is lowered to a branch, so it never reaches here.
        BinOp::Coalesce => None,
    }
}

/// Applies an ordering operator.
fn compare(op: BinOp, a: Value, b: Value) -> Option<bool> {
    let ordering = match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.partial_cmp(&y)?,
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(&y)?,
        (Value::Str(x), Value::Str(y)) => x.cmp(&y),
        _ => return None,
    };
    Some(match op {
        BinOp::Lt => ordering.is_lt(),
        BinOp::Le => ordering.is_le(),
        BinOp::Gt => ordering.is_gt(),
        BinOp::Ge => ordering.is_ge(),
        _ => return None,
    })
}

/// Whether an aggregate holds a value.
fn contains(aggregate: &Value, needle: &Value) -> bool {
    match aggregate {
        Value::List(items) => items.iter().any(|item| item == needle),
        Value::Map(entries) => {
            vela_world::Key::of(needle).is_some_and(|key| entries.contains_key(&key))
        }
        Value::Str(text) => match needle {
            Value::Str(part) => text.contains(part.as_str()),
            _ => false,
        },
        _ => false,
    }
}

/// Applies a prefix operator.
#[must_use]
pub fn unary(op: UnOp, a: Value) -> Option<Value> {
    match (op, a) {
        (UnOp::Neg, Value::Int(number)) => Some(Value::Int(-number)),
        (UnOp::Neg, Value::Float(number)) => Some(Value::Float(-number)),
        (UnOp::Not, Value::Bool(flag)) => Some(Value::Bool(!flag)),
        _ => None,
    }
}

/// A conversion the language provides.
#[must_use]
pub fn convert(name: &str, value: Value) -> Option<Value> {
    match name {
        "str" => Some(Value::Str(value.to_string())),
        "int" => match value {
            Value::Int(_) => Some(value),
            Value::Float(number) => Some(Value::Int(number as i64)),
            Value::Bool(flag) => Some(Value::Int(i64::from(flag))),
            Value::Str(text) => text.trim().parse().ok().map(Value::Int),
            _ => None,
        },
        "float" => match value {
            Value::Float(_) => Some(value),
            Value::Int(number) => Some(Value::Float(number as f64)),
            Value::Str(text) => text.trim().parse().ok().map(Value::Float),
            _ => None,
        },
        "bool" => match value {
            Value::Bool(_) => Some(value),
            Value::Int(number) => Some(Value::Bool(number != 0)),
            _ => None,
        },
        _ => None,
    }
}

/// Assembles a command from its argument list.
///
/// The schemas are documented in `crate::lower::story`. An argument that is not the shape
/// the schema expects is a compiler bug, so it faults rather than defaulting — a
/// reference interpreter that quietly substitutes `none` would hide exactly the mistakes
/// it exists to catch.
#[must_use]
pub fn command(kind: CommandKind, args: &[Value]) -> Option<Command> {
    let mut args = args.iter();
    let mut next = || args.next().cloned();

    match kind {
        CommandKind::Say => Some(Command::Say {
            speaker: optional_text(next()?)?,
            attributes: texts(next()?)?,
            text: text(next()?)?,
            options: options(next()?)?,
            transition: optional_text(next()?)?,
        }),
        CommandKind::Menu => Some(Command::Menu {
            prompt: optional_text(next()?)?,
            choices: texts(next()?)?
                .into_iter()
                .enumerate()
                .map(|(index, text)| Choice { index, text })
                .collect(),
        }),
        CommandKind::Scene | CommandKind::Show | CommandKind::Hide => Some(Command::Stage {
            kind: match kind {
                CommandKind::Scene => Stage::Scene,
                CommandKind::Show => Stage::Show,
                _ => Stage::Hide,
            },
            image: text(next()?)?,
            attributes: texts(next()?)?,
            transforms: texts(next()?)?,
            transition: optional_text(next()?)?,
        }),
        CommandKind::Transition => Some(Command::Transition {
            name: text(next()?)?,
        }),
        CommandKind::Play | CommandKind::Stop | CommandKind::Queue => Some(Command::Audio {
            kind: match kind {
                CommandKind::Play => Audio::Play,
                CommandKind::Stop => Audio::Stop,
                _ => Audio::Queue,
            },
            channel: text(next()?)?,
            source: optional_text(next()?)?,
            looping: matches!(next()?, Value::Bool(true)),
            fade: optional_float(next()?)?,
        }),
        CommandKind::Pause => Some(Command::Pause {
            seconds: optional_float(next()?)?,
        }),
        CommandKind::WaitClick => Some(Command::WaitClick),
    }
}

/// A string.
fn text(value: Value) -> Option<String> {
    match value {
        Value::Str(text) => Some(text),
        _ => None,
    }
}

/// A string, or an absent one.
fn optional_text(value: Value) -> Option<Option<String>> {
    match value {
        Value::None => Some(None),
        Value::Str(text) => Some(Some(text)),
        _ => None,
    }
}

/// A float, or an absent one.
fn optional_float(value: Value) -> Option<Option<f64>> {
    match value {
        Value::None => Some(None),
        Value::Float(number) => Some(Some(number)),
        Value::Int(number) => Some(Some(number as f64)),
        _ => None,
    }
}

/// A list of strings.
fn texts(value: Value) -> Option<Vec<String>> {
    match value {
        Value::List(items) => items.into_iter().map(text).collect(),
        _ => None,
    }
}

/// A map of say-scoped options.
fn options(value: Value) -> Option<Vec<(String, Value)>> {
    match value {
        Value::Map(entries) => entries
            .into_iter()
            .map(|(key, value)| match key {
                vela_world::Key::Str(name) => Some((name, value)),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}
