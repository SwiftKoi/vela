//! The session's state: files, revisions, and the machinery every query shares.
//!
//! The queries themselves live in `queries.rs`; this file holds what they operate on.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use vela_diag::Diagnostic;
use vela_hir::{Collected, Entry, Module, ModuleName, Modules};
use vela_span::{FileId, SourceMap};
use vela_syntax::ParseResult;

use crate::memo::{Cached, Deps};

/// A tracked query, named so its executions can be counted.
///
/// Tests assert on these counts to prove that an edit re-runs the queries it must and
/// *only* those. Without observable counts, "incremental" is a claim rather than a fact.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Query {
    /// Parse one file into a syntax tree.
    Parse,
    /// Collect one module's definitions and label graph.
    Symbols,
    /// Resolve one module's references against the modules it names.
    Check,
    /// Find labels no path from an entry point can reach.
    Analyse,
    /// Lower one file to MIR.
    Mir,
}

impl Query {
    /// The query's name, for test output.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Parse => "parse",
            Self::Symbols => "symbols",
            Self::Check => "check",
            Self::Analyse => "analyse",
            Self::Mir => "mir",
        }
    }
}

/// A file's environment and the MIR it lowered to.
///
/// The environment travels with the module because lowering *needs* it and because keeping
/// it inside the same cached value is what makes `mir` one query rather than two — a
/// separate environment query would be its own memoized value, and the two could be
/// invalidated at different times.
pub struct Compiled {
    /// The module's declarations.
    pub env: vela_types::Env,
    /// The lowered module.
    pub module: vela_mir::Module,
    /// What building the environment and lowering the module had to say.
    ///
    /// Carried here rather than rebuilt by `check`, which needs the environment anyway.
    /// Building it twice would risk the two disagreeing, and a `default` whose initialiser
    /// the checker accepts and lowering rejects is exactly the gap this closes.
    pub diagnostics: Vec<Diagnostic>,
}

/// One source file's mutable state.
pub(crate) struct FileState {
    /// Bumped on every edit. Never decreases, so a stale dependency is detectable.
    revision: u64,
    /// The module this file declares, when its path says it is one.
    module: Option<ModuleName>,
}

/// A compilation session.
pub struct Session {
    /// Every file, and the spans within them.
    pub(crate) sources: SourceMap,
    /// Per-file state, in id order.
    files: Vec<FileState>,
    /// Files by their driver-given name, so re-setting one keeps its id.
    by_name: BTreeMap<String, FileId>,
    /// Memoized parses.
    pub(crate) parsed: BTreeMap<FileId, Cached<Rc<ParseResult>>>,
    /// Memoized module symbols.
    pub(crate) symbols: BTreeMap<FileId, Cached<Rc<Collected>>>,
    /// Memoized checks.
    pub(crate) checked: BTreeMap<FileId, Cached<Rc<Vec<Diagnostic>>>>,
    /// Memoized lowerings.
    pub(crate) mirrors: BTreeMap<FileId, Cached<Rc<Compiled>>>,
    /// Which file holds each module, so a `use` can be turned into a file to read.
    pub(crate) module_files: BTreeMap<ModuleName, FileId>,
    /// Where reachability starts. Supplied by the driver from the project manifest.
    pub(crate) entries: Vec<Entry>,
    /// The sources every `@"path"` literal must name, or `None` when there is no manifest.
    ///
    /// `None` is not "no assets" — a project whose `assets/` is empty still has a manifest, and
    /// every literal in it is `E7001`. `None` means the driver has no manifest to check
    /// against, which is the case when a single file is compiled on its own; inventing an
    /// empty set there would turn "compiled out of its project" into "referenced a missing
    /// asset".
    pub(crate) assets: Option<BTreeSet<String>>,
    /// The whole-program analysis, which has no per-file key.
    pub(crate) analysed: Option<Cached<Rc<Vec<Diagnostic>>>>,
    /// Files read while the query currently executing has been running.
    pub(crate) scratch: Vec<FileId>,
    /// How deep the query stack is, so the outermost one can reset the scratch.
    pub(crate) depth: usize,
    /// How many times each query has actually run. A cache hit is not a run.
    pub(crate) runs: BTreeMap<Query, usize>,
}

impl Session {
    /// Creates an empty session.
    pub fn new() -> Self {
        Self {
            sources: SourceMap::new(),
            files: Vec::new(),
            by_name: BTreeMap::new(),
            parsed: BTreeMap::new(),
            symbols: BTreeMap::new(),
            checked: BTreeMap::new(),
            mirrors: BTreeMap::new(),
            module_files: BTreeMap::new(),
            entries: Vec::new(),
            assets: None,
            analysed: None,
            scratch: Vec::new(),
            depth: 0,
            runs: BTreeMap::new(),
        }
    }

