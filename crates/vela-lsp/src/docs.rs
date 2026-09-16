//! The generated reference, and the vocabulary names a hover can point into.
//!
//! `TOOLING.md §9` makes the reference a *generation* of the schemas rather than prose written beside
//! them, so a widget or an action is answered here from the same definition the checker and the
//! completion list read — the docs and the editor are one list read twice. This module is the third
//! reader: it turns a word into the sentence the reference prints, and, when the workspace has
//! generated the pages, into a link to the section it came from.
//!
//! # Why the link is conditional
//!
//! A project has the reference only if it asked for one — `vela doc --out docs/reference`, which
//! `TOOLING.md §9` describes as one line in a build script. A link emitted unconditionally would be a
//! link to a file that is not there, which an editor offers to open and then fails on; so the link
//! appears exactly when the page does, and the sentence is shown either way.

use std::path::Path;

use vela_ui::WidgetRegistry;
use vela_ui::actions::ActionRegistry;

use crate::uri;

/// A vocabulary name the reference documents.
pub struct Entry {
    /// What kind of thing it is: `widget` or `action`.
    pub kind: &'static str,
    /// How a reader writes it: `text`, `close_screen()`.
    pub signature: String,
    /// One sentence describing it, from the schema.
    pub summary: String,
    /// The reference page's file name.
    pub page: &'static str,
    /// That page's title, for the link's text.
    pub title: &'static str,
}

/// The entry for a word, if it names a widget or an action.
///
/// Widgets first: a widget and an action cannot share a name today, and if they ever did, a screen
/// body's line beginning is the more common thing to be looking at.
#[must_use]
pub fn lookup(word: &str) -> Option<Entry> {
    let widgets = WidgetRegistry::builtin();
    if let Some(widget) = widgets.get(word) {
        return Some(Entry {
            kind: "widget",
            signature: widget.name.to_string(),
            summary: widget.summary(),
            page: "widgets.md",
            title: "Widget reference",
        });
    }

    let actions = ActionRegistry::builtin();
    let action = actions.get(word)?;
    Some(Entry {
        kind: "action",
        signature: action.signature(),
        summary: action.summary(),
        page: "actions.md",
        title: "Action reference",
    })
}

/// Where a workspace's generated reference lives.
pub struct Reference {
    /// The workspace root, as the client named it.
    root: String,
}

impl Reference {
    /// The reference for a workspace.
    #[must_use]
    pub fn new(root: impl Into<String>) -> Self {
        Self { root: root.into() }
    }

    /// A link to a page's section, when the workspace has that page.
    ///
    /// The anchor is the heading's slug, computed the way a markdown renderer computes it, and the
    /// heading itself is built from the schema's own signature — so a link and the heading it aims at
    /// are the same spelling of the same name.
    #[must_use]
    pub fn link(&self, page: &str, signature: &str) -> Option<String> {
        let path = Path::new(&self.root).join("docs/reference").join(page);
        if !path.is_file() {
            return None;
        }
        let uri = uri::of(&self.root, &format!("docs/reference/{page}"));
        Some(format!("{uri}#{}", anchor(signature)))
    }
}

/// The word an offset is inside, with the range it covers.
///
/// `None` at a caret that is not on a word — whitespace, punctuation — which is the answer that lets
/// hover say nothing rather than guess.
#[must_use]
pub fn word_at(text: &str, offset: u32) -> Option<(&str, u32, u32)> {
    let bytes = text.as_bytes();
    let at = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(bytes.len());
    let is_word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';

    let mut start = at;
    while start > 0 && is_word(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = at;
    while end < bytes.len() && is_word(bytes[end]) {
        end += 1;
    }

    let word = text.get(start..end).filter(|word| !word.is_empty())?;
    Some((word, start as u32, end as u32))
}

/// A heading's anchor, the way a markdown renderer makes one: lower case, with everything that is not
/// a letter, a digit, an underscore, or a hyphen dropped. `` `close_screen()` `` becomes
/// `close_screen`, which is the heading `vela doc` wrote and the anchor a reader's client computes.
fn anchor(signature: &str) -> String {
    signature
        .chars()
        .filter(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
        .flat_map(char::to_lowercase)
        .collect()
}
