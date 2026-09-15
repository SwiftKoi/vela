//! Putting a program's modules together.

use std::collections::{BTreeMap, BTreeSet};

use vela_hir::ModuleName;

use crate::ir::{
    ConstDef, DefaultDef, DefaultId, EffectDef, EnumDef, FieldDef, Module, StructDef, Terminator,
    VariantDef,
};

use super::error::LinkError;
use super::names::Names;
use super::rewrite;

/// The name the linked module carries, for a disassembly or a diagnostic to print.
///
/// Not a module: a linked program is the whole program, and naming it after one of its modules
/// would say that module is special when the point is that none of them is.
const PROGRAM: &str = "program";

/// One module of a program, with what it names.
pub struct Unit<'a> {
    /// The module, as it was lowered.
    pub module: &'a Module,
    /// A written qualifier to the module it names: the `use` alias, or the full path when there is
    /// no alias. Without it a reference like `forest.clearing` cannot be told from one to a module
    /// that happens to be called `forest`.
    pub imports: &'a BTreeMap<String, ModuleName>,
}

/// Links a program's modules into one, or says why they cannot be.
///
/// Units are ordered by module name *inside*, so the image does not depend on the order a driver
/// happened to read the files in — which is what lets two builds of one project be byte-identical
/// (`BUILD_AND_ASSETS.md §7`).
///
/// # Errors
///
/// Fails on a duplicate module or `default`, a label reference to a module the program does not
/// have, or two modules disagreeing about an effect's signature.
pub fn link(units: &[Unit<'_>]) -> Result<Module, LinkError> {
    let mut ordered: Vec<&Unit<'_>> = units.iter().collect();
    ordered.sort_by(|a, b| a.module.name.as_str().cmp(b.module.name.as_str()));

    let known: BTreeSet<String> = ordered
        .iter()
        .map(|unit| unit.module.name.as_str().to_string())
        .collect();
    check(&ordered, &known)?;

    let (effects, effect_index) = merge_effects(&ordered)?;
    let mut linked = Module {
        name: ModuleName::new(PROGRAM),
        effects,
        ..Module::default()
    };

    for unit in &ordered {
        merge(unit, &known, &effect_index, &mut linked);
    }
    Ok(linked)
}

/// Merges one unit into the linked module.
fn merge(
    unit: &Unit<'_>,
    known: &BTreeSet<String>,
    effect_index: &BTreeMap<String, u32>,
    linked: &mut Module,
) {
    let module = unit.module.name.as_str();
    let effect_ids: Vec<u32> = unit
        .module
        .effects
        .iter()
        .map(|effect| effect_index.get(&effect.name).copied().unwrap_or(0))
        .collect();
    let mut names = Names::new(
        module,
        unit.imports,
        known,
        &effect_ids,
        u32::try_from(linked.fns.len()).unwrap_or(u32::MAX),
    );

    merge_pool(unit, &mut names, linked);
    merge_tables(unit, &mut names, linked);
    merge_bodies(unit, &names, linked);
}

/// The constant pool, first: every constant id in the unit — including the ones inside its
/// `default`s, `const`s, and bodies — is translated through the map this fills.
fn merge_pool(unit: &Unit<'_>, names: &mut Names<'_>, linked: &mut Module) {
    for (index, item) in unit.module.pool.items().iter().enumerate() {
        let id = linked.pool.add(rewrite::constant(item, names));
        names.record_constant(u32::try_from(index).unwrap_or(u32::MAX), id);
    }
}

/// The declarations: `default`s, `const`s, structs, and enums.
///
/// A `default` keeps its own name — world state is global, so it cannot be namespaced, which is
/// why two modules declaring one is refused rather than renamed. Everything else takes its
/// module's prefix, so two modules may each have their own `Route`.
fn merge_tables(unit: &Unit<'_>, names: &mut Names<'_>, linked: &mut Module) {
    for (index, definition) in unit.module.defaults.iter().enumerate() {
        let merged = DefaultDef {
            name: definition.name.clone(),
            ty: rewrite::ty(&definition.ty, names),
            init: names.constant(definition.init),
        };
        linked.defaults.push(merged);
        names.record_default(
            u32::try_from(index).unwrap_or(u32::MAX),
            DefaultId(u32::try_from(linked.defaults.len() - 1).unwrap_or(u32::MAX)),
        );
    }

    for definition in &unit.module.consts {
        linked.consts.push(ConstDef {
            name: names.qualified(&definition.name),
            ty: rewrite::ty(&definition.ty, names),
            value: names.constant(definition.value),
        });
    }

    for definition in &unit.module.structs {
        linked.structs.push(StructDef {
            name: names.qualified(&definition.name),
            fields: definition
                .fields
                .iter()
                .map(|field| FieldDef {
                    name: field.name.clone(),
                    ty: rewrite::ty(&field.ty, names),
                })
                .collect(),
        });
    }

    for definition in &unit.module.enums {
        linked.enums.push(EnumDef {
            name: names.qualified(&definition.name),
            variants: definition
                .variants
                .iter()
                .map(|variant| VariantDef {
                    name: variant.name.clone(),
                    fields: variant
                        .fields
                        .iter()
                        .map(|field| rewrite::ty(field, names))
                        .collect(),
                })
                .collect(),
        });
    }
}

/// The bodies, and the assets they name.
///
/// Functions before labels: `FuncRef::Defined` indexes the function table, and `Names` was built
/// with this unit's function base already.
fn merge_bodies(unit: &Unit<'_>, names: &Names<'_>, linked: &mut Module) {
    for body in &unit.module.fns {
        linked.fns.push(rewrite::body(body, names));
    }
    for body in &unit.module.labels {
        linked.labels.push(rewrite::body(body, names));
    }

    for path in &unit.module.assets.images {
        linked.assets.add_image(path.clone());
    }
    for path in &unit.module.assets.audio {
        linked.assets.add_audio(path.clone());
    }
}

/// Refuses a program whose modules cannot be told apart, or that names a module it does not have.
fn check(units: &[&Unit<'_>], known: &BTreeSet<String>) -> Result<(), LinkError> {
    let mut modules: BTreeSet<&str> = BTreeSet::new();
    let mut defaults: BTreeMap<&str, &str> = BTreeMap::new();

    for unit in units {
        let module = unit.module.name.as_str();
        if !modules.insert(module) {
            return Err(LinkError::DuplicateModule {
                name: module.to_string(),
            });
        }
        for definition in &unit.module.defaults {
            if let Some(first) = defaults.insert(definition.name.as_str(), module) {
                return Err(LinkError::DuplicateDefault {
                    name: definition.name.clone(),
                    modules: (first.to_string(), module.to_string()),
                });
            }
        }

        let names = Names::new(module, unit.imports, known, &[], 0);
        for body in unit.module.bodies() {
            for block in &body.blocks {
                if let Some(reference) = target_of(&block.term)
                    && let Some(qualifier) = &reference.module
                    && names.resolve(qualifier).is_none()
                {
                    return Err(LinkError::UnknownModule {
                        qualifier: qualifier.clone(),
                        module: module.to_string(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// The label a terminator transfers to, if it transfers to one.
fn target_of(term: &Terminator) -> Option<&crate::ir::LabelRef> {
    match term {
        Terminator::JumpLabel(target) => Some(target),
        Terminator::CallLabel { target, .. } => Some(target),
        _ => None,
    }
}

/// The effect table, merged by name.
///
/// An effect is a capability the host provides, named once for the whole engine, so two modules
/// declaring `rand.int` mean the same thing — unless they disagree about its signature, which is a
/// disagreement about what the host is being asked for.
fn merge_effects(
    units: &[&Unit<'_>],
) -> Result<(Vec<EffectDef>, BTreeMap<String, u32>), LinkError> {
    let mut table: Vec<EffectDef> = Vec::new();
    let mut index: BTreeMap<String, u32> = BTreeMap::new();
    let mut owner: BTreeMap<String, String> = BTreeMap::new();

    for unit in units {
        let module = unit.module.name.as_str();
        for effect in &unit.module.effects {
            match index.get(&effect.name) {
                Some(&existing) => {
                    let merged = &table[existing as usize];
                    if merged.params != effect.params || merged.ret != effect.ret {
                        return Err(LinkError::EffectMismatch {
                            name: effect.name.clone(),
                            modules: (
                                owner.get(&effect.name).cloned().unwrap_or_default(),
                                module.to_string(),
                            ),
                        });
                    }
                }
                None => {
                    index.insert(
                        effect.name.clone(),
                        u32::try_from(table.len()).unwrap_or(u32::MAX),
                    );
                    owner.insert(effect.name.clone(), module.to_string());
                    table.push(effect.clone());
                }
            }
        }
    }
    Ok((table, index))
}
