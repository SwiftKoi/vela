//! Assembling a whole module.

use std::collections::BTreeMap;

use vela_mir::{Body as MirBody, Const, Module as MirModule};
use vela_types::Ty;

use super::body::Emitter;
use crate::module::{
    ByteConst, ByteTy, CommandSchema, ConstPool, DebugInfo, DefaultDef, EffectDef, EnumDef,
    FieldSchema, FuncDef, Header, Module, StringTable, StructDef, TypeId, TypeTable, VariantDef,
};

/// Compiles a MIR module into bytecode.
///
/// Never fails on the *story's* account: a module whose checking reported errors still
/// lowers and still assembles, because `vela build` is what refuses to ship one. What can
/// fail is this compiler, and it says so rather than emitting a module that verifies by
/// accident.
#[must_use]
pub fn compile(module: &MirModule, debug: bool) -> Module {
    let mut builder = Builder::new();
    builder.declare_shapes(module);
    builder.declare_constants(module);

    for effect in &module.effects {
        let name = builder.strings.add(effect.name.clone());
        let arity = u32::try_from(effect.params.len()).unwrap_or(u32::MAX);
        builder.effects.push(EffectDef { name, arity });
    }

    let fns = module
        .fns
        .iter()
        .map(|body| builder.emit_body(module, body))
        .collect();
    let labels = module
        .labels
        .iter()
        .map(|body| builder.emit_body(module, body))
        .collect();

    let mut header = Header::new(debug);
    header.schema_digest = schema_digest(&builder.cmds);

    Module {
        header,
        strings: builder.strings,
        consts: builder.consts,
        types: builder.types,
        structs: builder.structs,
        enums: builder.enums,
        fns,
        labels,
        defaults: builder.defaults,
        cmds: builder.cmds,
        effects: builder.effects,
        debug: DebugInfo {
            present: debug,
            files: Vec::new(),
        },
        checksum: 0,
    }
}

/// What one assembly carries between bodies.
pub struct Builder {
    pub(crate) strings: StringTable,
    pub(crate) types: TypeTable,
    pub(crate) consts: ConstPool,
    pub(crate) structs: Vec<StructDef>,
    pub(crate) enums: Vec<EnumDef>,
    pub(crate) defaults: Vec<DefaultDef>,
    /// Where each label sits, so a `CallLabel` becomes an index.
    pub(crate) labels: BTreeMap<String, u32>,
    /// Struct name to its index, for a `StructNew`.
    pub(crate) struct_ids: BTreeMap<String, u32>,
    /// Enum name to its index, for an `EnumNew` and a dispatch table.
    pub(crate) enum_ids: BTreeMap<String, u32>,
    /// Constant name to its pool id, and its type.
    pub(crate) constants: BTreeMap<String, (crate::module::ConstId, Ty)>,
    /// The command schemas the module uses, in first-use order.
    pub(crate) cmds: Vec<CommandSchema>,
    /// The effects the module calls.
    pub(crate) effects: Vec<EffectDef>,
}

