//! Statements: the straight-line work inside a block.

use vela_span::Span;
use vela_syntax::{BinOp, UnOp};
use vela_world::CommandKind;

use crate::ir::defs::{ConstId, DefaultId, Slot};

/// A slot, or a constant.
///
/// Every `Value` has a known type at construction, which is what lets codegen emit typed
/// instructions without a second inference pass.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Value {
    /// A local slot.
    Slot(Slot),
    /// An entry in the constant pool.
    Const(ConstId),
}

impl Value {
    /// The slot, if this is one.
    #[must_use]
    pub fn slot(self) -> Option<Slot> {
        match self {
            Self::Slot(slot) => Some(slot),
            Self::Const(_) => None,
        }
    }
}

/// Somewhere a value can be written.
///
/// A *place*, not a value: `route.name` and `xs[0]` name a location rather than computing
/// one, and the distinction is what makes `xs[0] = 5` expressible without inventing
/// references.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Place {
    /// A local slot.
    Local(Slot),
    /// A `default`, which is world state and therefore part of the save schema.
    Default(DefaultId),
    /// A field of a struct held in another place.
    ///
    /// The field is named rather than indexed. MIR is the stable, human-readable contract
    /// — a name survives a struct gaining a field in the middle, and a disassembly that
    /// says `route.name` is worth more than one that says `route.3`. Resolving to an index
    /// is codegen's job, where the instruction set demands one.
    Field {
        /// The struct's location.
        base: Box<Self>,
        /// Which field, by name.
        field: String,
    },
    /// An element of a list, or a value under a map key.
    Index {
        /// The aggregate's location.
        base: Box<Self>,
        /// The subscript, which is a runtime value.
        index: Value,
    },
}

impl Place {
    /// The slot at the root of this place, if it has one.
    ///
    /// Every place bottoms out in a slot or a default; this answers which, so that passes
    /// can ask whether a place touches world state without walking the chain themselves.
    #[must_use]
    pub fn root(&self) -> Root {
        match self {
            Self::Local(slot) => Root::Local(*slot),
            Self::Default(id) => Root::Default(*id),
            Self::Field { base, .. } | Self::Index { base, .. } => base.root(),
        }
    }
}

/// What a [`Place`] ultimately names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Root {
    /// A local slot.
    Local(Slot),
    /// A `default`.
    Default(DefaultId),
}

/// Where a read takes its value from.
///
/// Distinct from [`Place`] because a read cannot be a bare location: `x` reads a slot, but
/// `route.name` and `xs[i]` *compute* a value from an aggregate.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Operand {
    /// A slot or a constant.
    Value(Value),
    /// A `default`. A place rather than a value, so reading one is its own operand.
    Default(DefaultId),
    /// A field of an aggregate.
    Field {
        /// The aggregate.
        base: Value,
        /// Which field, by name.
        field: String,
    },
    /// An element of a list, or a value under a map key.
    Index {
        /// The aggregate.
        base: Value,
        /// The subscript.
        index: Value,
    },
    /// How many elements an aggregate holds.
    ///
    /// A read rather than a statement because it computes a value, and a `for` loop is
    /// what needs it: the language has no `len(…)`, so an index loop is the lowering, and
    /// an index loop has to know where to stop.
    Len {
        /// The aggregate.
        base: Value,
    },
}

impl Operand {
    /// The operand's simplest form, if it is one.
    #[must_use]
    pub fn as_value(self) -> Option<Value> {
        match self {
            Self::Value(value) => Some(value),
            Self::Default(_) | Self::Field { .. } | Self::Index { .. } | Self::Len { .. } => None,
        }
    }
}

/// A conversion the language provides.
///
/// Built in rather than declared, because they are the *only* names the language reserves
/// for functions (`LANGUAGE.md §5.7`) and because a conversion has to be available before
/// any module is loaded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Builtin {
    /// `str(x)`
    Str,
    /// `int(x)`
    Int,
    /// `float(x)`
    Float,
    /// `bool(x)`
    Bool,
}

