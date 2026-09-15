//! The instruction set, as data.
//!
//! `BYTECODE.md §3.2` calls this table **normative**, and the reason is structural: the
//! interpreter and the verifier both read it, so they cannot disagree about what an
//! instruction means. A match statement in each of them would agree today and drift the
//! first time one was edited — and the drift would be a program that verifies and then
//! does something else.
//!
//! Adding an instruction touches this file and one handler. That is the deliberate
//! exception in `ARCHITECTURE.md §5`, and it is why the table is data rather than code.

/// One instruction's shape.
#[derive(Clone, Copy, Debug)]
pub struct OpSpec {
    /// The opcode, which is also the instruction's identity in the stream.
    pub op: Op,
    /// The mnemonic, as the disassembler prints it.
    pub name: &'static str,
    /// What the instruction does to the stack, for the verifier and for a reader.
    pub effect: Effect,
}

/// How an instruction changes the stack.
///
/// Deliberately coarse: `Pops`/`Pushes` count values, and the *types* are checked by the
/// verifier against the type table. Expressing full stack types here would duplicate the
/// type table in a second place that could disagree with it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Effect {
    /// Pops nothing, pushes nothing.
    Nothing,
    /// Pops nothing, pushes one value.
    Push,
    /// Pops nothing, pushes `n` values, where `n` is the instruction's operand.
    PushN,
    /// Pops one value, pushes nothing.
    Pop,
    /// Pops one value, pushes one.
    Replace,
    /// Pops two values, pushes one — arithmetic and comparison.
    Binary,
    /// Pops `n` values and pushes one — a call, a command, a concatenation.
    CallN,
    /// Pops one aggregate and one key, pushes the element or a flag.
    Index,
    /// Pops two values, pushes nothing — writing a struct's field.
    Set,
    /// Pops three values and pushes the updated aggregate.
    ///
    /// Aggregates are values, not references, so `xs[0] = v` has to produce a new list for
    /// the compiler to store back. An in-place update would need the runtime to know where
    /// the aggregate came from, which is exactly the aliasing a story engine should not
    /// have.
    Update,
    /// Suspends and receives one value in place of the command it popped.
    Yield,
    /// Leaves the function, popping `n` values (its result arity).
    Return,
    /// Transfers control and does not come back.
    Jump,
    /// Pops the variant's payload, which its schema in the enum table gives, and pushes one.
    Variant,
}

/// An opcode.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(u8)]
pub enum Op {
    /// A 64-bit integer constant.
    ConstI = 0,
    /// A 64-bit float constant.
    ConstF,
    /// A string constant, by string-table index.
    ConstS,
    /// A boolean constant.
    ConstB,
    /// `none`.
    ConstNone,
    /// A function used as a value, by index into `Module::fns`.
    ///
    /// **Added in M4.** `LANGUAGE.md §3` has lambda literals; without this a lifted lambda
    /// could be referenced by nothing.
    ConstFn,

    /// Push a local.
    LoadLocal,
    /// Pop into a local.
    StoreLocal,
    /// Push a `default` — world state.
    LoadDefault,
    /// Pop into a `default`.
    StoreDefault,

    /// Build a list from `n` values.
    ListNew,
    /// Index a list or a map.
    ListGet,
    /// Write an element.
    ListSet,
    /// The length of an aggregate.
    ListLen,
    /// Build a map from `n` key-value pairs.
    MapNew,
    /// Insert into a map.
    MapSet,
    /// Look a key up in a map.
    MapGet,
    /// Whether a map holds a key.
    MapHas,
    /// Build a struct.
    StructNew,
    /// Read a struct's field, by index.
    FieldGet,
    /// Write a struct's field.
    FieldSet,
    /// Build an enum variant.
    EnumNew,
    /// The tag of an enum value.
    EnumTag,
    /// Read a variant's payload, by position.
    EnumField,

    /// Integer addition.
    AddI,
    /// Integer subtraction.
    SubI,
    /// Integer multiplication.
    MulI,
    /// Integer division.
    DivI,
    /// Integer remainder.
    ModI,
    /// Integer negation.
    NegI,
    /// Float addition.
    AddF,
    /// Float subtraction.
    SubF,
    /// Float multiplication.
    MulF,
    /// Float division.
    DivF,
    /// Float negation.
    NegF,

