//! The bytecode module: what a compiled story is.

use vela_span::Span;

use crate::op::{Op, Operand};

/// The bytecode format this build writes and the oldest it reads.
///
/// `BYTECODE.md §6`: bumped only for **incompatible** changes. A new command variant or a
/// new type is additive and does not bump it; a new *instruction* does, because an old
/// loader has no handler for it and would otherwise read the next instruction as its
/// operand.
pub const FORMAT: u16 = 1;

/// The plugin ABI this build was compiled against.
pub const ABI: u16 = 1;

/// A string-table index.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct StringId(pub u32);

/// A constant-pool index.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ConstId(pub u32);

/// A type-table index.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TypeId(pub u32);

/// The strings a module mentions.
#[derive(Clone, Default, Debug)]
pub struct StringTable {
    items: Vec<String>,
}

impl StringTable {
    /// Adds a string, reusing an identical one.
    pub fn add(&mut self, text: impl Into<String>) -> StringId {
        let text = text.into();
        if let Some(index) = self.items.iter().position(|item| *item == text) {
            return StringId(u32::try_from(index).unwrap_or(u32::MAX));
        }
        let id = StringId(u32::try_from(self.items.len()).unwrap_or(u32::MAX));
        self.items.push(text);
        id
    }

    /// The string at an index.
    #[must_use]
    pub fn get(&self, id: StringId) -> Option<&str> {
        self.items.get(id.0 as usize).map(String::as_str)
    }

    /// Every string, in insertion order.
    #[must_use]
    pub fn items(&self) -> &[String] {
        &self.items
    }

    /// Rebuilds a table from decoded strings.
    #[must_use]
    pub fn from_items(items: Vec<String>) -> Self {
        Self { items }
    }
}

/// A type in the type table.
///
/// Structural rather than nominal: two `list<int>`s are the same entry, and a struct or
/// enum refers to the schema the module also carries. Equality is what the verifier's
/// stack-type rule compares with.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ByteTy {
    /// `int`
    Int,
    /// `float`
    Float,
    /// `bool`
    Bool,
    /// `str`
    Str,
    /// `none`
    None,
    /// Nothing at all — the type of a call with no result.
    Unit,
    /// `T?`
    Optional(Box<Self>),
    /// `list<T>`
    List(Box<Self>),
    /// `map<K, V>`
    Map(Box<Self>, Box<Self>),
    /// A declared struct, by index into `Module::structs`.
    Struct(u32),
    /// A declared enum, by index into `Module::enums`.
    Enum(u32),
    /// A function value, by index into `Module::fns`.
    Func(u32),
    /// A type the checker could not work out.
    ///
    /// Present for totality rather than for use: `vela build` refuses a module whose
    /// checking reported an error, so a verified module should not contain one. A story
    /// that does carry it is one that was compiled from a source file with a mistake in
    /// *another* function, which lowering still had to produce something for.
    Unknown,
}

/// The types a module mentions.
#[derive(Clone, Default, Debug)]
pub struct TypeTable {
    items: Vec<ByteTy>,
}

impl TypeTable {
    /// Adds a type, reusing an equal one.
    pub fn add(&mut self, ty: ByteTy) -> TypeId {
        if let Some(index) = self.items.iter().position(|item| *item == ty) {
            return TypeId(u32::try_from(index).unwrap_or(u32::MAX));
        }
        let id = TypeId(u32::try_from(self.items.len()).unwrap_or(u32::MAX));
        self.items.push(ty);
        id
    }

    /// The type at an index.
    #[must_use]
    pub fn get(&self, id: TypeId) -> Option<&ByteTy> {
        self.items.get(id.0 as usize)
    }

    /// Every type, in insertion order.
    #[must_use]
    pub fn items(&self) -> &[ByteTy] {
        &self.items
    }

    /// Rebuilds a table from decoded types.
    #[must_use]
    pub fn from_items(items: Vec<ByteTy>) -> Self {
        Self { items }
    }
}

/// A constant.
#[derive(Clone, PartialEq, Debug)]
pub enum ByteConst {
    /// `none`
    None,
    /// A boolean.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A float.
    Float(f64),
    /// A string, by string-table index.
    Str(StringId),
    /// A variant with no payload.
    Variant {
        /// The enum's index into `Module::enums`.
        enum_id: u32,
        /// The variant's position within it.
        variant: u32,
    },
    /// A function used as a value.
    Function(u32),
}

/// The constants a module mentions.
#[derive(Clone, Default, Debug)]
pub struct ConstPool {
    items: Vec<ByteConst>,
}

impl ConstPool {
    /// Adds a constant, reusing an equal one.
    pub fn add(&mut self, value: ByteConst) -> ConstId {
        if let Some(index) = self.items.iter().position(|item| *item == value) {
            return ConstId(u32::try_from(index).unwrap_or(u32::MAX));
        }
        let id = ConstId(u32::try_from(self.items.len()).unwrap_or(u32::MAX));
        self.items.push(value);
        id
    }

