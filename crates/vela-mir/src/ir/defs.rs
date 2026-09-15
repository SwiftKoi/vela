//! Declarations, and the pools they index into.

use vela_types::Ty;

/// A local slot.
///
/// Slots are mutable and named `_0.._n`, not SSA values. That is deliberate: slot-based IR
/// produces smaller bytecode and far simpler verifier rules, and a story engine does not
/// need the optimization power of SSA (`BYTECODE.md §2`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Slot(pub u32);

/// What a slot holds.
#[derive(Clone, Debug)]
pub struct LocalDecl {
    /// The identifier it was declared with, for the debugger's variable view.
    pub name: String,
    /// Its type, which every read and write is checked against at construction.
    pub ty: Ty,
}

/// An index into a module's constant pool.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ConstId(pub u32);

/// An index into a module's `default` declarations.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DefaultId(pub u32);

/// A field's position within its struct.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FieldId(pub u32);

/// A variant's position within its enum.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct VariantId(pub u32);

/// A compile-time constant.
///
/// Aggregates are deliberately absent. A list literal is built with instructions rather
/// than stored, because a constant that contains constants would need a second pool and a
/// recursive equality — cost with no benefit, since folding a list literal's elements is
/// the same win and the list construction is cheap.
#[derive(Clone, PartialEq, Debug)]
pub enum Const {
    /// `none`
    None,
    /// A boolean.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A float.
    Float(f64),
    /// A string, with escapes resolved and interpolations not yet applied.
    Str(String),
    /// A variant with no payload, e.g. `Ending.good`.
    Variant {
        /// The enum's name.
        enum_name: String,
        /// The variant's name.
        variant: String,
    },
    /// A function used as a value: a declared `fn`, or a lifted lambda.
    Function(String),
}

/// The constants a module holds, deduplicated.
#[derive(Clone, Default, Debug)]
pub struct ConstPool {
    items: Vec<Const>,
}

impl ConstPool {
    /// Adds a constant, reusing an identical one if the pool already has it.
    pub fn add(&mut self, value: Const) -> ConstId {
        if let Some(existing) = self.items.iter().position(|item| *item == value) {
            return ConstId(u32::try_from(existing).unwrap_or(u32::MAX));
        }
        let id = ConstId(u32::try_from(self.items.len()).unwrap_or(u32::MAX));
        self.items.push(value);
        id
    }

    /// The constant at an index.
    #[must_use]
    pub fn get(&self, id: ConstId) -> Option<&Const> {
        self.items.get(id.0 as usize)
    }

    /// Every constant, in insertion order.
    #[must_use]
    pub fn items(&self) -> &[Const] {
        &self.items
    }

    /// How many constants the module has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the module has no constants.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// A field of a struct.
#[derive(Clone, Debug)]
pub struct FieldDef {
    /// The field's name.
    pub name: String,
    /// Its type.
    pub ty: Ty,
}

/// A declared `struct`.
#[derive(Clone, Debug)]
pub struct StructDef {
    /// The struct's name.
    pub name: String,
    /// Its fields, in declaration order.
    pub fields: Vec<FieldDef>,
}

impl StructDef {
    /// The index of a field by name, which is what a `FieldId` means.
    #[must_use]
    pub fn index_of(&self, field: &str) -> Option<FieldId> {
        self.fields
            .iter()
            .position(|candidate| candidate.name == field)
            .and_then(|index| u32::try_from(index).ok())
            .map(FieldId)
    }
}

/// A variant of an enum.
#[derive(Clone, Debug)]
pub struct VariantDef {
    /// The variant's name.
    pub name: String,
    /// The payload's types, in declaration order.
    pub fields: Vec<Ty>,
}

/// A declared `enum`.
#[derive(Clone, Debug)]
pub struct EnumDef {
    /// The enum's name.
    pub name: String,
    /// Its variants, in declaration order.
    pub variants: Vec<VariantDef>,
}

impl EnumDef {
    /// The index of a variant by name, which is what a `VariantId` means.
    #[must_use]
    pub fn index_of(&self, variant: &str) -> Option<VariantId> {
        self.variants
            .iter()
            .position(|candidate| candidate.name == variant)
            .and_then(|index| u32::try_from(index).ok())
            .map(VariantId)
    }
}

/// A named `const`.
#[derive(Clone, Debug)]
pub struct ConstDef {
    /// The name.
    pub name: String,
    /// Its type.
    pub ty: Ty,
    /// Its value, in the pool.
    pub value: ConstId,
}

/// A `default` — world state, and therefore part of the save schema.
#[derive(Clone, Debug)]
pub struct DefaultDef {
    /// The name.
    pub name: String,
    /// Its type.
    pub ty: Ty,
    /// Its initial value, in the pool.
    pub init: ConstId,
}

/// A declared `effect`: a capability the host provides.
#[derive(Clone, Debug)]
pub struct EffectDef {
    /// The dotted name, e.g. `rand.int`.
    pub name: String,
    /// The parameters' types.
    pub params: Vec<Ty>,
    /// What it returns.
    pub ret: Ty,
}

/// The assets a module refers to.
///
/// Collected rather than checked: whether an asset *exists* is a build question
/// (`BUILD_AND_ASSETS.md §9`), answered against the asset manifest at M9. Recording the
/// references here is what makes that answer possible without re-parsing.
#[derive(Clone, Default, Debug)]
pub struct AssetRefs {
    /// Image paths, in first-use order.
    pub images: Vec<String>,
    /// Audio paths, in first-use order.
    pub audio: Vec<String>,
}

impl AssetRefs {
    /// Records an image, keeping first-use order and no duplicates.
    pub fn add_image(&mut self, path: impl Into<String>) {
        let path = path.into();
        if !self.images.contains(&path) {
            self.images.push(path);
        }
    }

    /// Records an audio source.
    pub fn add_audio(&mut self, path: impl Into<String>) {
        let path = path.into();
        if !self.audio.contains(&path) {
            self.audio.push(path);
        }
    }
}