    /// Integer equality.
    EqI,
    /// Float equality.
    EqF,
    /// String equality.
    EqS,
    /// Boolean equality.
    EqB,
    /// Integer less-than.
    LtI,
    /// Float less-than.
    LtF,
    /// Integer less-or-equal.
    LeI,
    /// Float less-or-equal.
    LeF,
    /// Integer greater-than.
    GtI,
    /// Float greater-than.
    GtF,
    /// Integer greater-or-equal.
    GeI,
    /// Float greater-or-equal.
    GeF,
    /// String less-than.
    ///
    /// **Added in M4.** The table had `EqS` and no ordering for strings, but the checker
    /// accepts `"a" < "b"` — so a story could write one and nothing could compile it.
    LtS,
    /// String less-or-equal.
    LeS,
    /// String greater-than.
    GtS,
    /// String greater-or-equal.
    GeS,

    /// Boolean negation.
    Not,

    /// Unconditional jump, by byte offset from this instruction.
    Jump,
    /// Jump when the top of the stack is false.
    JumpIfFalse,
    /// Jump when the top of the stack is true.
    JumpIfTrue,
    /// Leave the function with `n` results.
    Return,
    /// Call a function by index.
    CallFn,
    /// Call the function value on the stack, with `n` arguments below it.
    ///
    /// **Added in M4.** `LANGUAGE.md §3` has lambda literals, and MIR has an indirect
    /// `Callee`; a table without this could compile a lambda and never call one.
    CallValue,
    /// Enter a label, resuming after this instruction when it returns.
    CallLabel,

    /// Jump through a table indexed by an enum's tag.
    Dispatch,

    /// Whether an optional holds a value.
    IsNone,
    /// The payload of an optional that has one, or a fallback.
    UnwrapOr,
    /// The payload of an optional the compiler has proven has one.
    ///
    /// **Added in M4.** `BYTECODE.md §3.2` listed `UnwrapOr` and nothing that *asserts*,
    /// but `??` lowering branches on `IsNone` and then needs the payload without a
    /// fallback — because the fallback has already been placed on the other branch, and
    /// evaluating it here is exactly what short-circuiting exists to avoid.
    Unwrap,

    /// Concatenate `n` strings.
    Concat,
    /// Render a value as text.
    ToStr,
    /// Parse text as an integer.
    ToInt,
    /// Parse text as a float.
    ToFloat,
    /// Turn a value into a boolean.
    ///
    /// **Added in M4.** The checker accepts `bool(x)`; the table listed `ToStr`, `ToInt`
    /// and `ToFloat` and not this one.
    ToBool,

    /// Build a presentation command: variant, then `n` arguments.
    Cmd,
    /// Call a host effect: id, then `n` arguments.
    CallEffect,
    /// Suspend and hand the host the command on the stack.
    Yield,

    /// Duplicate the top of the stack.
    Dup,
    /// Discard the top of the stack.
    Pop,
    /// Do nothing.
    Nop,
}

