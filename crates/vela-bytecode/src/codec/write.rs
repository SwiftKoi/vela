//! Encoding a module into a `.velac` container.
//!
//! Little-endian, fixed-width, no alignment padding (`BYTECODE.md §3.1`). Every section is
//! length-prefixed, so a loader can skip one it does not understand — which is what makes
//! an additive change to the format a change a *new* reader handles and an old one survives.
//!
//! The checksum is written last and covers everything before it. It is not a signature: it
//! catches a truncated download or a half-written file, which are the two ways a module
//! actually arrives corrupt.

use crate::codec::fnv1a;
use crate::module::{
    ByteConst, ByteTy, CommandSchema, FuncDef, Instr, LocalDef, Module, StructDef, VariantDef,
};
use crate::op::Operand;

/// Encodes a module.
#[must_use]
pub fn encode(module: &Module) -> Vec<u8> {
    let mut writer = Writer::default();

    write_header(&mut writer, module);
    writer.section(|section| write_strings(section, module));
    writer.section(|section| write_consts(section, module));
    writer.section(|section| write_types(section, module));
    writer.section(|section| write_structs(section, module));
    writer.section(|section| write_enums(section, module));
    writer.section(|section| write_bodies(section, &module.fns));
    writer.section(|section| write_bodies(section, &module.labels));
    writer.section(|section| write_defaults(section, module));
    writer.section(|section| write_effects(section, module));
    writer.section(|section| write_commands(section, module));
    writer.section(|section| write_debug(section, module));

    let checksum = fnv1a(&writer.bytes);
    writer.u64(checksum);
    writer.bytes
}

/// The header, which a loader reads before it trusts anything else.
fn write_header(writer: &mut Writer, module: &Module) {
    writer.bytes.extend_from_slice(&module.header.magic);
    writer.u16(module.header.format);
    writer.u16(module.header.abi);
    writer.u32(module.header.flags);
    writer.bytes.extend_from_slice(&module.header.schema_digest);
}

/// The string table.
fn write_strings(writer: &mut Writer, module: &Module) {
    writer.u32(count(module.strings.items().len()));
    for text in module.strings.items() {
        writer.string(text);
    }
}

/// The constant pool.
fn write_consts(writer: &mut Writer, module: &Module) {
    writer.u32(count(module.consts.items().len()));
    for constant in module.consts.items() {
        match constant {
            ByteConst::None => writer.u8(0),
            ByteConst::Bool(flag) => {
                writer.u8(1);
                writer.u8(u8::from(*flag));
            }
            ByteConst::Int(number) => {
                writer.u8(2);
                writer.i64(*number);
            }
            ByteConst::Float(number) => {
                writer.u8(3);
                writer.f64(*number);
            }
            ByteConst::Str(id) => {
                writer.u8(4);
                writer.u32(id.0);
            }
            ByteConst::Variant { enum_id, variant } => {
                writer.u8(5);
                writer.u32(*enum_id);
                writer.u32(*variant);
            }
            ByteConst::Function(index) => {
                writer.u8(6);
                writer.u32(*index);
            }
        }
    }
}

/// The type table.
fn write_types(writer: &mut Writer, module: &Module) {
    writer.u32(count(module.types.items().len()));
    for ty in module.types.items() {
        write_type(writer, ty);
    }
}

/// One type.
fn write_type(writer: &mut Writer, ty: &ByteTy) {
    match ty {
        ByteTy::Int => writer.u8(0),
        ByteTy::Float => writer.u8(1),
        ByteTy::Bool => writer.u8(2),
        ByteTy::Str => writer.u8(3),
        ByteTy::None => writer.u8(4),
        ByteTy::Unit => writer.u8(5),
        ByteTy::Unknown => writer.u8(6),
        ByteTy::Optional(inner) => {
            writer.u8(7);
            write_type(writer, inner);
        }
        ByteTy::List(element) => {
            writer.u8(8);
            write_type(writer, element);
        }
        ByteTy::Map(key, value) => {
            writer.u8(9);
            write_type(writer, key);
            write_type(writer, value);
        }
        ByteTy::Struct(index) => {
            writer.u8(10);
            writer.u32(*index);
        }
        ByteTy::Enum(index) => {
            writer.u8(11);
            writer.u32(*index);
        }
        ByteTy::Func(index) => {
            writer.u8(12);
            writer.u32(*index);
        }
    }
}

/// The struct shapes.
fn write_structs(writer: &mut Writer, module: &Module) {
    writer.u32(count(module.structs.len()));
    for definition in &module.structs {
        write_struct(writer, definition);
    }
}

/// One struct.
fn write_struct(writer: &mut Writer, definition: &StructDef) {
    writer.u32(definition.name.0);
    writer.u32(count(definition.fields.len()));
    for field in &definition.fields {
        writer.u32(field.name.0);
        writer.u32(field.ty.0);
    }
}

