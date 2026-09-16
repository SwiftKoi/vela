//! What an offset names, and where else that is named.
//!
//! Four capabilities are one question asked differently. Goto-definition wants the declaration;
//! references and rename want every place it is named; hover wants a sentence about it. So they share
//! an answer — "this offset names *that*" — and the answer is assembled from what the compiler already
//! knows rather than from a second analysis: `vela-hir` records every definition with its span and
//! every `jump`/`call` with its own, and resolution goes through `target_of`, the one function that
//! decides what a reference points at. A language server with its own path resolution would be a second
//! answer to that question, and the two would disagree the first time one of them was fixed.
//!
//! # What this covers, and what it does not
//!
//! Module-level names: labels (including across a module boundary), and the `use` that names a module.
//! Locals, types, and expression positions are not here yet, because answering for them needs the
//! scope and the type at an offset, which is a question `vela-types` has to answer rather than one
//! this crate can infer. Saying so is better than a capability that answers half the time.

use std::collections::BTreeMap;
use std::rc::Rc;

use vela_compile::Session;
use vela_hir::{Collected, Module, ModuleName, Modules, target_of};
use vela_span::{FileId, Span};

/// What an offset names.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Named {
    /// A module-level definition, keyed by the module that declares it and its own name.
    ///
    /// The key rather than a pointer, because the answer has to survive the query that produced it:
    /// a definition's span belongs to the module it came from, and the caller is going to ask about
    /// another file next.
    Definition {
        /// The module that declares it.
        module: ModuleName,
        /// Its name within that module.
        name: String,
    },
    /// A `use`, which names a module rather than a definition in one.
    Import {
        /// The module it names.
        module: ModuleName,
    },
}

/// A name found at an offset, and where that name was written.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Found {
    /// What it names.
    pub named: Named,
    /// The span of the *name* as written — as closely as the tree records it.
    ///
    /// A `jump` keeps its whole statement rather than its target's own span, which is precise enough to
    /// jump to and not yet precise enough to rewrite: rename needs the target's span, which is a syntax
    /// tree change rather than a language server one.
    pub span: Span,
}

/// What an offset in a file names, if it names anything.
///
/// # Panics
///
/// Panics if `file` did not come from `session`; see `Session::sources`.
#[must_use]
pub fn at(session: &mut Session, file: FileId, offset: u32) -> Option<Found> {
    let collected = session.symbols(file);
    let module = &collected.module;

    // A transfer first, because it is the most specific thing a body can hold and a definition's span
    // covers its whole body: asking a definition first would answer "a label" for every offset in one.
    if let Some(found) = transfer(session, &collected, offset) {
        return Some(found);
    }

    // Then the definition whose *name* the offset is on. A definition's span is its declaration, so an
    // offset anywhere inside it — including the body — would match; the name is at the start, on the
    // first line, which is as close as the tree lets this get without recording name spans.
    for definition in module.definitions.values() {
        let name_ends = first_line_end(session, definition.span);
        if definition.span.start() <= offset && offset <= name_ends {
            return Some(Found {
                named: Named::Definition {
                    module: module.name.clone(),
                    name: definition.name.clone(),
                },
                span: definition.span,
            });
        }
    }

    // And the `use` that names a module, which is a name for something outside this file.
    for import in module.aliases.values() {
        if contains(import.span, offset) {
            return Some(Found {
                named: Named::Import {
                    module: import.module.clone(),
                },
                span: import.span,
            });
        }
    }
    None
}

/// The label a transfer at `offset` names, resolved the way the compiler resolves it.
fn transfer(session: &mut Session, collected: &Rc<Collected>, offset: u32) -> Option<Found> {
    let modules = Lookup::of(session);

    for node in &collected.module.story.nodes {
        for target in &node.targets {
            if !contains(target.span, offset) {
                continue;
            }
            let (module, label) = target_of(&collected.module, &modules, target)?;
            return Some(Found {
                named: Named::Definition {
                    module: module.name.clone(),
                    name: label.to_string(),
                },
                span: target.span,
            });
        }
    }
    None
}

/// Where a name is declared.
#[must_use]
pub fn declaration(session: &mut Session, named: &Named) -> Option<(FileId, Span)> {
    match named {
        Named::Definition { module, name } => {
            let file = file_of(session, module)?;
            let collected = session.symbols(file);
            collected
                .module
                .definitions
                .get(name)
                .map(|definition| (file, definition.span))
        }
        // A `use` names a module: what it points at is that module's file, if the program has it.
        Named::Import { module } => {
            file_of(session, module).map(|file| (file, Span::new(file, 0, 0)))
        }
    }
}

/// Every place a name is written: its declaration, and every reference that resolves to it.
///
/// Ordered, so two runs and two callers agree: by module name, then by position — which is also the
/// order a person reads them in.
#[must_use]
pub fn occurrences(session: &mut Session, named: &Named) -> Vec<(FileId, Span)> {
    let Named::Definition { module, name } = named else {
        return declaration(session, named).into_iter().collect();
    };

    let modules = Lookup::of(session);
    let mut found = Vec::new();

    for collected in modules.0.values() {
        if &collected.module.name == module
            && let Some(definition) = collected.module.definitions.get(name)
        {
            found.push((collected.module.file, definition.span));
        }

        for node in &collected.module.story.nodes {
            for target in &node.targets {
                // The same resolution rule as the compiler's, so a `jump` this says reaches the label
                // is a `jump` the checker says reaches it.
                let reaches = target_of(&collected.module, &modules, target).is_some_and(
                    |(target_module, label)| &target_module.name == module && label == name,
                );
                if reaches {
                    found.push((collected.module.file, target.span));
                }
            }
        }
    }

    found.sort_by_key(|(file, span)| (file.as_raw(), span.start()));
    found
}

/// The file a module's declarations came from.
fn file_of(session: &mut Session, module: &ModuleName) -> Option<FileId> {
    session
        .file_ids()
        .into_iter()
        .find(|file| session.module_of(*file) == Some(module))
}

/// The end of the first line of a span, which is as close as this can get to the name inside it.
fn first_line_end(session: &Session, span: Span) -> u32 {
    let source = session.sources().file(span.file());
    let text = source.text();

    text.get(span.start() as usize..span.end() as usize)
        .and_then(|declaration| declaration.find('\n'))
        .map_or(span.end(), |offset| span.start() + offset as u32)
}

/// Whether a span covers an offset.
fn contains(span: Span, offset: u32) -> bool {
    span.start() <= offset && offset < span.end()
}

/// Every module in a session, for the resolver.
///
/// A `Modules` implementation of this crate's own: the trait exists so that resolution is a pure
/// function of a module and whatever it names, and the session is what decides which modules those are.
struct Lookup(BTreeMap<ModuleName, Rc<Collected>>);

impl Lookup {
    /// Every module the session holds.
    fn of(session: &mut Session) -> Self {
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

impl Modules for Lookup {
    fn get(&self, name: &ModuleName) -> Option<&Module> {
        self.0.get(name).map(|collected| &collected.module)
    }

    fn all(&self) -> Vec<&Module> {
        self.0.values().map(|collected| &collected.module).collect()
    }
}