/// Every instruction, with its mnemonic and stack effect.
///
/// Ordered to match the declaration above, which a test asserts — so a new op cannot be
/// added to the enum and forgotten here without failing.
pub const OPS: &[OpSpec] = &[
    spec(Op::ConstI, "const.i", Effect::Push),
    spec(Op::ConstF, "const.f", Effect::Push),
    spec(Op::ConstS, "const.s", Effect::Push),
    spec(Op::ConstB, "const.b", Effect::Push),
    spec(Op::ConstNone, "const.none", Effect::Push),
    spec(Op::ConstFn, "const.fn", Effect::Push),
    spec(Op::LoadLocal, "load.local", Effect::Push),
    spec(Op::StoreLocal, "store.local", Effect::Pop),
    spec(Op::LoadDefault, "load.default", Effect::Push),
    spec(Op::StoreDefault, "store.default", Effect::Pop),
    spec(Op::ListNew, "list.new", Effect::CallN),
    spec(Op::ListGet, "list.get", Effect::Index),
    spec(Op::ListSet, "list.set", Effect::Update),
    spec(Op::ListLen, "list.len", Effect::Replace),
    spec(Op::MapNew, "map.new", Effect::CallN),
    spec(Op::MapSet, "map.set", Effect::Update),
    spec(Op::MapGet, "map.get", Effect::Index),
    spec(Op::MapHas, "map.has", Effect::Index),
    spec(Op::StructNew, "struct.new", Effect::CallN),
    spec(Op::FieldGet, "field.get", Effect::Replace),
    spec(Op::FieldSet, "field.set", Effect::Binary),
    spec(Op::EnumNew, "enum.new", Effect::Variant),
    spec(Op::EnumTag, "enum.tag", Effect::Replace),
    spec(Op::EnumField, "enum.field", Effect::Replace),
    spec(Op::AddI, "add.i", Effect::Binary),
    spec(Op::SubI, "sub.i", Effect::Binary),
    spec(Op::MulI, "mul.i", Effect::Binary),
    spec(Op::DivI, "div.i", Effect::Binary),
    spec(Op::ModI, "mod.i", Effect::Binary),
    spec(Op::NegI, "neg.i", Effect::Replace),
    spec(Op::AddF, "add.f", Effect::Binary),
    spec(Op::SubF, "sub.f", Effect::Binary),
    spec(Op::MulF, "mul.f", Effect::Binary),
    spec(Op::DivF, "div.f", Effect::Binary),
    spec(Op::NegF, "neg.f", Effect::Replace),
    spec(Op::EqI, "eq.i", Effect::Binary),
    spec(Op::EqF, "eq.f", Effect::Binary),
    spec(Op::EqS, "eq.s", Effect::Binary),
    spec(Op::EqB, "eq.b", Effect::Binary),
    spec(Op::LtI, "lt.i", Effect::Binary),
    spec(Op::LtF, "lt.f", Effect::Binary),
    spec(Op::LeI, "le.i", Effect::Binary),
    spec(Op::LeF, "le.f", Effect::Binary),
    spec(Op::GtI, "gt.i", Effect::Binary),
    spec(Op::GtF, "gt.f", Effect::Binary),
    spec(Op::GeI, "ge.i", Effect::Binary),
    spec(Op::GeF, "ge.f", Effect::Binary),
    spec(Op::LtS, "lt.s", Effect::Binary),
    spec(Op::LeS, "le.s", Effect::Binary),
    spec(Op::GtS, "gt.s", Effect::Binary),
    spec(Op::GeS, "ge.s", Effect::Binary),
    spec(Op::Not, "not", Effect::Replace),
    spec(Op::Jump, "jump", Effect::Jump),
    spec(Op::JumpIfFalse, "jump.if.false", Effect::Pop),
    spec(Op::JumpIfTrue, "jump.if.true", Effect::Pop),
    spec(Op::Return, "return", Effect::Return),
    spec(Op::CallFn, "call.fn", Effect::CallN),
    spec(Op::CallValue, "call.value", Effect::CallN),
    spec(Op::CallLabel, "call.label", Effect::Jump),
    spec(Op::Dispatch, "dispatch", Effect::Pop),
    spec(Op::IsNone, "is.none", Effect::Replace),
    spec(Op::UnwrapOr, "unwrap.or", Effect::Binary),
    spec(Op::Unwrap, "unwrap", Effect::Replace),
    spec(Op::Concat, "concat", Effect::CallN),
    spec(Op::ToStr, "to.str", Effect::Replace),
    spec(Op::ToInt, "to.int", Effect::Replace),
    spec(Op::ToFloat, "to.float", Effect::Replace),
    spec(Op::ToBool, "to.bool", Effect::Replace),
    spec(Op::Cmd, "cmd", Effect::CallN),
    spec(Op::CallEffect, "call.effect", Effect::CallN),
    spec(Op::Yield, "yield", Effect::Yield),
    spec(Op::Dup, "dup", Effect::Push),
    spec(Op::Pop, "pop", Effect::Pop),
    spec(Op::Nop, "nop", Effect::Nothing),
];

/// Shorthand for the table above.
const fn spec(op: Op, name: &'static str, effect: Effect) -> OpSpec {
    OpSpec { op, name, effect }
}

