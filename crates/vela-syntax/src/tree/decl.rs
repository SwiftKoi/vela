//! Top-level declarations.

use vela_span::Span;

use crate::tree::{Expr, ScreenLine, Stmt, TestDecl, Type};

/// One parameter of a function, screen, lambda, or enum variant.
#[derive(Clone, Debug)]
pub struct Param {
    /// The parameter's span.
    pub span: Span,
    /// Its name.
    pub name: String,
    /// Its type, if one was written.
    ///
    /// A type may be omitted, and most screen parameters are: Ren'Py's screens rarely type theirs,
    /// and the migration has to accept what a project wrote rather than invent a type for it. The
    /// checker reads an absent type as `Ty::Unknown` — its existing answer for "nobody said", which
    /// fits everywhere and is never reported against (`LANGUAGE.md §5.4`).
    pub ty: Option<Type>,
    /// A default, if one was written.
    pub default: Option<Expr>,
}

/// An `effect`: a capability the host provides.
///
/// There is no body, and that is the point: an effect is not implemented in the language, it
/// is *requested* from outside it. `RUNTIME.md §3` decides who implements one — production
/// injects a real host, tests inject mocks, plugins receive a restricted subset — and this
/// declaration is how a script says what it needs.
#[derive(Debug)]
pub struct EffectDecl {
    /// The declaration's span.
    pub span: Span,
    /// The dotted name, e.g. `rand.int`. Dotted because effects are grouped by capability:
    /// `rand.int`, `rand.float`, `rand.pick`.
    pub path: Vec<String>,
    /// The parameters.
    pub params: Vec<Param>,
    /// The type it returns, if it returns one.
    pub ret: Option<Type>,
}

impl EffectDecl {
    /// The name as written.
    #[must_use]
    pub fn dotted(&self) -> String {
        self.path.join(".")
    }
}

/// A `use` declaration.
#[derive(Debug)]
pub struct UseDecl {
    /// The declaration's span.
    pub span: Span,
    /// The module path.
    pub path: Vec<String>,
    /// A local alias, from `as`.
    pub alias: Option<String>,
}

/// A `const` declaration.
#[derive(Debug)]
pub struct ConstDecl {
    /// The declaration's span.
    pub span: Span,
    /// The constant's name.
    pub name: String,
    /// A declared type, if one was written.
    pub ty: Option<Type>,
    /// Its value.
    pub value: Expr,
}

/// A `default` declaration: World state that is persisted in saves.
#[derive(Debug)]
pub struct DefaultDecl {
    /// The declaration's span.
    pub span: Span,
    /// The name.
    pub name: String,
    /// A declared type, if one was written.
    pub ty: Option<Type>,
    /// The initial value.
    pub value: Expr,
}

/// One field of a `struct`.
#[derive(Debug)]
pub struct StructField {
    /// The field's span.
    pub span: Span,
    /// Its name.
    pub name: String,
    /// Its type.
    pub ty: Type,
    /// A default, if one was written.
    pub default: Option<Expr>,
}

/// A `struct` declaration.
#[derive(Debug)]
pub struct StructDecl {
    /// The declaration's span.
    pub span: Span,
    /// The struct's name.
    pub name: String,
    /// Its fields.
    pub fields: Vec<StructField>,
}

/// One variant of an `enum`.
#[derive(Debug)]
pub struct Variant {
    /// The variant's span.
    pub span: Span,
    /// Its name.
    pub name: String,
    /// Its payload fields.
    pub fields: Vec<Param>,
}

/// An `enum` declaration.
#[derive(Debug)]
pub struct EnumDecl {
    /// The declaration's span.
    pub span: Span,
    /// The enum's name.
    pub name: String,
    /// Its variants.
    pub variants: Vec<Variant>,
}

/// A `key = value` setting, used by characters, styles, and themes.
#[derive(Clone, Debug)]
pub struct Setting {
    /// The setting's span.
    pub span: Span,
    /// The type word, when one is written: `color` in `color bg = 0x10121a`.
    ///
    /// Not decoration. It is what tells a reader — and a palette — that `bg` is a colour
    /// while `sm` in `space sm = 4` is a length, and without it the two are the same kind of
    /// value with the same kind of number in them.
    pub ty: Option<String>,
    /// The key.
    pub key: String,
    /// The value.
    pub value: Expr,
}

