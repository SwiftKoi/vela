//! Decoding a `.velac` container.
//!
//! Total, in the sense that matters for a loader: no input makes this panic. A truncated
//! file, a bad magic number, or a section whose length runs past the end all produce an
//! error rather than a slice index out of range — because the input is a file that arrived
//! over a network, and a loader that crashes on a corrupt one is a loader that crashes.
//!
//! Every read is bounds-checked once, here, rather than at each call site. Beyond this
//! point the decoded module is the same in-memory shape the compiler produces, so nothing
//! downstream has to know a container was involved.

use std::fmt;

use vela_span::{FileId, Span};

use crate::codec::fnv1a;
use crate::module::{
    CommandSchema, ConstId, DebugInfo, DefaultDef, FORMAT, FieldSchema, FuncDef, Header, Instr,
    LocalDef, Module, StringId, StringTable, TypeId, TypeTable,
};
use crate::op::{Op, Operand, OperandKind};

use super::tables::{
    read_consts, read_effects, read_enums, read_strings, read_structs, read_types,
};

/// Why a container could not be read.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DecodeError {
    /// The format version is not one this build reads (`E7101`).
    UnknownFormat(u16),
    /// The bytes do not describe a module (`E7103`).
    Corrupt(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFormat(version) => {
                write!(f, "bytecode format {version} is not supported")
            }
            Self::Corrupt(reason) => write!(f, "the module container is corrupt: {reason}"),
        }
    }
}

/// Decodes a container.
///
/// # Errors
///
/// Returns [`DecodeError::UnknownFormat`] for a format this build does not read, and
/// [`DecodeError::Corrupt`] for anything else that does not describe a module.
pub fn decode(bytes: &[u8]) -> Result<Module, DecodeError> {
    let mut reader = Reader::new(bytes);

    let header = read_header(&mut reader)?;
    if reader.failed() {
        return Err(corrupt(&reader));
    }

    // Sections are read in the order they were written, and each is length-prefixed so that
    // a mismatch is caught here rather than by reading the wrong bytes as the next section.
    let strings = reader.section(read_strings)?;
    let consts = reader.section(read_consts)?;
    let types = reader.section(read_types)?;
    let structs = reader.section(read_structs)?;
    let enums = reader.section(read_enums)?;
    let fns = reader.section(read_bodies)?;
    let labels = reader.section(read_bodies)?;
    let defaults = reader.section(read_defaults)?;
    let effects = reader.section(read_effects)?;
    let cmds = reader.section(read_commands)?;
    let debug = reader.section(read_debug)?;

    if reader.failed() {
        return Err(corrupt(&reader));
    }

    let stored = reader.u64();
    if reader.failed() {
        return Err(corrupt(&reader));
    }

    // The checksum covers everything before it, which is what catches a truncated transfer.
    let covered = bytes.len().saturating_sub(8);
    let computed = fnv1a(&bytes[..covered]);
    if stored != computed {
        return Err(DecodeError::Corrupt(format!(
            "checksum is {stored:#018x}, expected {computed:#018x}"
        )));
    }

    Ok(Module {
        header,
        strings: StringTable::from_items(strings),
        consts: crate::module::ConstPool::from_items(consts),
        types: TypeTable::from_items(types),
        structs,
        enums,
        fns,
        labels,
        defaults,
        effects,
        cmds,
        debug,
        checksum: stored,
    })
}

/// Reads and checks the header.
fn read_header(reader: &mut Reader<'_>) -> Result<Header, DecodeError> {
    let mut magic = [0u8; 4];
    for byte in &mut magic {
        *byte = reader.u8();
    }
    if reader.failed() || magic != Header::MAGIC {
        return Err(DecodeError::Corrupt("bad magic".to_string()));
    }

    let format = reader.u16();
    let abi = reader.u16();
    let flags = reader.u32();
    let mut schema_digest = [0u8; 32];
    for byte in &mut schema_digest {
        *byte = reader.u8();
    }

    // A *newer* format is rejected; an older one is read, because backward compatibility is
    // required and forward compatibility is explicitly a non-goal (`BYTECODE.md §6`).
    if format > FORMAT {
        return Err(DecodeError::UnknownFormat(format));
    }

    Ok(Header {
        magic,
        format,
        abi,
        flags,
        schema_digest,
    })
}
/// The function and label bodies.
fn read_bodies(reader: &mut Reader<'_>) -> Vec<FuncDef> {
    let count = reader.count();
    let mut items = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        items.push(read_body(reader));
    }
    items
}