impl Builder {
    /// An empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            strings: StringTable::default(),
            types: TypeTable::default(),
            consts: ConstPool::default(),
            structs: Vec::new(),
            enums: Vec::new(),
            defaults: Vec::new(),
            labels: BTreeMap::new(),
            struct_ids: BTreeMap::new(),
            enum_ids: BTreeMap::new(),
            constants: BTreeMap::new(),
            cmds: Vec::new(),
            effects: Vec::new(),
        }
    }

    /// Registers every struct and enum, so that a type can name one declared later.
    fn declare_shapes(&mut self, module: &MirModule) {
        for definition in &module.structs {
            let index = u32::try_from(self.structs.len()).unwrap_or(u32::MAX);
            self.struct_ids.insert(definition.name.clone(), index);
            self.structs.push(StructDef {
                name: self.strings.add(definition.name.clone()),
                fields: Vec::new(),
            });
        }
        for definition in &module.enums {
            let index = u32::try_from(self.enums.len()).unwrap_or(u32::MAX);
            self.enum_ids.insert(definition.name.clone(), index);
            self.enums.push(EnumDef {
                name: self.strings.add(definition.name.clone()),
                variants: Vec::new(),
            });
        }

        // Second pass: the shapes, now that every name resolves to an index.
        for (index, definition) in module.structs.iter().enumerate() {
            let fields = definition
                .fields
                .iter()
                .map(|field| {
                    let name = self.strings.add(field.name.clone());
                    let ty = self.ty(&field.ty);
                    FieldSchema { name, ty }
                })
                .collect();
            if let Some(slot) = self.structs.get_mut(index) {
                slot.fields = fields;
            }
        }
        for (index, definition) in module.enums.iter().enumerate() {
            let variants = definition
                .variants
                .iter()
                .map(|variant| VariantDef {
                    name: self.strings.add(variant.name.clone()),
                    fields: variant.fields.iter().map(|ty| self.ty(ty)).collect(),
                })
                .collect();
            if let Some(slot) = self.enums.get_mut(index) {
                slot.variants = variants;
            }
        }
    }

    /// Folds the module's constants and defaults into the pool.
    fn declare_constants(&mut self, module: &MirModule) {
        for definition in &module.consts {
            let value = self.constant(module, definition.value);
            let id = self.consts.add(value);
            self.constants
                .insert(definition.name.clone(), (id, definition.ty.clone()));
        }
        for definition in &module.defaults {
            let value = self.constant(module, definition.init);
            let id = self.consts.add(value);
            self.constants
                .insert(definition.name.clone(), (id, definition.ty.clone()));
            // Interned before the push: `self.defaults.push` borrows the builder mutably,
            // and so does interning.
            let name = self.strings.add(definition.name.clone());
            let ty = self.ty(&definition.ty);
            self.defaults.push(DefaultDef { name, ty, init: id });
        }

        // Where each label sits, so a `CallLabel` can name it.
        for (index, body) in module.labels.iter().enumerate() {
            self.labels.insert(
                body.name.to_string(),
                u32::try_from(index).unwrap_or(u32::MAX),
            );
        }
    }

    /// A constant, by pool id.
    pub(crate) fn constant(&mut self, module: &MirModule, id: vela_mir::ConstId) -> ByteConst {
        match module.pool.get(id) {
            Some(Const::None) | None => ByteConst::None,
            Some(Const::Bool(flag)) => ByteConst::Bool(*flag),
            Some(Const::Int(number)) => ByteConst::Int(*number),
            Some(Const::Float(number)) => ByteConst::Float(*number),
            Some(Const::Str(text)) => ByteConst::Str(self.strings.add(text.clone())),
            Some(Const::Variant { enum_name, variant }) => ByteConst::Variant {
                enum_id: self.enum_ids.get(enum_name).copied().unwrap_or(u32::MAX),
                variant: self.variant_index(module, enum_name, variant),
            },
            Some(Const::Function(name)) => ByteConst::Function(self.function_index(module, name)),
        }
    }

    /// A variant's position within its enum.
    fn variant_index(&self, module: &MirModule, enum_name: &str, variant: &str) -> u32 {
        module
            .enum_named(enum_name)
            .and_then(|definition| definition.index_of(variant))
            .map_or(u32::MAX, |id| id.0)
    }

    /// Emits one body.
    pub(crate) fn emit_body(&mut self, module: &MirModule, body: &MirBody) -> FuncDef {
        let mut emitter = Emitter::new(self, module, body);
        emitter.run();
        emitter.finish()
    }

    /// The schema index for a command variant, registering it on first use.
    ///
    /// Registered while emitting rather than collected afterwards, because the call site is
    /// where the argument types are known — and a schema whose types came from a second
    /// pass over the same statements could disagree with the ones that were compiled.
    pub(crate) fn command_schema(&mut self, kind: vela_world::CommandKind, types: &[Ty]) -> u32 {
        let name = self.strings.add(kind.as_str());
        if let Some(index) = self.cmds.iter().position(|schema| schema.name == name) {
            return u32::try_from(index).unwrap_or(u32::MAX);
        }

        let fields = kind
            .fields()
            .iter()
            .zip(types)
            .map(|(field, ty)| {
                let name = self.strings.add(*field);
                let ty = self.ty(ty);
                FieldSchema { name, ty }
            })
            .collect();

        let index = u32::try_from(self.cmds.len()).unwrap_or(u32::MAX);
        self.cmds.push(CommandSchema { name, fields });
        index
    }

    /// A function's index, by name.
    fn function_index(&self, module: &MirModule, name: &str) -> u32 {
        module
            .fns
            .iter()
            .position(|body| body.name.as_str() == name)
            .and_then(|index| u32::try_from(index).ok())
            .unwrap_or(u32::MAX)
    }

    /// A type in the type table.
    pub(crate) fn ty(&mut self, ty: &Ty) -> TypeId {
        let lowered = match ty {
            Ty::Int => ByteTy::Int,
            Ty::Float => ByteTy::Float,
            Ty::Bool => ByteTy::Bool,
            Ty::Str => ByteTy::Str,
            Ty::None => ByteTy::None,
            Ty::Unit => ByteTy::Unit,
            Ty::Optional(inner) => ByteTy::Optional(Box::new(self.byte_ty(inner))),
            Ty::List(element) => ByteTy::List(Box::new(self.byte_ty(element))),
            Ty::Map(key, value) => {
                let key = self.byte_ty(key);
                let value = self.byte_ty(value);
                ByteTy::Map(Box::new(key), Box::new(value))
            }
            Ty::Struct(name) => {
                ByteTy::Struct(self.struct_ids.get(name).copied().unwrap_or(u32::MAX))
            }
            Ty::Enum(name) => ByteTy::Enum(self.enum_ids.get(name).copied().unwrap_or(u32::MAX)),
            Ty::Fn(_, _) => ByteTy::Func(u32::MAX),
            Ty::Unknown => ByteTy::Unknown,
        };
        self.types.add(lowered)
    }

    /// A type, lowered without registering it. Used for the payload of an aggregate, which
    /// is already being registered as part of its parent.
    fn byte_ty(&mut self, ty: &Ty) -> ByteTy {
        let id = self.ty(ty);
        self.types.get(id).cloned().unwrap_or(ByteTy::Unknown)
    }
}

/// A digest of the command schemas, so that a mismatch is detectable before execution.
fn schema_digest(cmds: &[CommandSchema]) -> [u8; 32] {
    use crate::codec::fnv1a;

    let mut input = String::new();
    for command in cmds {
        input.push_str(&command.name.0.to_string());
        for field in &command.fields {
            input.push(':');
            input.push_str(&field.name.0.to_string());
            input.push('=');
            input.push_str(&field.ty.0.to_string());
        }
    }

    // FNV-1a, widened to 32 bytes by hashing the length and the text separately. Not a
    // cryptographic digest: this detects a rebuild, and a collision would mean a runtime
    // accepts a module built against a different schema — which is the thing the module's
    // own command list already catches per variant.
    let mut digest = [0u8; 32];
    let first = fnv1a(input.as_bytes());
    let second = fnv1a(&input.len().to_le_bytes());
    digest[..8].copy_from_slice(&first.to_le_bytes());
    digest[8..16].copy_from_slice(&second.to_le_bytes());
    digest[16..24].copy_from_slice(&first.rotate_left(17).to_le_bytes());
    digest[24..].copy_from_slice(&second.rotate_left(29).to_le_bytes());
    digest
}