/// A `character` declaration.
#[derive(Debug)]
pub struct CharacterDecl {
    /// The declaration's span.
    pub span: Span,
    /// The character's name, as used in `say` statements.
    pub name: String,
    /// Its settings.
    pub settings: Vec<Setting>,
}

/// An `image` declaration.
#[derive(Debug)]
pub struct ImageDecl {
    /// The declaration's span.
    pub span: Span,
    /// The image's dotted name.
    pub name: Vec<String>,
    /// What it resolves to.
    pub value: Expr,
}

/// A `transform` declaration.
///
/// The body is an animation, which is not parsed until M13. It is consumed and recorded
/// as a span so that a file containing transforms still parses today.
#[derive(Debug)]
pub struct TransformDecl {
    /// The declaration's span.
    pub span: Span,
    /// The transform's name.
    pub name: String,
    /// The body, unparsed.
    pub body: Span,
}

/// A `screen` declaration.
///
/// The body is a widget tree, which is not parsed until M7. It is consumed and recorded
/// as a span so that a file containing screens still parses today.
#[derive(Clone, Debug)]
pub struct ScreenDecl {
    /// The declaration's span.
    pub span: Span,
    /// The screen's name.
    pub name: String,
    /// Its parameters.
    pub params: Vec<Param>,
    /// The widget tree.
    pub body: Vec<ScreenLine>,
}

/// A `style` declaration.
#[derive(Clone, Debug)]
pub struct StyleDecl {
    /// The declaration's span.
    pub span: Span,
    /// The style's name.
    pub name: String,
    /// A style to inherit from, from the `from` clause.
    pub from: Option<String>,
    /// Its settings.
    pub settings: Vec<Setting>,
}

/// A `theme` declaration.
#[derive(Clone, Debug)]
pub struct ThemeDecl {
    /// The declaration's span.
    pub span: Span,
    /// The theme's name.
    pub name: String,
    /// Its settings.
    pub settings: Vec<Setting>,
}

/// A `fn` declaration.
#[derive(Debug)]
pub struct FnDecl {
    /// The declaration's span.
    pub span: Span,
    /// The function's name.
    pub name: String,
    /// Its parameters.
    pub params: Vec<Param>,
    /// Its return type, if declared.
    pub ret: Option<Type>,
    /// Its body.
    pub body: Vec<Stmt>,
}

/// A `label` declaration, an entry point for story control flow.
#[derive(Debug)]
pub struct LabelDecl {
    /// The declaration's span.
    pub span: Span,
    /// The label's name.
    pub name: String,
    /// Its body.
    pub body: Vec<Stmt>,
}

/// A top-level item.
#[derive(Debug)]
pub enum Item {
    /// `use`
    Use(UseDecl),
    /// `effect`
    Effect(EffectDecl),
    /// `const`
    Const(ConstDecl),
    /// `default`
    Default(DefaultDecl),
    /// `struct`
    Struct(StructDecl),
    /// `enum`
    Enum(EnumDecl),
    /// `character`
    Character(CharacterDecl),
    /// `image`
    Image(ImageDecl),
    /// `transform`
    Transform(TransformDecl),
    /// `screen`
    Screen(ScreenDecl),
    /// `style`
    Style(StyleDecl),
    /// `test`
    Test(TestDecl),
    /// `theme`
    Theme(ThemeDecl),
    /// `fn`
    Function(FnDecl),
    /// `label`
    Label(LabelDecl),
    /// A span where an item should have been.
    Error {
        /// The offending span.
        span: Span,
    },
}

impl Item {
    /// The item's span.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Use(d) => d.span,
            Self::Effect(d) => d.span,
            Self::Const(d) => d.span,
            Self::Default(d) => d.span,
            Self::Struct(d) => d.span,
            Self::Enum(d) => d.span,
            Self::Character(d) => d.span,
            Self::Image(d) => d.span,
            Self::Transform(d) => d.span,
            Self::Screen(d) => d.span,
            Self::Style(d) => d.span,
            Self::Test(d) => d.span,
            Self::Theme(d) => d.span,
            Self::Function(d) => d.span,
            Self::Label(d) => d.span,
            Self::Error { span } => *span,
        }
    }
}