/// One function or label.
fn read_body(reader: &mut Reader<'_>) -> FuncDef {
    let name = StringId(reader.u32());
    let ret = TypeId(reader.u32());

    let params = reader.u32s();
    let locals = reader.locals();

    let code_len = reader.count();
    let mut code = Vec::with_capacity(code_len.min(4096));
    for _ in 0..code_len {
        code.push(read_instr(reader));
    }

    let span_len = reader.count();
    let mut spans = Vec::with_capacity(span_len.min(4096));
    for _ in 0..span_len {
        let file = reader.u32();
        let start = reader.u32();
        let end = reader.u32();
        spans.push(Span::new(FileId::from_raw(file), start, end));
    }

    FuncDef {
        name,
        params,
        ret,
        locals,
        code,
        spans,
    }
}

/// One instruction.
fn read_instr(reader: &mut Reader<'_>) -> Instr {
    let opcode = reader.u8();
    let kind = reader.u8();

    let Some(op) = Op::from_byte(opcode) else {
        reader.fail("an instruction has an unknown opcode");
        return Instr::plain(Op::Nop);
    };
    let Some(kind) = OperandKind::from_byte(kind) else {
        reader.fail("an instruction has an unknown operand kind");
        return Instr::plain(op);
    };

    let operand = match kind {
        OperandKind::None => Operand::None,
        OperandKind::U32 => Operand::U32(reader.u32()),
        OperandKind::I64 => Operand::I64(reader.i64()),
        OperandKind::F64 => Operand::F64(reader.f64()),
        OperandKind::Str => Operand::Str(reader.u32()),
        OperandKind::U32U32 => {
            let first = reader.u32();
            let second = reader.u32();
            Operand::Pair(first, second)
        }
        OperandKind::Tables => {
            let enum_id = reader.u32();
            let count = reader.count();
            let mut targets = Vec::with_capacity(count.min(4096));
            for _ in 0..count {
                targets.push(reader.u32());
            }
            Operand::Tables(enum_id, targets)
        }
    };

    Instr { op, operand }
}

/// The defaults.
fn read_defaults(reader: &mut Reader<'_>) -> Vec<DefaultDef> {
    let count = reader.count();
    let mut items = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        items.push(DefaultDef {
            name: StringId(reader.u32()),
            ty: TypeId(reader.u32()),
            init: ConstId(reader.u32()),
        });
    }
    items
}

/// The command schemas.
fn read_commands(reader: &mut Reader<'_>) -> Vec<CommandSchema> {
    let count = reader.count();
    let mut items = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let name = StringId(reader.u32());
        let fields = reader.count();
        let mut schemas = Vec::with_capacity(fields.min(1024));
        for _ in 0..fields {
            schemas.push(FieldSchema {
                name: StringId(reader.u32()),
                ty: TypeId(reader.u32()),
            });
        }
        items.push(CommandSchema {
            name,
            fields: schemas,
        });
    }
    items
}

/// The debug info.
fn read_debug(reader: &mut Reader<'_>) -> DebugInfo {
    let present = reader.u8() != 0;
    let count = reader.count();
    let mut files = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        files.push(reader.string());
    }
    DebugInfo { present, files }
}

/// The reason a reader gave up, for the diagnostic.
fn corrupt(reader: &Reader<'_>) -> DecodeError {
    DecodeError::Corrupt(
        reader
            .error
            .clone()
            .unwrap_or_else(|| "the bytes ran out".to_string()),
    )
}

