//! Resolving a reference outside the compiler.
//!
//! `target_of` is the one function that decides what a reference points at, so a tool that needs the
//! answer asks *it* rather than re-deriving the rule — over a table of the session's modules, which is
//! the part a caller has to supply. Two callers need it: `vela test`, to know where a test starts, and
//! `vela analyze`, to know what a graph edge points at.
//!
//! The duplication with `vela-lsp`'s own table is deliberate and grudging: `Modules` is a trait so that
//! resolution stays a pure function of a module and whatever it names, and one implementation in
//! `vela-hir` — or one query database — would be the fix. Until then this is the smaller copy.

use std::collections::BTreeMap;
use std::rc::Rc;

use vela_compile::Session;
use vela_hir::{Collected, LabelRef, Module, ModuleName, Modules, Transfer};
use vela_span::{FileId, Span};

/// Every module in a session, by name.
pub(crate) struct Table(BTreeMap<ModuleName, Rc<Collected>>);

impl Table {
    /// Reads every module the session holds.
    pub(crate) fn of(session: &mut Session) -> Self {
        let mut modules = BTreeMap::new();
        for file in session.file_ids() {
            let collected = session.symbols(file);
            if session.module_of(file).is_some() {
                modules.insert(collected.module.name.clone(), collected);
            }
        }
        Self(modules)
    }
}

impl Modules for Table {
    fn get(&self, name: &ModuleName) -> Option<&Module> {
        self.0.get(name).map(|collected| &collected.module)
    }

    fn all(&self) -> Vec<&Module> {
        self.0.values().map(|collected| &collected.module).collect()
    }
}

/// Where a transferred-to path points, as `(module, label)`.
///
/// `None` means the path does not resolve, which the compiler has already reported as a diagnostic: a
/// caller decides what to say about a reference that points nowhere — a test fails at it, and a graph
/// simply has no edge.
pub(crate) fn target(
    session: &mut Session,
    table: &Table,
    file: FileId,
    path: &[String],
    span: Span,
) -> Option<(ModuleName, String)> {
    let collected = session.symbols(file);
    let reference = LabelRef {
        path: path.to_vec(),
        span,
        transfer: Transfer::Jump,
    };

    let (module, label) = vela_hir::target_of(&collected.module, table, &reference)?;
    Some((module.name.clone(), label.to_string()))
}
