//! Types as written in source.
//!
//! This is *syntax*, not the type system: `list<Foo>` here is just a name with an
//! argument. Resolving what `Foo` means, and whether `list<Foo>` is well-formed, belongs
//! to `vela-types`.

use vela_span::Span;

/// A type expression.
#[derive(Clone, Debug)]
pub enum Type {
    /// A named type, possibly dotted: `int`, `chapters.Route`.
    Named {
        /// The type's span.
        span: Span,
        /// The dotted segments.
        path: Vec<String>,
    },
    /// `T?`.
    Optional {
        /// The span, including the `?`.
        span: Span,
        /// The type being made optional.
        inner: Box<Type>,
    },
    /// `list<T>`.
    List {
        /// The span, including the arguments.
        span: Span,
        /// The element type.
        element: Box<Type>,
    },
    /// `map<K, V>`.
    Map {
        /// The span, including the arguments.
        span: Span,
        /// The key type.
        key: Box<Type>,
        /// The value type.
        value: Box<Type>,
    },
    /// A parenthesised list, i.e. a tuple type.
    Tuple {
        /// The span, including the parentheses.
        span: Span,
        /// The element types.
        elements: Vec<Type>,
    },
    /// A span where a type should have been.
    Error {
        /// The offending span.
        span: Span,
    },
}

impl Type {
    /// The type's span.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Named { span, .. }
            | Self::Optional { span, .. }
            | Self::List { span, .. }
            | Self::Map { span, .. }
            | Self::Tuple { span, .. }
            | Self::Error { span } => *span,
        }
    }
}