    /// Adds a file, or replaces its text when the name is already known.
    ///
    /// Replacing keeps the file's id, so everything keyed on it — memoized results,
    /// spans inside diagnostics already produced — keeps referring to the same file.
    /// Handing out a new id on every edit would silently invalidate the whole cache.
    pub fn set_file(&mut self, name: impl Into<String>, text: impl Into<String>) -> FileId {
        let name = name.into();

        if let Some(&id) = self.by_name.get(&name) {
            self.sources.set_text(id, text);
            self.files[id.as_raw() as usize].revision += 1;
            return id;
        }

        let id = self.sources.add(name.clone(), text);
        let module = ModuleName::from_path(std::path::Path::new(&name));
        if let Some(module) = &module {
            self.module_files.insert(module.clone(), id);
        }

        self.files.push(FileState {
            revision: 1,
            module,
        });
        self.by_name.insert(name, id);
        id
    }

    /// The sources, for rendering diagnostics.
    pub fn sources(&self) -> &SourceMap {
        &self.sources
    }

    /// The files in the session, in insertion order.
    pub fn file_ids(&self) -> Vec<FileId> {
        (0..self.files.len() as u32).map(FileId::from_raw).collect()
    }

    /// The module a file declares, when its path says it is one.
    pub fn module_of(&self, file: FileId) -> Option<&ModuleName> {
        self.files
            .get(file.as_raw() as usize)
            .and_then(|state| state.module.as_ref())
    }

    /// How many times a query has actually *run*. A cache hit is not a run.
    pub fn runs(&self, query: Query) -> usize {
        self.runs.get(&query).copied().unwrap_or(0)
    }

    /// Sets the labels reachability starts from.
    ///
    /// A project setting, so the driver supplies it from `vela.toml`. Without one there is
    /// no notion of reachable, and the analysis reports nothing rather than everything.
    pub fn set_entries(&mut self, entries: Vec<Entry>) {
        self.entries = entries;
    }

    /// Sets the entry point from its `module.label` spelling, reporting whether it parsed.
    ///
    /// The driver reads this spelling from a manifest, so parsing it belongs here rather
    /// than being reimplemented — and differently — by every caller.
    pub fn set_entry(&mut self, dotted: &str) -> bool {
        match Entry::parse(dotted) {
            Some(entry) => {
                self.entries = vec![entry];
                true
            }
            None => false,
        }
    }

    /// Sets the asset sources every `@"path"` literal is checked against.
    ///
    /// The set is the *manifest's* — one source path per asset, relative to the assets root —
    /// and it is a project setting like the entry point, so the driver supplies it. Passing an
    /// empty list is a project with no assets, which is not the same as passing nothing.
    ///
    /// Invalidates the whole-program analysis explicitly. The analysis reads this set directly
    /// rather than through a tracked file, so it has no revision to compare and cannot be
    /// invalidated by the usual mechanism — the invalidation is here instead, where the change
    /// happens, rather than relying on a caller to remember.
    pub fn set_assets(&mut self, sources: Vec<String>) {
        self.assets = Some(sources.into_iter().collect());
        self.analysed = None;
    }

    /// Runs a query body and collects the inputs it read.
    ///
    /// The closure takes the session as a *parameter* rather than capturing it, which is
    /// what lets a query body call other queries while this borrow is live.
    ///
    /// Entries are *read*, never taken: a nested query must add to its caller's
    /// dependencies as well as its own. Removing them instead leaves the outer query with
    /// an empty set, which is always considered current — so it is never invalidated and
    /// silently serves a stale result. That failure is invisible until an edit stops
    /// showing up, which is exactly how this was found.
    pub(crate) fn collect_deps<T>(&mut self, body: impl FnOnce(&mut Self) -> T) -> (T, Deps) {
        if self.depth == 0 {
            self.scratch.clear();
        }

        let mark = self.scratch.len();
        self.depth += 1;
        let value = body(self);
        self.depth -= 1;

        // Deduplicated: a file can legitimately be read by several nested queries.
        let touched: BTreeSet<FileId> = self.scratch[mark..].iter().copied().collect();
        let deps = touched
            .into_iter()
            .map(|file| (file, self.revision(file)))
            .collect();
        (value, deps)
    }

    /// The current revision of a file.
    pub(crate) fn revision(&self, file: FileId) -> u64 {
        self.files
            .get(file.as_raw() as usize)
            .map_or(0, |state| state.revision)
    }

    /// Whether every recorded input still has the revision it had.
    pub(crate) fn deps_are_current(&self, deps: &Deps) -> bool {
        deps.iter()
            .all(|(file, revision)| self.revision(*file) == *revision)
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// The modules a query may consult.
///
/// A `check` fills it with the modules one module names, so only those are read and only
/// those become dependencies. `analyse` fills it with every module, because reachability
/// cannot be answered from a subset — `all` returns what the lookup holds, so the caller
/// decides which question it is answering by what it puts in.
pub(crate) struct Lookup<'a>(pub(crate) &'a BTreeMap<ModuleName, Rc<Collected>>);

impl Modules for Lookup<'_> {
    fn get(&self, name: &ModuleName) -> Option<&Module> {
        self.0.get(name).map(|collected| &collected.module)
    }

    fn all(&self) -> Vec<&Module> {
        self.0.values().map(|collected| &collected.module).collect()
    }
}