impl Builtin {
    /// The name, as written.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Str => "str",
            Self::Int => "int",
            Self::Float => "float",
            Self::Bool => "bool",
        }
    }

    /// The conversion a name refers to, if it is one.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "str" => Some(Self::Str),
            "int" => Some(Self::Int),
            "float" => Some(Self::Float),
            "bool" => Some(Self::Bool),
            _ => None,
        }
    }
}

/// A function that can be called.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FuncRef {
    /// A body in this module, by index into [`crate::ir::Module::fns`].
    Defined(u32),
    /// A conversion the language provides.
    Builtin(Builtin),
    /// A host capability, by index into [`crate::ir::Module::effects`].
    ///
    /// Separate from [`Self::Defined`] because it is the one call that leaves the program:
    /// it needs a capability, its result is recorded in the input log, and the runtime
    /// provides it rather than the module.
    Effect(u32),
}

/// What is being called.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Callee {
    /// A call target known at compile time.
    Direct(FuncRef),
    /// A target computed at run time, which is what calling a lambda held in a variable
    /// requires.
    Indirect(Value),
}

/// A statement.
#[derive(Clone, Debug)]
pub struct Stmt {
    /// What the statement does.
    pub kind: StmtKind,
    /// Where it came from, for the debugger and for a verifier report.
    pub span: Span,
}

/// What a statement does.
#[derive(Clone, Debug)]
pub enum StmtKind {
    /// `dst = a op b`
    Assign {
        /// Where the result goes.
        dst: Place,
        /// The operator.
        op: BinOp,
        /// The left operand.
        a: Value,
        /// The right operand.
        b: Value,
    },
    /// `dst = op a`
    AssignUn {
        /// Where the result goes.
        dst: Place,
        /// The operator.
        op: UnOp,
        /// The operand.
        a: Value,
    },
    /// `dst = src`
    Load {
        /// Where the value goes.
        dst: Place,
        /// Where it comes from.
        src: Operand,
    },
    /// `dst = callee(args)`, or the call for its effect when `dst` is absent.
    Call {
        /// Where the result goes, if it is kept.
        dst: Option<Place>,
        /// What is being called.
        callee: Callee,
        /// The arguments.
        args: Vec<Value>,
    },
    /// Builds a presentation command, which the next `Yield` hands to the host.
    ///
    /// The arguments are untyped slots and constants rather than a structured `Command`,
    /// because a command is a *runtime* value and this is the site that assembles it.
    Cmd {
        /// Which command is being built.
        kind: CommandKind,
        /// Its arguments, in the order the schema names them.
        args: Vec<Value>,
    },
    /// `dst = [items]`
    ListNew {
        /// Where the list goes.
        dst: Place,
        /// Its elements.
        items: Vec<Value>,
    },
    /// `dst = {key: value, …}`
    MapNew {
        /// Where the map goes.
        dst: Place,
        /// Its entries, in source order.
        entries: Vec<(Value, Value)>,
    },
    /// `dst = Name { field: value, … }`
    StructNew {
        /// Where the struct goes.
        dst: Place,
        /// Which struct.
        name: String,
        /// Its fields, in declaration order.
        fields: Vec<(String, Value)>,
    },
    /// `dst = Enum.Variant(args)`
    EnumNew {
        /// Where the value goes.
        dst: Place,
        /// Which enum.
        enum_name: String,
        /// Which variant.
        variant: String,
        /// The payload.
        args: Vec<Value>,
    },
    /// `dst = base.0` — the payload of a matched variant, by position.
    ///
    /// A statement rather than an [`Operand`], because reading a payload is only valid
    /// where the variant is already known, which a `Dispatch` has narrowed to.
    EnumField {
        /// Where the payload goes.
        dst: Place,
        /// The enum value.
        base: Value,
        /// Which payload position.
        index: u32,
    },
    /// `dst = base is none`
    IsNone {
        /// Where the answer goes.
        dst: Place,
        /// The optional.
        base: Value,
    },
    /// `dst = base`, asserting it has a payload.
    ///
    /// Reaching this with `none` is a fault rather than a value: `??` is how an absent
    /// payload is handled, and lowering only emits `Unwrap` on the branch where `IsNone`
    /// has already said there is one.
    Unwrap {
        /// Where the payload goes.
        dst: Place,
        /// The optional.
        base: Value,
    },
}
