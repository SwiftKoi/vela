//! The compat report: everything the migration refused to guess at.
//!
//! `TOOLING.md §8`: *"anything outside that set produces a report entry with `file:line`, the
//! original text, and the reason"*, and the rule behind it is that a wrong automatic translation
//! is worse than an explicit "port this by hand" — it fails later, and it fails inside someone's
//! save file.
//!
//! The report is deterministic by construction: entries are collected while files are walked in
//! a sorted order, so two runs over one project produce the same text, byte for byte. That is
//! what makes it a *work item list* a team can diff and watch go to zero.

/// One thing the migration did not translate.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    /// The file, as a path relative to the project's `game/` — the name a person would look for.
    pub file: String,
    /// The 1-based line it is on.
    pub line: u32,
    /// The source text, trimmed, so the reader can find it without opening the file.
    pub original: String,
    /// What is wrong and what to do about it.
    pub reason: String,
}

/// Every entry, in the order they were found.
#[derive(Clone, Default, Debug)]
pub struct Report {
    entries: Vec<Entry>,
}

impl Report {
    /// An empty report.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Notes something that was not translated.
    pub fn push(&mut self, file: &str, line: u32, original: &str, reason: impl Into<String>) {
        self.entries.push(Entry {
            file: file.to_string(),
            line,
            original: original.trim().to_string(),
            reason: reason.into(),
        });
    }

    /// Every entry.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// How many entries there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing was reported — which is a claim, not a silence.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Folds another report's entries in, preserving order.
    pub fn absorb(&mut self, other: Self) {
        self.entries.extend(other.entries);
    }

    /// The report as text, one entry per line.
    ///
    /// `file:line` first so the whole thing is greppable and so an editor can jump from it,
    /// which is the form the spec asks for.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            out.push_str(&format!(
                "{}:{}: {}\n    {}\n",
                entry.file, entry.line, entry.reason, entry.original
            ));
        }
        out
    }
}
