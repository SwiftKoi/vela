//! What a link renames, for one module.
//!
//! Every module declares names in its own scope, and a linked program has one scope. A name that
//! crosses the boundary therefore has to be prefixed with the module it belongs to — and a
//! *reference* to another module, written with an alias or a full path, has to be resolved to the
//! one module it names before it can be prefixed.

use std::collections::{BTreeMap, BTreeSet};

use vela_hir::ModuleName;

use crate::ir::{ConstId, DefaultId, LabelRef};

/// The renames one unit's names take on in the linked module.
pub(super) struct Names<'a> {
    /// The unit's module name, which prefixes everything it declares.
    module: &'a str,
    /// A written qualifier to the module it names, from this unit's `use` items.
    imports: &'a BTreeMap<String, ModuleName>,
    /// Every module in the program, so a qualifier written as a full path resolves too.
    known: &'a BTreeSet<String>,
    /// Where this unit's effects, functions, constants, and defaults start in the linked tables.
    effect_ids: &'a [u32],
    fn_base: u32,
    /// Pool ids, this unit's to the linked module's, filled while the pools are merged.
    constants: BTreeMap<u32, ConstId>,
    /// `default` ids, this unit's to the linked module's.
    defaults: BTreeMap<u32, DefaultId>,
}

impl<'a> Names<'a> {
    /// The renames for one unit.
    pub(super) fn new(
        module: &'a str,
        imports: &'a BTreeMap<String, ModuleName>,
        known: &'a BTreeSet<String>,
        effect_ids: &'a [u32],
        fn_base: u32,
    ) -> Self {
        Self {
            module,
            imports,
            known,
            effect_ids,
            fn_base,
            constants: BTreeMap::new(),
            defaults: BTreeMap::new(),
        }
    }

    /// The unit's own copy of a name.
    pub(super) fn qualified(&self, name: &str) -> String {
        format!("{}.{}", self.module, name)
    }

    /// A label reference, resolved to one qualified name in the linked label table.
    pub(super) fn label(&self, reference: &LabelRef) -> LabelRef {
        let label = match &reference.module {
            Some(qualifier) => match self.resolve(qualifier) {
                Some(module) => format!("{module}.{}", reference.label),
                // A qualifier `check` would have refused; see `program::check`. Keeping the
                // reference as written means a program that somehow got here fails on a missing
                // label rather than silently calling one with a wrong name.
                None => reference.label.clone(),
            },
            None => self.qualified(&reference.label),
        };
        LabelRef {
            module: None,
            label,
        }
    }

    /// The module a written qualifier names.
    pub(super) fn resolve(&self, qualifier: &str) -> Option<String> {
        if let Some(import) = self.imports.get(qualifier) {
            return Some(import.as_str().to_string());
        }
        self.known
            .contains(qualifier)
            .then(|| qualifier.to_string())
    }

    /// Records where a constant went in the linked pool.
    pub(super) fn record_constant(&mut self, from: u32, to: ConstId) {
        self.constants.insert(from, to);
    }

    /// A constant id in the linked pool.
    pub(super) fn constant(&self, id: ConstId) -> ConstId {
        self.constants.get(&id.0).copied().unwrap_or(id)
    }

    /// Records where a `default` went in the linked table.
    pub(super) fn record_default(&mut self, from: u32, to: DefaultId) {
        self.defaults.insert(from, to);
    }

    /// A `default` id in the linked table.
    pub(super) fn default_id(&self, id: DefaultId) -> DefaultId {
        self.defaults.get(&id.0).copied().unwrap_or(id)
    }

    /// A function's index in the linked function table.
    pub(super) fn function(&self, index: u32) -> u32 {
        self.fn_base.saturating_add(index)
    }

    /// An effect's index in the linked effect table.
    pub(super) fn effect(&self, index: u32) -> u32 {
        self.effect_ids
            .get(index as usize)
            .copied()
            .unwrap_or(index)
    }
}
