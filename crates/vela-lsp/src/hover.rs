//! What to say about a position.
//!
//! Three sources, asked in the order a reader would. The symbol index answers for the names a module
//! declares — a label, a `default`, a screen — and says where each is declared. The checker answers for
//! everything inside a body, including the local whose type nobody wrote down. And the widget and action
//! *vocabulary* answers last, for the words a screen uses that are not declared anywhere: `text` at the
//! start of a line, `close_screen` after `action`. That third one is the generated reference's schema
//! (`TOOLING.md §9`), so a hover and the reference page describe a widget with the same sentence.
//!
//! Markdown rather than plain text, because that is what an editor renders: names go in code spans and
//! the kind in bold, so `trust: int` and a label's qualified path read the way a reader writes them.

use vela_compile::Session;
use vela_hir::ModuleName;
use vela_span::{FileId, Span};
use vela_syntax::Program;

use crate::completion;
use crate::docs::{self, Reference};
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
pub fn at(
    session: &mut Session,
    file: FileId,
    offset: u32,
    reference: &Reference,
) -> Option<Hover> {
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
    if let Some(found) = vela_types::at(&parsed.program, &compiled.env, offset) {
        return Some(Hover {
            markdown: match found {
                vela_types::Found::Name { name, ty } => format!("**local** `{name}: {ty}`"),
                vela_types::Found::Expression { ty } => format!("`{ty}`"),
            },
            range: None,
        });
    }

    // Neither the index nor the checker knows this word, which is what a screen's vocabulary looks
    // like: `text` and `close_screen` are not declared anywhere a name is.
    vocabulary(session, file, offset, &parsed.program, reference)
}

/// A widget or an action, from the schema the reference page is generated from.
fn vocabulary(
    session: &Session,
    file: FileId,
    offset: u32,
    tree: &Program,
    reference: &Reference,
) -> Option<Hover> {
    let (word, start, end) = docs::word_at(session.sources().get(file)?.text(), offset)?;
    let entry = docs::lookup(word)?;

    // Only where the word can mean what it says. A screen's line begins with a widget and nothing else
    // can, and an action follows the word `action` — the same two questions completion asks, so a hover
    // and a completion list cannot disagree about what a position is for. Without this, a variable
    // named `text` would be called a widget.
    //
    // The question is asked of the word's *start*, not of the caret: a hover sits in the middle of a
    // word (`col|umn`), and the completion list it is agreeing with is asked for at the line's start.
    let here = if entry.kind == "widget" {
        completion::widgets_wanted(session, file, tree, start)
    } else {
        action_context(session.sources().get(file)?.text(), start)
    };
    if !here {
        return None;
    }

    let mut markdown = format!(
        "**{}** `{}` — {}",
        entry.kind, entry.signature, entry.summary
    );
    if let Some(link) = reference.link(entry.page, &entry.signature) {
        markdown.push_str(&format!("\n\n[{}]({link})", entry.title));
    }

    Some(Hover {
        markdown,
        range: Some(Span::new(file, start, end)),
    })
}

/// Whether a word follows the keyword that introduces an action.
fn action_context(text: &str, start: u32) -> bool {
    let before = text.get(..start as usize).unwrap_or(text);
    let line = before.rsplit('\n').next().unwrap_or(before).trim_end();
    line == "action" || line.ends_with(" action")
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
