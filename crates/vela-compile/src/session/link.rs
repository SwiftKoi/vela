//! Linking a session's modules into one program.

use std::collections::BTreeMap;

use vela_hir::ModuleName;
use vela_mir::{LinkError, Module, Unit, link};
use vela_span::FileId;

use crate::session::state::Session;

impl Session {
    /// Links every module in the session into one program.
    ///
    /// This is what a *run* uses: `vela run` and `vela build` both ask for the whole program,
    /// because a story written across files is one program by the time anything executes it
    /// (`vela_mir::link`).
    ///
    /// Deliberately **not** a tracked query. Linking reads every module, so its only dependency
    /// would be all of them, and it is asked for once per command rather than once per keystroke —
    /// a cache whose invalidation is "any file changed" is the whole-program analysis's cost
    /// without its benefit.
    ///
    /// # Errors
    ///
    /// Fails when the modules conflict: a duplicate `default`, a label reference to a module the
    /// program does not have, or two modules disagreeing about an effect's signature.
    pub fn linked(&mut self) -> Result<Module, LinkError> {
        // Every module, in a stable order so a conflict is reported the same way twice. `link`
        // sorts again for the image itself; this order is only for the work.
        let mut files: Vec<FileId> = self
            .file_ids()
            .into_iter()
            .filter(|file| self.module_of(*file).is_some())
            .collect();
        files.sort_by_key(|file| self.module_name(*file));

        // Kept alive together: a `Unit` borrows both the lowered module and the alias table.
        let lowered: Vec<_> = files.iter().map(|file| self.mir(*file)).collect();
        let collected: Vec<_> = files.iter().map(|file| self.symbols(*file)).collect();
        let imports: Vec<BTreeMap<String, ModuleName>> = collected
            .iter()
            .map(|symbols| {
                symbols
                    .module
                    .aliases
                    .iter()
                    .map(|(alias, import)| (alias.clone(), import.module.clone()))
                    .collect()
            })
            .collect();

        let units: Vec<Unit<'_>> = lowered
            .iter()
            .zip(&imports)
            .map(|(compiled, imports)| Unit {
                module: &compiled.module,
                imports,
            })
            .collect();
        link(&units)
    }

    /// A module file's dotted name, for ordering.
    fn module_name(&self, file: FileId) -> String {
        self.module_of(file)
            .map_or_else(String::new, |name| name.as_str().to_string())
    }
}
