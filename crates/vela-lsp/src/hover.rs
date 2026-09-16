//! What to say about a position.
//!
//! Two sources, asked in the order a reader would: the symbol index (what kind of thing is this name,
//! and where is it declared) and then the checker (what type does this have). The first answers for the
//! names a module declares — a label, a `default`, a screen — and the second for everything inside a
//! body, including the local whose type nobody wrote down.
//!
//! Markdown rather than plain text, because that is what an editor renders: names go in code spans and
//! the kind in bold, so `trust: int` and a label's qualified path read the way a reader writes them.

use vela_compile::Session;
use vela_hir::ModuleName;
use vela_span::{FileId, Span};

use crate::symbols::{self, Named};

/// What an editor should show, and what it should underline.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Hover {
    /// The text, as markdown.
    pub markdown: String,
    /// The span the answer is about, when it is in the document that was asked about.
    pub range: Option<Span>,
}

/// What is at an offset, or `None` when nothing is.
pub fn at(session: &mut Session, file: FileId, offset: u32) -> Option<Hover> {
    if let Some(found) = symbols::at(session, file, offset) {
        let markdown = named(session, &found.named);
        let range = (found.span.file() == file).then_some(found.span);
        return Some(Hover { markdown, range });
    }

    // The checker's answer, for everything the index does not know: locals, parameters, and
    // expressions. Both queries are memoized, so asking for the tree and the environment is what the
    // last edit already paid for.
    let parsed = session.parse(file);
    let compiled = session.mir(file);
    let found = vela_types::at(&parsed.program, &compiled.env, offset)?;

    Some(Hover {
        markdown: match found {
            vela_types::Found::Name { name, ty } => format!("**local** `{name}: {ty}`"),
            vela_types::Found::Expression { ty } => format!("`{ty}`"),
        },
        range: None,
    })
}

/// Markdown describing a name the index knows.
fn named(session: &mut Session, named: &Named) -> String {
    match named {
        // A `use` names a module, so there is no definition to describe — just where the module is.
        Named::Import { module } => match files_of(session, module).first().copied() {
            Some(file) => format!(
                "**module** `{module}` — in `{}`",
                session.sources().file(file).name()
            ),
            None => format!("**module** `{module}`"),
        },

        Named::Definition { module, name } => {
            let declared = symbols::declaration(session, named);
            let kind = declared.and_then(|(file, _)| kind_of(session, file, name));
            let annotation = declared.and_then(|(file, _)| value_type(session, file, name));

            let mut text = format!(
                "**{}** `{module}.{name}`",
                // The keyword rather than `describe`: that method answers "defined more than once as *a
                // label*", and the article belongs in a sentence rather than in bold at the top of a
                // hover box. A reference this program cannot resolve — a `jump` into a module that is
                // not here — is still a label to the reader, and the checker has said the rest.
                kind.map_or("label", vela_hir::DefKind::keyword)
            );
            if let Some(ty) = annotation {
                text.push_str(&format!(": `{ty}`"));
            }
            if let Some((file, _)) = declared {
                text.push_str(&format!(
                    " — declared in `{}`",
                    session.sources().file(file).name()
                ));
            }
            text
        }
    }
}

/// The kind of a module-level definition.
fn kind_of(session: &mut Session, file: FileId, name: &str) -> Option<vela_hir::DefKind> {
    let collected = session.symbols(file);
    Some(collected.module.definitions.get(name)?.kind)
}

/// The type a module-level name holds, for the kinds that hold one.
///
/// `value` before `declared`: a `default` holds a value and a `struct` *is* a type, and the environment
/// answers for both. A name that is neither — a label, a screen — has nothing to say about types, and
/// saying nothing is the answer.
fn value_type(session: &mut Session, file: FileId, name: &str) -> Option<String> {
    let compiled = session.mir(file);
    if let Some(ty) = compiled.env.value(name) {
        return Some(ty.to_string());
    }
    compiled.env.declared(name).map(|ty| ty.to_string())
}

/// The files a module's declarations came from.
fn files_of(session: &mut Session, module: &ModuleName) -> Vec<FileId> {
    session
        .file_ids()
        .into_iter()
        .filter(|file| session.module_of(*file) == Some(module))
        .collect()
}
