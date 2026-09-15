//! Types.

use std::fmt;

/// A type.
#[derive(Clone, PartialEq, Debug)]
pub enum Ty {
    /// `int`
    Int,
    /// `float`
    Float,
    /// `bool`
    Bool,
    /// `str`
    Str,
    /// The type of an expression that produces nothing, such as a call to a `fn` with no
    /// return type.
    Unit,
    /// The type of the `none` literal.
    ///
    /// Distinct from [`Ty::Unit`]: `none` is a value, and it fits only an optional.
    None,
    /// `T?`
    Optional(Box<Ty>),
    /// `list<T>`
    List(Box<Ty>),
    /// `map<K, V>`
    Map(Box<Ty>, Box<Ty>),
    /// A declared `struct`.
    Struct(String),
    /// A declared `enum`.
    Enum(String),
    /// A function's signature.
    Fn(Vec<Ty>, Box<Ty>),
    /// A type that could not be worked out.
    ///
    /// Unknown unifies with everything and is never reported against. That is the whole
    /// point: one untypeable expression should produce one diagnostic, not one for every
    /// place its result is then used.
    Unknown,
}

impl Ty {
    /// Whether a value of type `other` fits where this type is expected.
    ///
    /// Deliberately no widening: `int` is not a `float` (`LANGUAGE.md §5.7`), because a
    /// story that silently turns an affection score into `3.0` produces a display bug
    /// that no one can explain.
    #[must_use]
    pub fn accepts(&self, other: &Self) -> bool {
        if matches!(self, Self::Unknown) || matches!(other, Self::Unknown) {
            return true;
        }
        if self == other {
            return true;
        }

        match self {
            // `none`, and the payload, both fit an optional.
            Self::Optional(inner) => matches!(other, Self::None) || inner.accepts(other),
            Self::List(element) => match other {
                Self::List(other_element) => element.accepts(other_element),
                _ => false,
            },
            Self::Map(key, value) => match other {
                Self::Map(other_key, other_value) => {
                    key.accepts(other_key) && value.accepts(other_value)
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Whether this is an optional, which is what an unwrap-shaped use needs to know.
    #[must_use]
    pub fn is_optional(&self) -> bool {
        matches!(self, Self::Optional(_))
    }

    /// Whether this is a number, for arithmetic.
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(self, Self::Int | Self::Float)
    }

    /// Whether a value of this type can appear in a `{...}` interpolation.
    ///
    /// A declared type cannot, because its rendering would be arbitrary; `LANGUAGE.md §5.5`
    /// requires an explicit `str(…)` so that formatting is never accidental.
    #[must_use]
    pub fn is_displayable(&self) -> bool {
        matches!(
            self,
            Self::Int | Self::Float | Self::Bool | Self::Str | Self::Unknown
        )
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int => f.write_str("int"),
            Self::Float => f.write_str("float"),
            Self::Bool => f.write_str("bool"),
            Self::Str => f.write_str("str"),
            Self::Unit => f.write_str("nothing"),
            Self::None => f.write_str("none"),
            Self::Optional(inner) => write!(f, "{inner}?"),
            Self::List(element) => write!(f, "list<{element}>"),
            Self::Map(key, value) => write!(f, "map<{key}, {value}>"),
            Self::Struct(name) | Self::Enum(name) => f.write_str(name),
            Self::Fn(params, ret) => {
                let params: Vec<String> = params.iter().map(ToString::to_string).collect();
                write!(f, "fn({}) -> {ret}", params.join(", "))
            }
            Self::Unknown => f.write_str("?"),
        }
    }
}
