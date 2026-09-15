//! The tracked queries.
//!
//! Each one either returns a cached value whose inputs are still current, or does the
//! work and records what it read. The recording is not optional: a query that forgets it
//! is never invalidated, and a query that records too much is only slow — so the first is
//! a correctness bug and the second is a performance one.

use std::collections::BTreeMap;
use std::rc::Rc;

use vela_diag::Diagnostic;
use vela_hir::{Collected, ModuleName};
use vela_span::FileId;
use vela_syntax::ParseResult;

use crate::memo::Cached;
use crate::session::state::{Compiled, Lookup, Query, Session};

impl Session {
    /// Every diagnostic in the session, ordered by file and then by position.
    pub fn diagnostics(&mut self) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        for file in self.file_ids() {
            if self.module_of(file).is_some() {
                out.extend(self.check(file).iter().cloned());
            } else {
                // Not a module, so there is nothing to resolve against; its own parse
                // diagnostics are all it has.
                out.extend(self.parse(file).diagnostics.iter().cloned());
            }
        }

        // Whole-program findings last: they are about the shape of the story rather than
        // about one file, so ordering them by file would be arbitrary.
        out.extend(self.analyse().iter().cloned());
        out
    }

    /// Parses one file, reusing the previous result while its text is unchanged.
    pub fn parse(&mut self, file: FileId) -> Rc<ParseResult> {
        if let Some(cached) = self.parsed.get(&file)
            && self.deps_are_current(&cached.deps)
        {
            // Clone the handle before recording: recording borrows the session mutably,
            // and the cache lookup above still holds it.
            let value = Rc::clone(&cached.value);
            self.record_hit(file);
            return value;
        }

        *self.runs.entry(Query::Parse).or_default() += 1;

        let (value, deps) = self.collect_deps(|session| session.parse_uncached(file));
        self.parsed.insert(
            file,
            Cached {
                deps,
                value: Rc::clone(&value),
            },
        );
        value
    }

    /// Collects one module's definitions and label graph.
    ///
    /// Reads only this module's file, which is what makes the query invalidated by exactly
    /// one edit.
    pub fn symbols(&mut self, file: FileId) -> Rc<Collected> {
        if let Some(cached) = self.symbols.get(&file)
            && self.deps_are_current(&cached.deps)
        {
            // Clone the handle before recording: recording borrows the session mutably,
            // and the cache lookup above still holds it.
            let value = Rc::clone(&cached.value);
            self.record_hit(file);
            return value;
        }

        *self.runs.entry(Query::Symbols).or_default() += 1;

        let (value, deps) = self.collect_deps(|session| session.symbols_uncached(file));
        self.symbols.insert(
            file,
            Cached {
                deps,
                value: Rc::clone(&value),
            },
        );
        value
    }

    /// Resolves one module's references, reporting everything that does not land.
    ///
    /// Reads the modules this one *names*, and no others. That is the whole reason the
    /// resolution pass takes a lookup rather than the whole program: a module that nothing
    /// imports cannot be disturbed by an edit elsewhere, so it is not re-checked.
    pub fn check(&mut self, file: FileId) -> Rc<Vec<Diagnostic>> {
        if let Some(cached) = self.checked.get(&file)
            && self.deps_are_current(&cached.deps)
        {
            // Clone the handle before recording: recording borrows the session mutably,
            // and the cache lookup above still holds it.
            let value = Rc::clone(&cached.value);
            self.record_hit(file);
            return value;
        }

        *self.runs.entry(Query::Check).or_default() += 1;

        let (value, deps) = self.collect_deps(|session| session.check_uncached(file));
        self.checked.insert(
            file,
            Cached {
                deps,
                value: Rc::clone(&value),
            },
        );
        value
    }

    /// Lowers one file to MIR.
    ///
    /// Reads only this file, because lowering is a per-module question: the environment
    /// comes from the module's own declarations, and a label in another module is a
    /// *reference* that lowering records rather than resolves.
    pub fn mir(&mut self, file: FileId) -> Rc<Compiled> {
        if let Some(cached) = self.mirrors.get(&file)
            && self.deps_are_current(&cached.deps)
        {
            // Clone the handle before recording: recording borrows the session mutably,
            // and the cache lookup above still holds it.
            let value = Rc::clone(&cached.value);
            self.record_hit(file);
            return value;
        }

        *self.runs.entry(Query::Mir).or_default() += 1;

        let (value, deps) = self.collect_deps(|session| session.mir_uncached(file));
        self.mirrors.insert(
            file,
            Cached {
                deps,
                value: Rc::clone(&value),
            },
        );
        value
    }

    /// Reports labels no path from an entry point can reach.
    ///
    /// Reads *every* module, which is what a whole-program question costs: a label is
    /// reachable because something somewhere transfers to it, so no subset of modules can
    /// answer it. Every module therefore becomes a dependency, and an edit anywhere
    /// re-runs this.
    pub fn analyse(&mut self) -> Rc<Vec<Diagnostic>> {
        if let Some(cached) = &self.analysed
            && self.deps_are_current(&cached.deps)
        {
            let value = Rc::clone(&cached.value);
            return value;
        }

        *self.runs.entry(Query::Analyse).or_default() += 1;

        let (value, deps) = self.collect_deps(|session| session.analyse_uncached());
        self.analysed = Some(Cached {
            deps,
            value: Rc::clone(&value),
        });
        value
    }

    /// Does the actual work, and records the file it read.
    fn parse_uncached(&mut self, file: FileId) -> Rc<ParseResult> {
        self.scratch.push(file);
        let text = self.sources.file(file).text();
        Rc::new(vela_syntax::parse(file, text))
    }

    fn symbols_uncached(&mut self, file: FileId) -> Rc<Collected> {
        self.scratch.push(file);
        let parsed = self.parse(file);

        // A file that is not a module has no dotted name. Only reachable if a caller asks
        // for the symbols of one; `diagnostics` routes those to `parse` instead.
        let name = self
            .module_of(file)
            .cloned()
            .unwrap_or_else(|| ModuleName::new(self.sources.file(file).name().to_string()));

        Rc::new(vela_hir::collect(name, file, &parsed.program))
    }

    fn check_uncached(&mut self, file: FileId) -> Rc<Vec<Diagnostic>> {
        // Everything the phases below have to say, in one list. Parse first, because a
        // syntax error means the later phases saw a partly-invented tree and their
        // findings about it are less trustworthy.
        let parsed = self.parse(file);
        let collected = self.symbols(file);

        // Resolving an alias to a file, then to that module's symbols, is what records
        // the imported file as a dependency.
        let imported_files: Vec<(ModuleName, FileId)> = collected
            .module
            .aliases
            .values()
            .filter_map(|import| {
                self.module_files
                    .get(&import.module)
                    .map(|&id| (import.module.clone(), id))
            })
            .collect();

        let mut imported: BTreeMap<ModuleName, Rc<Collected>> = BTreeMap::new();
        for (name, id) in imported_files {
            imported.insert(name, self.symbols(id));
        }

        let mut diagnostics = parsed.diagnostics.clone();
        diagnostics.extend(collected.diagnostics.clone());
        diagnostics.extend(vela_hir::resolve(&collected.module, &Lookup(&imported)));
        diagnostics.extend(vela_hir::resolve_names(&collected.module, &parsed.program));

        // Types are a per-module question: the environment comes from this module's own
        // declarations, so no edit elsewhere can change the answer. A qualified type name
        // from another module lowers to unknown rather than being guessed at.
        //
        // The environment and lowering's findings come from the `mir` query, so that
        // "everything the compiler knows about this file" is one answer rather than two
        // that could disagree.
        let compiled = self.mir(file);
        diagnostics.extend(compiled.diagnostics.clone());
        diagnostics.extend(vela_types::check(&parsed.program, &compiled.env));
        diagnostics.sort_by_key(|diagnostic| diagnostic.primary.span.start());
        Rc::new(diagnostics)
    }

    fn mir_uncached(&mut self, file: FileId) -> Rc<Compiled> {
        self.scratch.push(file);
        let parsed = self.parse(file);

        let name = self
            .module_of(file)
            .cloned()
            .unwrap_or_else(|| ModuleName::new(self.sources.file(file).name().to_string()));

        let (env, mut diagnostics) = vela_types::Env::build(&parsed.program);
        let lowered = vela_mir::lower(&name, &parsed.program, &env);
        diagnostics.extend(lowered.diagnostics);

        Rc::new(Compiled {
            env,
            module: lowered.module,
            diagnostics,
        })
    }

    fn analyse_uncached(&mut self) -> Rc<Vec<Diagnostic>> {
        // Collected into a local first: the queries below borrow `self` mutably, and iterating
        // the map directly would hold an immutable borrow of it across the call.
        let files: Vec<FileId> = self.module_files.values().copied().collect();

        let mut program: BTreeMap<ModuleName, Rc<Collected>> = BTreeMap::new();
        for file in &files {
            let collected = self.symbols(*file);
            program.insert(collected.module.name.clone(), collected);
        }

        let entries = self.entries.clone();
        let mut out = vela_hir::unreachable_labels(&Lookup(&program), &entries);

        // `@"path"` literals, against the manifest the driver supplied. Skipped entirely when
        // there is none — a file compiled on its own has no assets to be wrong about.
        if let Some(known) = self.assets.clone() {
            let mut missing = Vec::new();
            for file in &files {
                let parsed = self.parse(*file);
                missing.extend(crate::assets::missing_assets(&parsed.paths, &known));
            }

            // Sorted among themselves, in source order, so a file with several bad literals
            // reads in the order an author would fix them.
            //
            // *Among themselves* is the point: a sort over the combined list would also move
            // the reachability findings, making each pass's order depend on the other's. The
            // two are independent questions and are appended as such — which is the same shape
            // `diagnostics` already uses, where the whole-program answers come last.
            missing.sort_by_key(|diagnostic| {
                (
                    diagnostic.primary.span.file().as_raw(),
                    diagnostic.primary.span.start(),
                )
            });
            out.extend(missing);
        }

        Rc::new(out)
    }

    /// Records that a cached query still read this file.
    ///
    /// A cache *hit* must report its input just as a miss does. Without this, a caller
    /// that found the answer already computed records no dependency on the file it came
    /// from — so the caller is never invalidated when that file changes. The entry it
    /// reads is still correct; the caller's own entry is the one that goes stale.
    fn record_hit(&mut self, file: FileId) {
        self.scratch.push(file);
    }
}