/// The enum shapes.
fn write_enums(writer: &mut Writer, module: &Module) {
    writer.u32(count(module.enums.len()));
    for definition in &module.enums {
        writer.u32(definition.name.0);
        writer.u32(count(definition.variants.len()));
        for variant in &definition.variants {
            write_variant(writer, variant);
        }
    }
}

/// One variant.
fn write_variant(writer: &mut Writer, variant: &VariantDef) {
    writer.u32(variant.name.0);
    writer.u32(count(variant.fields.len()));
    for field in &variant.fields {
        writer.u32(field.0);
    }
}

/// The function and label bodies.
fn write_bodies(writer: &mut Writer, bodies: &[FuncDef]) {
    writer.u32(count(bodies.len()));
    for body in bodies {
        write_body(writer, body);
    }
}

/// One function or label.
fn write_body(writer: &mut Writer, body: &FuncDef) {
    writer.u32(body.name.0);
    writer.u32(body.ret.0);

    writer.u32(count(body.params.len()));
    for param in &body.params {
        writer.u32(*param);
    }

    writer.u32(count(body.locals.len()));
    for local in &body.locals {
        write_local(writer, local);
    }

    writer.u32(count(body.code.len()));
    for instr in &body.code {
        write_instr(writer, instr);
    }

    // Spans are written only when they are there. A release build drops them, and a loader
    // that expected a fixed arity would be reading the next section.
    writer.u32(count(body.spans.len()));
    for span in &body.spans {
        writer.u32(span.file().as_raw());
        writer.u32(span.start());
        writer.u32(span.end());
    }
}

/// A local slot's declaration.
fn write_local(writer: &mut Writer, local: &LocalDef) {
    writer.u32(local.name.0);
    writer.u32(local.ty.0);
}

/// One instruction: opcode, operand kind, operands.
fn write_instr(writer: &mut Writer, instr: &Instr) {
    writer.u8(instr.op as u8);
    writer.u8(instr.operand.kind() as u8);

    match &instr.operand {
        Operand::None => {}
        Operand::U32(value) => writer.u32(*value),
        Operand::I64(value) => writer.i64(*value),
        Operand::F64(value) => writer.f64(*value),
        Operand::Str(value) => writer.u32(*value),
        Operand::Pair(first, second) => {
            writer.u32(*first);
            writer.u32(*second);
        }
        Operand::Tables(enum_id, targets) => {
            writer.u32(*enum_id);
            writer.u32(count(targets.len()));
            for target in targets {
                writer.u32(*target);
            }
        }
    }
}

/// The defaults, which are the save schema.
fn write_defaults(writer: &mut Writer, module: &Module) {
    writer.u32(count(module.defaults.len()));
    for definition in &module.defaults {
        writer.u32(definition.name.0);
        writer.u32(definition.ty.0);
        writer.u32(definition.init.0);
    }
}

/// The effects the module calls.
fn write_effects(writer: &mut Writer, module: &Module) {
    writer.u32(count(module.effects.len()));
    for effect in &module.effects {
        writer.u32(effect.name.0);
        writer.u32(effect.arity);
    }
}

/// The command schemas.
fn write_commands(writer: &mut Writer, module: &Module) {
    writer.u32(count(module.cmds.len()));
    for schema in &module.cmds {
        write_command(writer, schema);
    }
}

/// One command schema.
fn write_command(writer: &mut Writer, schema: &CommandSchema) {
    writer.u32(schema.name.0);
    writer.u32(count(schema.fields.len()));
    for field in &schema.fields {
        writer.u32(field.name.0);
        writer.u32(field.ty.0);
    }
}

/// The debug info.
fn write_debug(writer: &mut Writer, module: &Module) {
    writer.u8(u8::from(module.debug.present));
    writer.u32(count(module.debug.files.len()));
    for file in &module.debug.files {
        writer.string(file);
    }
}

/// A length as a `u32`, saturating rather than panicking on an impossible count.
fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// A byte sink.
#[derive(Default)]
struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    /// Writes a length-prefixed section.
    ///
    /// The length is patched afterwards, so a section never has to be built in a buffer of
    /// its own to know how long it is.
    fn section(&mut self, write: impl FnOnce(&mut Self)) {
        let length_at = self.bytes.len();
        self.u32(0);
        let start = self.bytes.len();
        write(self);

        let length = u32::try_from(self.bytes.len() - start).unwrap_or(u32::MAX);
        self.bytes[length_at..length_at + 4].copy_from_slice(&length.to_le_bytes());
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn f64(&mut self, value: f64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Writes a length-prefixed string.
    fn string(&mut self, text: &str) {
        self.u32(u32::try_from(text.len()).unwrap_or(u32::MAX));
        self.bytes.extend_from_slice(text.as_bytes());
    }
}
