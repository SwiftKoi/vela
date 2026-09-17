//! The values a screen body holds, and the scope it holds them in.
//!
//! Split from `eval.rs` by `REPO_LAYOUT.md §3.1`'s third recipe — the data model out of the
//! algorithms that read it. A `Value` is what a screen was handed or built (`SCREENS.md §7`, `§2.4`),
//! and an `Args` is the scope those values live in; the evaluator beside this file is what produces
//! one from an expression.

use crate::actions::Action;

/// A value a screen argument can hold.
///
/// The small set a screen body can branch on. A `screen` is called by the runtime with
/// arguments it already has as `World` values, and this is the shape they arrive in.
#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    /// A string.
    Str(String),
    /// A number.
    Num(f64),
    /// A boolean.
    Bool(bool),
    /// An action: what a widget does when it is activated (`SCREENS.md §7`).
    ///
    /// An action is a *value* and not only a syntax, which is what lets a screen take one as a
    /// parameter and hand it to a widget. Without that, `confirm(message, yes_action, no_action)`
    /// cannot be written at all: the caller's answer is the content.
    Action(Action),
    /// A sequence, for a `for` to walk (`SCREENS.md §2.4`).
    ///
    /// A list rather than anything cleverer: a screen draws children from data, and the data arrives
    /// from outside it. Nothing *builds* one in a screen body except a literal, which is what makes a
    /// loop testable before the systems that feed it exist.
    List(Vec<Value>),
    /// A record: named fields, so `option.caption` resolves (`SCREENS.md §2.4`).
    ///
    /// Ordered rather than hashed, like everything else that reaches output — two runs of one screen
    /// must walk the same fields in the same order.
    Record(Vec<(String, Value)>),
    /// No value.
    None,
}

impl Value {
    /// The value as text, for `text <name>`.
    #[must_use]
    pub fn as_text(&self) -> String {
        match self {
            Self::Str(text) => text.clone(),
            Self::Bool(true) => "true".to_string(),
            Self::Bool(false) => "false".to_string(),
            // An action as text is the call it was written as. Nothing draws one today, but a screen
            // that interpolates an action should say what it is rather than render as blank.
            Self::Action(action) => action.to_string(),
            // A sequence and a record render as themselves, `[a, b]` and `{caption: "Yes"}`. Nothing
            // draws one on purpose, and the alternative — blank — is a screen that looks like it
            // worked. Elements go through this same function, so a nested value cannot print
            // differently depending on where it sits.
            Self::List(items) => {
                let parts: Vec<String> = items.iter().map(Self::as_text).collect();
                format!("[{}]", parts.join(", "))
            }
            Self::Record(fields) => {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|(name, value)| format!("{name}: {}", value.as_text()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            Self::None => String::new(),
            // Not `{}`: a float's default formatting is locale-adjacent enough that the
            // determinism rules ban it, and a fixed precision is what a screen wants anyway.
            Self::Num(number) if number.fract() == 0.0 => format!("{number:.0}"),
            Self::Num(number) => format!("{number:.2}"),
        }
    }

    /// Whether the value counts as true in a screen condition.
    #[must_use]
    pub fn truthy(&self) -> bool {
        match self {
            Self::None => false,
            Self::Bool(value) => *value,
            Self::Str(text) => !text.is_empty(),
            Self::Num(number) => *number != 0.0,
            // An action is something, so it is true — the same answer every non-`none` value gets.
            Self::Action(_) => true,
            // Empty is false, which is Python's answer and Ren'Py's: `if items:` is how a screen asks
            // whether it was handed anything to draw.
            Self::List(items) => !items.is_empty(),
            Self::Record(fields) => !fields.is_empty(),
        }
    }

    /// The fields of a record, if this value is one.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&Value> {
        let Self::Record(fields) = self else {
            return None;
        };
        fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, value)| value)
    }
}

/// The arguments a screen was called with, by parameter name, in call order.
#[derive(Clone, Debug, Default)]
pub struct Args {
    values: Vec<(String, Value)>,
}

impl Args {
    /// No arguments.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Binds a name, replacing any binding it already has.
    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        self.values.push((name.into(), value));
    }

    /// The value bound to a parameter, if any.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    /// This scope, with `name` bound to `value`.
    ///
    /// A clone with one binding replaced, which is what a loop needs: `for option in items` must
    /// *shadow* an outer `option` rather than be shadowed by it, and the shadow has to end with the
    /// iteration. [`get`](Self::get) answers with the first binding it finds, so a loop that appended
    /// would never be the one read.
    #[must_use]
    pub fn with(&self, name: &str, value: Value) -> Self {
        let mut values: Vec<(String, Value)> = self
            .values
            .iter()
            .filter(|(key, _)| key != name)
            .cloned()
            .collect();
        values.push((name.to_string(), value));
        Self { values }
    }
}
