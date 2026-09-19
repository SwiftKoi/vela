//! `setting("text_speed")`: the question whose answer is a setting rather than a yes or no.
//!
//! A screen decides from what it has — a parameter, a variable, a loop element, a literal — and from what
//! the host says (`SCREENS.md §2.6`). `variant("pc")` is that question as a truth value; this is the same
//! question as a *value*, which is what a settings screen draws: `bar value = setting("text_speed")`, and
//! `if setting("skip_unseen")` for a checkbox that has to know what it is flipping.
//!
//! It answers from the store `RUNTIME.md §2.1` describes, and the store reaches it through [`Args`] the way
//! the variants do — because a setting has to reach an arm, a loop body and a `use` argument exactly the
//! way a name does, and a scope that carried one and not the other would make the answer depend on which
//! walk was asking.

use vela_world::Value as Stored;

use crate::settings::{SettingDecl, SettingTy};
use crate::value::{Args, Value};

use super::value_of;

/// The name a screen asks a setting with.
pub(crate) const SETTING: &str = "setting";

/// Whether this word is the settings question.
pub(crate) fn is_question(name: &str) -> bool {
    name == SETTING
}

/// The answer to a `setting("name")` call.
///
/// A name the engine does not know, an argument that is not a string literal, and a call with no argument
/// all answer `none` — the total-evaluator rule `variant_answer` follows: every expression a screen body
/// can hold has a value, and the checker is what tells the author that the question cannot mean anything
/// (`E5020`).
///
/// A setting nobody has chosen answers its **declaration**, which is what makes a screen draw what the
/// engine would do, and what makes a checkbox's first press a change rather than a no-op on a value the
/// player never chose.
pub(super) fn answer(args: &[vela_syntax::Expr], values: &Args) -> Value {
    let [name, ..] = args else {
        return Value::None;
    };
    let Value::Str(name) = value_of(name, values) else {
        return Value::None;
    };
    match SettingDecl::named(&name) {
        Some(setting) => match values.preferences().get(&name) {
            Some(stored) => from_stored(stored),
            None => declared(setting),
        },
        None => Value::None,
    }
}

/// What a declaration says a setting is before anyone has chosen.
fn declared(setting: &SettingDecl) -> Value {
    match setting.ty {
        SettingTy::Bool => Value::Bool(setting.default == "true"),
        SettingTy::Number => Value::Num(setting.default.parse().unwrap_or(0.0)),
        SettingTy::Choice(_) => Value::Str(setting.default.to_string()),
    }
}

/// A stored setting, as the value a screen holds.
///
/// The two types are the two halves of one boundary (`settings::world_value` is the other direction): what
/// a screen cannot hold — a struct, an enum, a function — answers `none` rather than being flattened into
/// something that would draw.
fn from_stored(value: &Stored) -> Value {
    match value {
        Stored::Bool(flag) => Value::Bool(*flag),
        Stored::Int(number) => Value::Num(*number as f64),
        Stored::Float(number) => Value::Num(*number),
        Stored::Str(text) => Value::Str(text.clone()),
        Stored::List(items) => Value::List(items.iter().map(from_stored).collect()),
        Stored::None
        | Stored::Map(_)
        | Stored::Struct { .. }
        | Stored::Enum { .. }
        | Stored::Function(_) => Value::None,
    }
}
