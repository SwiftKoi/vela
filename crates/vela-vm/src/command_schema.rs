//! Building a `Command` from the schema a module carries.
//!
//! `BYTECODE.md §3.3`: a command is a schema registration, not an instruction. The module
//! says which variant it is and how many arguments follow; this turns that plus the values
//! on the stack into the structured `Command` the host consumes.
//!
//! The match below is over the *variant names* `vela-world` declares, so adding a command is
//! an entry in that table and a case here — and nothing else in the runtime needs to know.

use vela_bytecode::Module;
use vela_world::{Audio, Choice, Command, Key, Stage, Value};

use crate::fault::Fault;

/// Builds a command from its variant and arguments.
///
/// # Errors
///
/// Fails if the variant is not one the module declares, or if its arguments are not the
/// shapes the schema says. Both are compiler bugs: the verifier checks the arity, and the
/// code generator is the only thing that decides which variant to emit.
pub fn build_command(module: &Module, variant: u32, args: Vec<Value>) -> Result<Command, Fault> {
    let schema = module
        .cmds
        .get(variant as usize)
        .ok_or_else(|| Fault::BadSchema(format!("#{variant}")))?;
    let name = module
        .strings
        .get(schema.name)
        .unwrap_or_default()
        .to_string();

    let mut args = args.into_iter();
    let mut next = || args.next().unwrap_or(Value::None);

    match name.as_str() {
        "say" => Ok(Command::Say {
            speaker: text(next())?,
            attributes: texts(next())?,
            text: value_text(next())?,
            options: options(next())?,
            transition: text(next())?,
        }),
        "menu" => Ok(Command::Menu {
            prompt: text(next())?,
            choices: texts(next())?
                .into_iter()
                .enumerate()
                .map(|(index, text)| Choice { index, text })
                .collect(),
        }),
        "scene" | "show" | "hide" => Ok(Command::Stage {
            kind: match name.as_str() {
                "scene" => Stage::Scene,
                "show" => Stage::Show,
                _ => Stage::Hide,
            },
            image: value_text(next())?,
            attributes: texts(next())?,
            transforms: texts(next())?,
            transition: text(next())?,
        }),
        "with" => Ok(Command::Transition {
            name: value_text(next())?,
        }),
        "play" | "stop" | "queue" => Ok(Command::Audio {
            kind: match name.as_str() {
                "play" => Audio::Play,
                "stop" => Audio::Stop,
                _ => Audio::Queue,
            },
            channel: value_text(next())?,
            source: text(next())?,
            looping: matches!(next(), Value::Bool(true)),
            fade: number(next()),
        }),
        "pause" => Ok(Command::Pause {
            seconds: number(next()),
        }),
        "wait_click" => Ok(Command::WaitClick),
        other => Err(Fault::BadSchema(other.to_string())),
    }
}

/// A required string.
fn value_text(value: Value) -> Result<String, Fault> {
    match value {
        Value::Str(text) => Ok(text),
        other => Err(Fault::BadSchema(format!(
            "a str, not {}",
            other.type_name()
        ))),
    }
}

/// A string, or an absent one.
fn text(value: Value) -> Result<Option<String>, Fault> {
    match value {
        Value::None => Ok(None),
        Value::Str(text) => Ok(Some(text)),
        other => Err(Fault::BadSchema(format!(
            "a str, not {}",
            other.type_name()
        ))),
    }
}

/// A list of strings.
fn texts(value: Value) -> Result<Vec<String>, Fault> {
    let Value::List(items) = value else {
        return Ok(Vec::new());
    };
    items.into_iter().map(value_text).collect()
}

/// A float, or an absent one.
fn number(value: Value) -> Option<f64> {
    match value {
        Value::Float(number) => Some(number),
        Value::Int(number) => Some(number as f64),
        _ => None,
    }
}

/// A map of say-scoped options.
fn options(value: Value) -> Result<Vec<(String, Value)>, Fault> {
    let Value::Map(entries) = value else {
        return Ok(Vec::new());
    };
    Ok(entries
        .into_iter()
        .filter_map(|(key, value)| match key {
            Key::Str(name) => Some((name, value)),
            _ => None,
        })
        .collect())
}
