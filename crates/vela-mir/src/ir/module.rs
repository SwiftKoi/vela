//! A whole module, compiled to MIR.

use vela_hir::ModuleName;

use crate::ir::defs::{AssetRefs, ConstDef, ConstPool, DefaultDef, EnumDef, StructDef};
use crate::ir::{Body, Const, ConstId, DefaultId, EffectDef};

/// A whole module, compiled to MIR.
#[derive(Clone, Debug)]
pub struct Module {
    /// The module's dotted name.
    pub name: ModuleName,
    /// The declared structs.
    pub structs: Vec<StructDef>,
    /// The declared enums.
    pub enums: Vec<EnumDef>,
    /// The named `const`s.
    pub consts: Vec<ConstDef>,
    /// The `default`s, which are the save schema.
    pub defaults: Vec<DefaultDef>,
    /// The functions, including lifted lambdas. [`FuncRef::Defined`] indexes this.
    pub fns: Vec<Body>,
    /// The labels, in source order.
    pub labels: Vec<Body>,
    /// Every constant the module mentions.
    pub pool: ConstPool,
    /// The declared effects, by index. [`crate::ir::FuncRef::Effect`] indexes this.
    pub effects: Vec<EffectDef>,
    /// Every asset it refers to.
    pub assets: AssetRefs,
}

impl Default for Module {
    fn default() -> Self {
        Self {
            name: ModuleName::new(""),
            structs: Vec::new(),
            enums: Vec::new(),
            consts: Vec::new(),
            defaults: Vec::new(),
            fns: Vec::new(),
            labels: Vec::new(),
            pool: ConstPool::default(),
            effects: Vec::new(),
            assets: AssetRefs::default(),
        }
    }
}

impl Module {
    /// A `default` by name.
    #[must_use]
    pub fn default_named(&self, name: &str) -> Option<(DefaultId, &DefaultDef)> {
        self.defaults
            .iter()
            .position(|candidate| candidate.name == name)
            .and_then(|index| Some((DefaultId(u32::try_from(index).ok()?), &self.defaults[index])))
    }

    /// A struct by name.
    #[must_use]
    pub fn struct_named(&self, name: &str) -> Option<&StructDef> {
        self.structs.iter().find(|candidate| candidate.name == name)
    }

    /// An enum by name.
    #[must_use]
    pub fn enum_named(&self, name: &str) -> Option<&EnumDef> {
        self.enums.iter().find(|candidate| candidate.name == name)
    }

    /// A label by name.
    #[must_use]
    pub fn label_named(&self, name: &str) -> Option<&Body> {
        self.labels
            .iter()
            .find(|candidate| candidate.name.as_str() == name)
    }

    /// Every body: functions first, then labels.
    pub fn bodies(&self) -> impl Iterator<Item = &Body> {
        self.fns.iter().chain(self.labels.iter())
    }

    /// The type of a constant pool entry, for the printer.
    #[must_use]
    pub fn const_def(&self, id: ConstId) -> Option<&Const> {
        self.pool.get(id)
    }
}