    /// The constant at an index.
    #[must_use]
    pub fn get(&self, id: ConstId) -> Option<&ByteConst> {
        self.items.get(id.0 as usize)
    }

    /// Every constant, in insertion order.
    #[must_use]
    pub fn items(&self) -> &[ByteConst] {
        &self.items
    }

    /// Rebuilds a pool from decoded constants.
    #[must_use]
    pub fn from_items(items: Vec<ByteConst>) -> Self {
        Self { items }
    }
}

/// One instruction.
#[derive(Clone, PartialEq, Debug)]
pub struct Instr {
    /// What it does.
    pub op: Op,
    /// What it does it to.
    pub operand: Operand,
}

impl Instr {
    /// An instruction with no operand.
    #[must_use]
    pub fn plain(op: Op) -> Self {
        Self {
            op,
            operand: Operand::None,
        }
    }

    /// An instruction with an unsigned operand.
    #[must_use]
    pub fn with_u32(op: Op, operand: u32) -> Self {
        Self {
            op,
            operand: Operand::U32(operand),
        }
    }

    /// How big the instruction is on the wire: two bytes of header, then the operands.
    #[must_use]
    pub fn width(&self) -> usize {
        2 + self.operand.width()
    }
}

/// A local slot's declaration.
#[derive(Clone, PartialEq, Debug)]
pub struct LocalDef {
    /// Its name, by string-table index.
    pub name: StringId,
    /// Its type, by type-table index.
    pub ty: TypeId,
}

/// A function or a label.
#[derive(Clone, PartialEq, Debug)]
pub struct FuncDef {
    /// Its name, by string-table index.
    pub name: StringId,
    /// The slots its arguments arrive in.
    pub params: Vec<u32>,
    /// What it returns, by type-table index.
    pub ret: TypeId,
    /// Every slot, indexed by slot number.
    pub locals: Vec<LocalDef>,
    /// The instructions.
    pub code: Vec<Instr>,
    /// Where each instruction came from, parallel to `code`.
    ///
    /// Indexed by instruction rather than by byte offset: the offset form is what the
    /// debugger wants and is derived by the codec, which is the only place that knows the
    /// widths. Keeping offsets here would mean recomputing them on every insertion.
    pub spans: Vec<Span>,
}

impl FuncDef {
    /// The byte offset of instruction `index` from the start of the code.
    #[must_use]
    pub fn offset_of(&self, index: usize) -> u32 {
        let mut offset = 0u32;
        for instr in self.code.iter().take(index) {
            offset = offset.saturating_add(u32::try_from(instr.width()).unwrap_or(u32::MAX));
        }
        offset
    }

    /// The instruction starting at a byte offset, and its index.
    #[must_use]
    pub fn at_offset(&self, offset: u32) -> Option<(usize, &Instr)> {
        let mut current = 0u32;
        for (index, instr) in self.code.iter().enumerate() {
            if current == offset {
                return Some((index, instr));
            }
            current = current.saturating_add(u32::try_from(instr.width()).unwrap_or(u32::MAX));
        }
        None
    }
}

/// A struct's or enum's shape, so the verifier can check aggregate operations.
#[derive(Clone, PartialEq, Debug)]
pub struct StructDef {
    /// Its name, by string-table index.
    pub name: StringId,
    /// Its fields, in declaration order.
    ///
    /// Named as well as typed, because a struct *value* carries its field names — a
    /// `FieldGet` is by name, and a debugger showing `Route { name: "north" }` needs them.
    /// An index would have been smaller and would have made the disassembly unreadable.
    pub fields: Vec<FieldSchema>,
}

/// An enum's shape.
#[derive(Clone, PartialEq, Debug)]
pub struct EnumDef {
    /// Its name, by string-table index.
    pub name: StringId,
    /// Each variant: its name, then its payload types.
    pub variants: Vec<VariantDef>,
}

/// One variant of an enum.
#[derive(Clone, PartialEq, Debug)]
pub struct VariantDef {
    /// Its name, by string-table index.
    pub name: StringId,
    /// Its payload's types, in declaration order.
    pub fields: Vec<TypeId>,
}

/// A `default`: world state, and the save schema.
#[derive(Clone, PartialEq, Debug)]
pub struct DefaultDef {
    /// Its name, by string-table index.
    pub name: StringId,
    /// Its type, by type-table index.
    pub ty: TypeId,
    /// Its initial value, by constant-pool index.
    pub init: ConstId,
}

/// What a module carries for a debugger.
///
/// `BYTECODE.md §5` names three things: a span table, slot names, and a line table. Spans
/// live in each `FuncDef` and slot names in each `LocalDef`; the line table proper is built
/// by the DAP server from these plus the source, which it already has.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct DebugInfo {
    /// Whether any is present. `vela build --release` clears it.
    pub present: bool,
    /// The source files the spans refer to, by file id.
    pub files: Vec<String>,
}