impl Op {
    /// The instruction's specification.
    #[must_use]
    pub fn spec(self) -> &'static OpSpec {
        // The table is exhaustive by construction, and a test asserts it. Indexing by the
        // discriminant is what makes decoding O(1) rather than a search.
        &OPS[self as usize]
    }

    /// The mnemonic.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.spec().name
    }

    /// The instruction with this opcode, if there is one.
    #[must_use]
    pub fn from_byte(byte: u8) -> Option<Self> {
        OPS.get(byte as usize).map(|spec| spec.op)
    }
}

/// The shape of an instruction's operands.
///
/// `BYTECODE.md §3.1` fixes the encoding as "opcode + operand kind" and does not enumerate
/// the kinds. These are them: the *shape*, not the meaning, which is what lets a decoder
/// read an instruction without a table lookup and what makes a corrupted opcode a rejected
/// module rather than a misread stream.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum OperandKind {
    /// Nothing follows.
    None = 0,
    /// One unsigned 32-bit operand.
    U32,
    /// One signed 64-bit operand.
    I64,
    /// One 64-bit float.
    F64,
    /// One string-table index.
    Str,
    /// Two unsigned operands: an id and an arity.
    U32U32,
    /// A dispatch table: an enum id, then a target offset per entry.
    Tables,
}

impl OperandKind {
    /// The kind with this tag, if there is one.
    #[must_use]
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::None),
            1 => Some(Self::U32),
            2 => Some(Self::I64),
            3 => Some(Self::F64),
            4 => Some(Self::Str),
            5 => Some(Self::U32U32),
            6 => Some(Self::Tables),
            _ => None,
        }
    }
}

/// An instruction's operands.
#[derive(Clone, PartialEq, Debug)]
pub enum Operand {
    /// Nothing.
    None,
    /// An index, an offset, or a count.
    U32(u32),
    /// A signed constant.
    I64(i64),
    /// A float constant.
    F64(f64),
    /// A string-table index.
    Str(u32),
    /// An id and an arity.
    Pair(u32, u32),
    /// A dispatch table: which enum the tag belongs to, and one target per entry.
    ///
    /// Travels with the instruction rather than sitting after it as loose bytes, so that
    /// nothing has to know how an instruction is laid out in order to find its table.
    Tables(u32, Vec<u32>),
}

/// The enum id a dispatch table uses when its tag is not an enum at all.
///
/// A menu dispatches on a choice position, which has no variants to cover. The verifier's
/// totality rule has nothing to check there, and saying so with a value beats guessing from
/// the table's shape.
pub const NO_ENUM: u32 = u32::MAX;

impl Operand {
    /// The operand's kind, which is what the stream records.
    #[must_use]
    pub fn kind(&self) -> OperandKind {
        match self {
            Self::None => OperandKind::None,
            Self::U32(_) => OperandKind::U32,
            Self::I64(_) => OperandKind::I64,
            Self::F64(_) => OperandKind::F64,
            Self::Str(_) => OperandKind::Str,
            Self::Pair(_, _) => OperandKind::U32U32,
            Self::Tables(_, _) => OperandKind::Tables,
        }
    }

    /// The first unsigned operand, if this has one.
    #[must_use]
    pub fn u32(&self) -> Option<u32> {
        match self {
            Self::U32(value) | Self::Str(value) => Some(*value),
            Self::Pair(first, _) | Self::Tables(first, _) => Some(*first),
            _ => None,
        }
    }

    /// The arity this operand carries, for an instruction whose count travels with it.
    ///
    /// `U32` is the count itself; a `Pair` puts an id first and the count second.
    #[must_use]
    pub fn count(&self) -> usize {
        match self {
            Self::U32(value) => *value as usize,
            Self::Pair(_, count) => *count as usize,
            _ => 0,
        }
    }

    /// How big this is on the wire.
    #[must_use]
    pub fn width(&self) -> usize {
        match self {
            Self::None => 0,
            Self::U32(_) | Self::Str(_) => 4,
            Self::I64(_) | Self::F64(_) => 8,
            Self::Pair(_, _) => 8,
            // The enum id and the count, then the targets.
            Self::Tables(_, targets) => 8 + 4 * targets.len(),
        }
    }
}
