//! Runtime values.
//!
//! A `Value` is *plain data*: no pointers, no handles, no interior mutability. That is
//! what makes `World` snapshottable and serializable, and it is why entities are
//! generational ids rather than references.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A key in a map value.
///
/// Restricted to types with a total order, because a map is the one aggregate whose
/// iteration order is observable. A hash map would make two runs of the same story
/// differ, which `ARCHITECTURE.md §4` forbids outright — so the restriction is enforced by
/// the type rather than by remembering.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Key {
    /// An integer key.
    Int(i64),
    /// A string key.
    Str(String),
    /// A boolean key.
    Bool(bool),
}

impl Key {
    /// The key a value can serve as, if it can.
    #[must_use]
    pub fn of(value: &Value) -> Option<Self> {
        match value {
            Value::Int(number) => Some(Self::Int(*number)),
            Value::Str(text) => Some(Self::Str(text.clone())),
            Value::Bool(flag) => Some(Self::Bool(*flag)),
            _ => None,
        }
    }
}

/// A runtime value.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum Value {
    /// `none`
    None,
    /// A boolean.
    Bool(bool),
    /// A 64-bit signed integer.
    Int(i64),
    /// A 64-bit float.
    Float(f64),
    /// A string.
    Str(String),
    /// A list.
    List(Vec<Value>),
    /// A map, ordered so that iteration is reproducible.
    ///
    /// Written as a list of pairs rather than a JSON object: a `Key` is typed, and a JSON
    /// object's keys must be strings, so an object would have to stringify them and a
    /// round-trip would have to guess the type back.
    Map(#[serde(with = "map_pairs")] BTreeMap<Key, Value>),
    /// An instance of a declared struct.
    Struct {
        /// The struct's name.
        name: String,
        /// Its fields, in declaration order.
        fields: Vec<(String, Value)>,
    },
    /// A function used as a value: a declared `fn`, or a lifted lambda.
    ///
    /// A name rather than a pointer, so that it stays plain data and so that a value can
    /// never reach into code the module does not contain.
    Function(String),
    /// An instance of a declared enum variant.
    Enum {
        /// The enum's name.
        name: String,
        /// The variant's name.
        variant: String,
        /// The variant's payload, in declaration order.
        fields: Vec<Value>,
    },
}

impl Value {
    /// The name of the value's type, as an author would write it.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Str(_) => "str",
            Self::List(_) => "list",
            Self::Map(_) => "map",
            Self::Struct { .. } => "struct",
            Self::Enum { .. } => "enum",
            Self::Function(_) => "fn",
        }
    }

    /// The value's truth, when it has one.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(flag) => Some(*flag),
            _ => None,
        }
    }

    /// The variant's tag, when this is an enum value.
    #[must_use]
    pub fn variant(&self) -> Option<&str> {
        match self {
            Self::Enum { variant, .. } => Some(variant),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    /// Renders a value the way `str(…)` would.
    ///
    /// Float formatting is pinned rather than left to `{}`, because `1.0` printing as `1`
    /// would make a score read as an integer in a story, and because a value's rendering
    /// is part of what a test asserts on.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => f.write_str("none"),
            Self::Bool(flag) => write!(f, "{flag}"),
            Self::Int(number) => write!(f, "{number}"),
            Self::Float(number) => f.write_str(&format_float(*number)),
            Self::Str(text) => f.write_str(text),
            Self::List(items) => {
                f.write_str("[")?;
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            Self::Map(entries) => {
                f.write_str("{")?;
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{key:?}: {value}")?;
                }
                f.write_str("}")
            }
            Self::Function(name) => write!(f, "<fn {name}>"),
            Self::Struct { name, fields } => {
                write!(f, "{name}(")?;
                for (index, (field, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{field}: {value}")?;
                }
                f.write_str(")")
            }
            Self::Enum {
                variant, fields, ..
            } => {
                write!(f, "{variant}")?;
                if fields.is_empty() {
                    return Ok(());
                }
                f.write_str("(")?;
                for (index, value) in fields.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{value}")?;
                }
                f.write_str(")")
            }
        }
    }
}

/// Serializes a value's map as a list of `[key, value]` pairs.
///
/// The `BTreeMap`'s order is total, so the pairs — and the bytes — are reproducible.
mod map_pairs {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::{Key, Value};

    /// Writes the map as an array of pairs.
    pub(super) fn serialize<S: Serializer>(
        map: &BTreeMap<Key, Value>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        map.iter()
            .collect::<Vec<(&Key, &Value)>>()
            .serialize(serializer)
    }

    /// Reads the array of pairs back into a map.
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<Key, Value>, D::Error> {
        Ok(Vec::<(Key, Value)>::deserialize(deserializer)?
            .into_iter()
            .collect())
    }
}

/// Formats a float, keeping it visibly a float.
///
/// `1.0_f64` formats as `1` with the default display, which would make an arithmetic
/// result read as an integer in dialogue. A value that is integral gets a trailing `.0`
/// so that `str(1.0)` is `"1.0"` and not `"1"`.
#[must_use]
pub fn format_float(number: f64) -> String {
    if number.is_nan() {
        return "nan".to_string();
    }
    if number.is_infinite() {
        return if number > 0.0 { "inf" } else { "-inf" }.to_string();
    }

    let rendered = format!("{number}");
    if rendered.contains(['.', 'e', 'E']) {
        rendered
    } else {
        format!("{rendered}.0")
    }
}
