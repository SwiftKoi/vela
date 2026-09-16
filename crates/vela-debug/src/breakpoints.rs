//! Where the debugger stops, and what it stops for.
//!
//! Two kinds of breakpoint, because the language has two natural anchors. A **line** in a file is
//! where an author points; a **label** name is where a reader thinks in terms of the story's own
//! structure. DAP spells the first `setBreakpoints` and the second `setFunctionBreakpoints`, and
//! both are kept here so that one stop decision reads them together rather than each handler
//! deciding whether it is its turn to stop.

use std::collections::{BTreeMap, BTreeSet};

use vela_span::FileId;

/// The breakpoints a client has set.
#[derive(Default, Debug)]
pub struct Breakpoints {
    /// Line breakpoints, by file, as 0-based lines.
    lines: BTreeMap<FileId, BTreeSet<u32>>,
    /// Label breakpoints, by name as the client wrote it.
    labels: BTreeSet<String>,
}

impl Breakpoints {
    /// No breakpoints.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the line breakpoints in a file.
    ///
    /// *Replaces*, not adds: DAP's `setBreakpoints` for a source is the whole set for that source,
    /// and an empty list clears them. Accumulating would make a breakpoint impossible to remove.
    pub fn set_lines(&mut self, file: FileId, lines: impl IntoIterator<Item = u32>) {
        let set: BTreeSet<u32> = lines.into_iter().collect();
        if set.is_empty() {
            self.lines.remove(&file);
        } else {
            self.lines.insert(file, set);
        }
    }

    /// Replaces the label breakpoints.
    pub fn set_labels(&mut self, labels: impl IntoIterator<Item = String>) {
        self.labels = labels.into_iter().collect();
    }

    /// Whether a line in a file is a breakpoint.
    #[must_use]
    pub fn hits_line(&self, file: FileId, line: u32) -> bool {
        self.lines
            .get(&file)
            .is_some_and(|lines| lines.contains(&line))
    }

    /// Whether entering a body is a breakpoint.
    ///
    /// A client names a label the way an author wrote it (`start`), and a linked program qualifies
    /// it (`main.start`), so the last segment matches too. That is the same rule a reference is
    /// written by, and the alternative — demanding the qualified name — would ask a person to know
    /// how modules were linked.
    #[must_use]
    pub fn hits_label(&self, body: &str) -> bool {
        self.labels
            .iter()
            .any(|label| label == body || body.ends_with(&format!(".{label}")))
    }

    /// Whether anything is set at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.labels.is_empty()
    }

    /// How many line breakpoints are set.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.values().map(BTreeSet::len).sum()
    }
}
