//! Whole-program reachability.
//!
//! Unlike collecting and checking, this cannot be answered one module at a time: a label
//! is reachable because *something somewhere* transfers to it. That is why it is a
//! separate pass with its own lookup, and why an edit to any module invalidates it —
//! pretending otherwise would mean reporting labels as dead when they are not.

use std::collections::{BTreeSet, VecDeque};

use vela_diag::Diagnostic;

use crate::error;
use crate::module::ModuleName;
use crate::resolve::{Modules, target_of};

/// A label to start reaching from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    /// The module the label is in.
    pub module: ModuleName,
    /// The label's own name.
    pub label: String,
}

impl Entry {
    /// An entry named `module.label`, as written in a manifest.
    #[must_use]
    pub fn parse(dotted: &str) -> Option<Self> {
        let (module, label) = dotted.rsplit_once('.')?;
        (!module.is_empty() && !label.is_empty()).then(|| Self {
            module: ModuleName::new(module),
            label: label.to_string(),
        })
    }
}

/// Reports labels that no path from an entry point can reach.
///
/// With no entries there is no notion of reachable, so nothing is reported: warning about
/// every label in a project would be noise, and noise is what teaches people to ignore
/// warnings.
#[must_use]
pub fn unreachable_labels(modules: &impl Modules, entries: &[Entry]) -> Vec<Diagnostic> {
    if entries.is_empty() {
        return Vec::new();
    }

    let reached = reachable(modules, entries);
    let mut diagnostics = Vec::new();

    for module in modules.all() {
        for node in &module.story.nodes {
            let key = (module.name.clone(), node.name.clone());
            if !reached.contains(&key) {
                diagnostics.push(error::unreachable_label(
                    &node.name,
                    node.span,
                    module.name.as_str(),
                ));
            }
        }
    }

    diagnostics
}

/// Every `(module, label)` reachable from the entries.
fn reachable(modules: &impl Modules, entries: &[Entry]) -> BTreeSet<(ModuleName, String)> {
    let mut reached: BTreeSet<(ModuleName, String)> = BTreeSet::new();
    let mut queue: VecDeque<(ModuleName, String)> = VecDeque::new();

    for entry in entries {
        let key = (entry.module.clone(), entry.label.clone());
        if reached.insert(key.clone()) {
            queue.push_back(key);
        }
    }

    while let Some((module_name, label)) = queue.pop_front() {
        let Some(module) = modules.get(&module_name) else {
            continue;
        };

        for reference in module.story.targets_of(&label) {
            let Some((target, name)) = target_of(module, modules, reference) else {
                continue;
            };
            let key = (target.name.clone(), name.to_string());
            if reached.insert(key.clone()) {
                queue.push_back(key);
            }
        }
    }

    reached
}