/// A bounds-checked cursor.
///
/// Reads past the end return zero and set an error rather than panicking. Every caller
/// checks once at the end, which is cheaper than a `Result` in every signature and just as
/// total — the module is either built or rejected, and a partially-read one is never
/// returned.
pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    error: Option<String>,
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            at: 0,
            error: None,
        }
    }

    pub(super) fn failed(&self) -> bool {
        self.error.is_some()
    }

    pub(super) fn fail(&mut self, reason: &str) {
        if self.error.is_none() {
            self.error = Some(reason.to_string());
        }
    }

    /// Takes `width` bytes, or fails.
    pub(super) fn take(&mut self, width: usize) -> &'a [u8] {
        let end = self.at.saturating_add(width);
        if end > self.bytes.len() {
            self.fail("the container ended mid-value");
            return &[];
        }
        let slice = &self.bytes[self.at..end];
        self.at = end;
        slice
    }

    pub(super) fn u8(&mut self) -> u8 {
        self.take(1).first().copied().unwrap_or(0)
    }

    pub(super) fn u16(&mut self) -> u16 {
        let bytes = self.take(2);
        if bytes.len() < 2 {
            return 0;
        }
        u16::from_le_bytes([bytes[0], bytes[1]])
    }

    pub(super) fn u32(&mut self) -> u32 {
        let bytes = self.take(4);
        if bytes.len() < 4 {
            return 0;
        }
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    pub(super) fn u64(&mut self) -> u64 {
        let bytes = self.take(8);
        if bytes.len() < 8 {
            return 0;
        }
        let mut buffer = [0u8; 8];
        buffer.copy_from_slice(bytes);
        u64::from_le_bytes(buffer)
    }

    pub(super) fn i64(&mut self) -> i64 {
        self.u64() as i64
    }

    pub(super) fn f64(&mut self) -> f64 {
        f64::from_bits(self.u64())
    }

    /// A length, refused if it could not possibly fit in what is left.
    pub(super) fn count(&mut self) -> usize {
        let count = self.u32() as usize;
        // A length is the one place a corrupt file can ask for an enormous allocation, so it
        // is checked against what remains rather than trusted.
        if count > self.bytes.len() {
            self.fail("a length is larger than the container");
            return 0;
        }
        count
    }

    pub(super) fn string(&mut self) -> String {
        let length = self.count();
        let bytes = self.take(length);
        String::from_utf8_lossy(bytes).into_owned()
    }

    /// A length-prefixed run of `u32`s.
    pub(super) fn u32s(&mut self) -> Vec<u32> {
        let count = self.count();
        let mut items = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            items.push(self.u32());
        }
        items
    }

    /// A length-prefixed run of type ids.
    pub(super) fn types(&mut self) -> Vec<TypeId> {
        self.u32s().into_iter().map(TypeId).collect()
    }

    /// A length-prefixed run of local declarations.
    pub(super) fn locals(&mut self) -> Vec<LocalDef> {
        let count = self.count();
        let mut items = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            items.push(LocalDef {
                name: StringId(self.u32()),
                ty: TypeId(self.u32()),
            });
        }
        items
    }

    /// Reads a length-prefixed section.
    fn section<T>(&mut self, read: impl FnOnce(&mut Self) -> T) -> Result<T, DecodeError> {
        let length = self.u32() as usize;
        let end = self.at.saturating_add(length);
        if end > self.bytes.len() {
            return Err(DecodeError::Corrupt(
                "a section is longer than the container".to_string(),
            ));
        }

        let outer = self.bytes;
        let mut inner = Reader {
            bytes: &self.bytes[self.at..end],
            at: 0,
            error: None,
        };
        let value = read(&mut inner);

        if let Some(error) = inner.error {
            return Err(DecodeError::Corrupt(error));
        }
        self.bytes = outer;
        self.at = end;
        Ok(value)
    }
}