/// The header a `.velac` container begins with (`BYTECODE.md §3.1`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Header {
    /// Always `b"VELA"`.
    pub magic: [u8; 4],
    /// The bytecode format. A loader rejects an unknown one with `E7101`.
    pub format: u16,
    /// The plugin ABI the module was built against.
    pub abi: u16,
    /// Bit 0: debug info present. Bit 1: has a source map.
    pub flags: u32,
    /// A digest of the schema the module was built against.
    pub schema_digest: [u8; 32],
}

impl Header {
    /// The magic bytes every module begins with.
    pub const MAGIC: [u8; 4] = *b"VELA";

    /// Bit 0 of `flags`: debug info is present.
    pub const FLAG_DEBUG: u32 = 1;
    /// Bit 1 of `flags`: a source map is present.
    pub const FLAG_SOURCE_MAP: u32 = 1 << 1;

    /// A header for a freshly built module.
    #[must_use]
    pub fn new(debug: bool) -> Self {
        Self {
            magic: Self::MAGIC,
            format: FORMAT,
            abi: ABI,
            flags: if debug { Self::FLAG_DEBUG } else { 0 },
            schema_digest: [0; 32],
        }
    }

    /// Whether the module carries debug information.
    #[must_use]
    pub fn has_debug(&self) -> bool {
        self.flags & Self::FLAG_DEBUG != 0
    }
}

/// A compiled module.
///
/// # Equality and the checksum
///
/// `PartialEq` compares everything **except** the checksum, because the checksum is a
/// property of the *encoding* rather than of the module: it is computed by `encode` over
/// the bytes it wrote and verified by `decode`. Comparing it would make the round-trip
/// criterion — `decode(encode(m)) == m` — uncheckable for a reason that has nothing to do
/// with whether the round trip worked.
#[derive(Clone, Debug)]
pub struct Module {
    /// How to read it.
    pub header: Header,
    /// Every string it mentions.
    pub strings: StringTable,
    /// Every constant.
    pub consts: ConstPool,
    /// Every type.
    pub types: TypeTable,
    /// Every declared struct, by index.
    pub structs: Vec<StructDef>,
    /// Every declared enum, by index.
    pub enums: Vec<EnumDef>,
    /// Every function. `CallFn` indexes this.
    pub fns: Vec<FuncDef>,
    /// Every label. `CallLabel` indexes this.
    pub labels: Vec<FuncDef>,
    /// Every `default`, which is the save schema.
    pub defaults: Vec<DefaultDef>,
    /// The effects the module calls, by name and arity, in declaration order.
    ///
    /// Names rather than ids because the *runtime* supplies the implementation: a module and
    /// an engine built separately have to agree on what `rand.int` means, and the name is
    /// the only thing they can agree on.
    pub effects: Vec<EffectDef>,
    /// Every command variant the module uses, with its field schema.
    pub cmds: Vec<CommandSchema>,
    /// What a debugger reads.
    pub debug: DebugInfo,
    /// An FNV-1a hash over the encoded module. Written by `encode`, checked by `decode`.
    pub checksum: u64,
}

impl PartialEq for Module {
    fn eq(&self, other: &Self) -> bool {
        self.header == other.header
            && self.strings.items() == other.strings.items()
            && self.consts.items() == other.consts.items()
            && self.types.items() == other.types.items()
            && self.structs == other.structs
            && self.enums == other.enums
            && self.fns == other.fns
            && self.labels == other.labels
            && self.defaults == other.defaults
            && self.cmds == other.cmds
            && self.debug == other.debug
    }
}

/// A host capability the module calls.
#[derive(Clone, PartialEq, Debug)]
pub struct EffectDef {
    /// Its dotted name, by string-table index.
    pub name: StringId,
    /// How many arguments it takes.
    pub arity: u32,
}

/// A command variant and the fields it takes.
///
/// `BYTECODE.md §3.3`: adding a command is a *schema registration*, not an instruction. The
/// loader and the UI both read this, so a module that uses a variant its runtime does not
/// know is rejected with `E7102` rather than executing something that means the wrong
/// thing.
#[derive(Clone, PartialEq, Debug)]
pub struct CommandSchema {
    /// The variant's name, by string-table index.
    pub name: StringId,
    /// Its fields: name and type, in the order `Cmd` pushes them.
    pub fields: Vec<FieldSchema>,
}

/// One field of a command or effect.
#[derive(Clone, PartialEq, Debug)]
pub struct FieldSchema {
    /// The field's name, by string-table index.
    pub name: StringId,
    /// Its type, by type-table index.
    pub ty: TypeId,
}

/// The effects a module can call, with their capabilities.
///
/// Not carried in the container: effects are a property of the *runtime*, and a module that
/// declares them would be claiming what a host may do. `BYTECODE.md §3.3`.
#[derive(Clone, PartialEq, Debug)]
pub struct EffectSchema {
    /// The effect's name.
    pub name: &'static str,
    /// Its argument types.
    pub args: &'static [ByteTy],
    /// What it returns.
    pub ret: ByteTy,
    /// The capability it needs (`ARCHITECTURE.md §6.3`).
    pub capability: &'static str,
}
