//! The label graph inside one module.
//!
//! Every `jump` and `call` is an edge that has to land somewhere. Recording the edges
//! while collecting means resolution and, later, reachability analysis both read the same
//! structure instead of walking the syntax tree twice.

use vela_span::Span;

/// How control reaches another label.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transfer {
    /// `jump` — control leaves and does not come back.
    Jump,
    /// `call` — control returns when the target does.
    Call,
}

/// A reference to a label, which must resolve.
#[derive(Debug)]
pub struct LabelRef {
    /// The dotted path as written.
    pub path: Vec<String>,
    /// Where it was written.
    pub span: Span,
    /// How control gets there.
    pub transfer: Transfer,
}

impl LabelRef {
    /// The path as written, for a diagnostic message.
    #[must_use]
    pub fn dotted(&self) -> String {
        self.path.join(".")
    }

    /// The module qualifier, if the path has one.
    #[must_use]
    pub fn qualifier(&self) -> Option<String> {
        let (_, qualifier) = self.path.split_last()?;
        (!qualifier.is_empty()).then(|| qualifier.join("."))
    }

    /// The label's own name, the last segment.
    #[must_use]
    pub fn label(&self) -> &str {
        self.path.last().map_or("", String::as_str)
    }
}

/// A label, and everything it can transfer to.
#[derive(Debug)]
pub struct LabelNode {
    /// The label's own name, unqualified.
    pub name: String,
    /// Where it was written.
    pub span: Span,
    /// Where it can go.
    pub targets: Vec<LabelRef>,
}

/// The labels in one module.
#[derive(Debug, Default)]
pub struct StoryGraph {
    /// Every label, in source order.
    pub nodes: Vec<LabelNode>,
}

impl StoryGraph {
    /// Finds a label by its own name.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&LabelNode> {
        self.nodes.iter().find(|node| node.name == name)
    }

    /// Every label this one can reach directly.
    #[must_use]
    pub fn targets_of(&self, name: &str) -> &[LabelRef] {
        self.find(name).map_or(&[], |node| node.targets.as_slice())
    }
}
