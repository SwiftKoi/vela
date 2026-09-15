//! Resolving label references.

use vela_diag::Diagnostic;

use crate::collect::Module;
use crate::error;
use crate::module::ModuleName;
use crate::story::LabelRef;

/// Where to find the other modules of a program.
///
/// A trait rather than a concrete program, so that resolution stays a pure function of a
/// module and whatever it names. The driver decides which modules are in scope — and
/// that choice is what makes a query's recorded dependencies honest, because only the
/// modules actually consulted are read.
pub trait Modules {
    /// The module with this name, if the program has one.
    fn get(&self, name: &ModuleName) -> Option<&Module>;

    /// Every module in the program, for whole-program questions.
    ///
    /// Defaults to none. A lookup carrying only what one module names cannot answer a
    /// whole-program question, and saying so is better than answering it wrongly.
    fn all(&self) -> Vec<&Module> {
        Vec::new()
    }
}

/// The module and label a reference names, when both exist.
///
/// The single place that decides what a reference points at, so checking and reachability
/// cannot disagree about it — two traversals with their own resolution rules would drift
/// the moment one of them was fixed.
#[must_use]
pub fn target_of<'a>(
    module: &'a Module,
    modules: &'a impl Modules,
    reference: &'a LabelRef,
) -> Option<(&'a Module, &'a str)> {
    let label = reference.label();

    match reference.qualifier() {
        None => module.has_label(label).then_some((module, label)),
        Some(qualifier) => {
            let target = lookup(module, modules, &qualifier)?;
            target.has_label(label).then_some((target, label))
        }
    }
}

/// Resolves a module's label references, reporting the ones that do not land.
#[must_use]
pub fn resolve(module: &Module, modules: &impl Modules) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // An import that names nothing would otherwise do nothing at all: the alias resolves
    // to no module, and every qualified reference through it then reports a missing `use`,
    // which is the wrong diagnosis and points at the wrong line.
    for import in module.aliases.values() {
        if modules.get(&import.module).is_none() {
            diagnostics.push(error::unknown_module(import.module.as_str(), import.span));
        }
    }

    for node in &module.story.nodes {
        for target in &node.targets {
            if let Some(diagnostic) = resolve_target(module, modules, target) {
                diagnostics.push(diagnostic);
            }
        }
    }

    diagnostics
}

/// Resolves one label reference, or explains why it does not resolve.
fn resolve_target(
    module: &Module,
    modules: &impl Modules,
    target: &LabelRef,
) -> Option<Diagnostic> {
    let Some(qualifier) = target.qualifier() else {
        // Unqualified, so it must be a label in this module. There is deliberately no
        // global namespace to fall back on — a flat one is what `LANGUAGE.md §6` breaks
        // with, because a typo cannot be caught and a rename cannot be done safely.
        if module.has_label(target.label()) {
            return None;
        }
        return Some(error::undefined_label(
            &target.dotted(),
            target.span,
            module.name.as_str(),
            &label_names(module),
        ));
    };

    let Some(target_module) = lookup(module, modules, &qualifier) else {
        return Some(error::not_imported(&qualifier, target.span));
    };

    if target_module.has_label(target.label()) {
        return None;
    }
    Some(error::undefined_label(
        &target.dotted(),
        target.span,
        target_module.name.as_str(),
        &label_names(target_module),
    ))
}

/// A module's label names, in source order.
///
/// Owned rather than borrowed because the diagnostic outlives the borrow of the module it
/// was built from, and a suggestion that keeps a module alive is a suggestion the language
/// server cannot make.
fn label_names(module: &Module) -> Vec<String> {
    module
        .story
        .nodes
        .iter()
        .map(|node| node.name.clone())
        .collect()
}

/// Finds the module a qualifier names: an alias if one was declared, otherwise the
/// module's own dotted name, which `use chapters.forest` makes usable as written.
fn lookup<'a>(module: &Module, modules: &'a impl Modules, qualifier: &str) -> Option<&'a Module> {
    match module.aliases.get(qualifier) {
        Some(import) => modules.get(&import.module),
        None => modules.get(&ModuleName::new(qualifier)),
    }
}
