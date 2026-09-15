//! Reading the tables: strings, constants, types, and shapes.
//!
//! Separated from the body reader because they are what a *loader* needs before it can
//! interpret an instruction: a type id means nothing until the type table has been read,
//! and a string constant means nothing until the string table has.

use crate::module::{
    ByteConst, ByteTy, EffectDef, EnumDef, FieldSchema, StringId, StructDef, TypeId, VariantDef,
};

use super::read::Reader;

/// The string table.
pub(super) fn read_strings(reader: &mut Reader<'_>) -> Vec<String> {
    let count = reader.count();
    let mut items = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        items.push(reader.string());
    }
    items
}

/// The constant pool.
pub(super) fn read_consts(reader: &mut Reader<'_>) -> Vec<ByteConst> {
    let count = reader.count();
    let mut items = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        items.push(read_const(reader));
    }
    items
}

/// One constant.
pub(super) fn read_const(reader: &mut Reader<'_>) -> ByteConst {
    match reader.u8() {
        1 => ByteConst::Bool(reader.u8() != 0),
        2 => ByteConst::Int(reader.i64()),
        3 => ByteConst::Float(reader.f64()),
        4 => ByteConst::Str(StringId(reader.u32())),
        5 => ByteConst::Variant {
            enum_id: reader.u32(),
            variant: reader.u32(),
        },
        6 => ByteConst::Function(reader.u32()),
        _ => {
            reader.fail("a constant has an unknown tag");
            ByteConst::None
        }
    }
}

/// The type table.
pub(super) fn read_types(reader: &mut Reader<'_>) -> Vec<ByteTy> {
    let count = reader.count();
    let mut items = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        items.push(read_type(reader));
    }
    items
}

/// One type.
pub(super) fn read_type(reader: &mut Reader<'_>) -> ByteTy {
    match reader.u8() {
        0 => ByteTy::Int,
        1 => ByteTy::Float,
        2 => ByteTy::Bool,
        3 => ByteTy::Str,
        4 => ByteTy::None,
        5 => ByteTy::Unit,
        6 => ByteTy::Unknown,
        7 => ByteTy::Optional(Box::new(read_type(reader))),
        8 => ByteTy::List(Box::new(read_type(reader))),
        9 => {
            let key = read_type(reader);
            let value = read_type(reader);
            ByteTy::Map(Box::new(key), Box::new(value))
        }
        10 => ByteTy::Struct(reader.u32()),
        11 => ByteTy::Enum(reader.u32()),
        12 => ByteTy::Func(reader.u32()),
        _ => {
            reader.fail("a type has an unknown tag");
            ByteTy::Unknown
        }
    }
}

/// The effects the module calls.
pub(super) fn read_effects(reader: &mut Reader<'_>) -> Vec<EffectDef> {
    let count = reader.count();
    let mut items = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        items.push(EffectDef {
            name: StringId(reader.u32()),
            arity: reader.u32(),
        });
    }
    items
}

/// The struct shapes.
pub(super) fn read_structs(reader: &mut Reader<'_>) -> Vec<StructDef> {
    let count = reader.count();
    let mut items = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let name = StringId(reader.u32());
        let fields = reader.count();
        let mut schemas = Vec::with_capacity(fields.min(1024));
        for _ in 0..fields {
            let field_name = StringId(reader.u32());
            let ty = TypeId(reader.u32());
            schemas.push(FieldSchema {
                name: field_name,
                ty,
            });
        }
        items.push(StructDef {
            name,
            fields: schemas,
        });
    }
    items
}

/// The enum shapes.
pub(super) fn read_enums(reader: &mut Reader<'_>) -> Vec<EnumDef> {
    let count = reader.count();
    let mut items = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let name = StringId(reader.u32());
        let variants = reader.count();
        let mut definitions = Vec::with_capacity(variants.min(1024));
        for _ in 0..variants {
            let name = StringId(reader.u32());
            let fields = reader.types();
            definitions.push(VariantDef { name, fields });
        }
        items.push(EnumDef {
            name,
            variants: definitions,
        });
    }
    items
}
