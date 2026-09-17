//! Hot reload: matching a screen's new tree against its old one.
//!
//! `SCREENS.md §12`: *"editing a screen inside `vela run` recompiles it and diffs the widget
//! tree — nodes with stable ids keep their state (scroll position, input buffer, animation
//! phase), nodes without ids are matched positionally, and a structural change that cannot be
//! reconciled falls back to a full screen rebuild, with a console note saying so."*
//!
//! Rebuild is safe **by construction** because of §2's purity rule: a screen is a function of
//! its arguments and bound state, and there is no hidden mutation to lose. So the interesting
//! part is not "can we rebuild" — we always can — but *what is worth carrying across*, and
//! that is a question about identity.
//!
//! This is the pure half: two trees in, a verdict out. The file watching and the recompiling
//! are plumbing around it, and keeping them out means the interesting rules are testable with
//! no running story at all.

use std::collections::BTreeMap;

use vela_syntax::{ScreenArg, ScreenLine, ScreenNode};

/// How a node is identified across an edit.
///
/// An explicit `id` is what an author uses when they care; a path is what they get when they
/// do not, and it is why reordering siblings loses state while editing a sibling does not.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Key {
    /// An author wrote `id`.
    Id(String),
    /// A position in the tree, from the root.
    Path(Vec<usize>),
}

/// What an edit did to the state that can be carried across.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Diff {
    /// The nodes whose state survives, because they can be matched to an old node.
    pub kept: Vec<Key>,
    /// Whether the screen has to be rebuilt from scratch.
    pub rebuild: bool,
    /// Why, when it does. For the console, per §12.
    pub note: Option<String>,
}

impl Diff {
    /// Whether state under `key` survives this edit.
    #[must_use]
    pub fn keeps(&self, key: &Key) -> bool {
        self.kept.contains(key)
    }

    /// How many nodes keep their state.
    #[must_use]
    pub fn kept_count(&self) -> usize {
        self.kept.len()
    }
}

/// Compares a screen's old body with its new one.
#[must_use]
pub fn diff(before: &[ScreenLine], after: &[ScreenLine]) -> Diff {
    let mut old = Tree::default();
    collect(before, &mut Vec::new(), &mut old);
    let mut new = Tree::default();
    collect(after, &mut Vec::new(), &mut new);

    // The one thing that cannot be reconciled. Two nodes claiming one id means a match would
    // be a guess, and guessing wrong moves one widget's scroll position to another — which is
    // worse than losing both, because it looks like it worked.
    for (id, paths) in &new.by_id {
        if paths.len() > 1 {
            return Diff {
                kept: Vec::new(),
                rebuild: true,
                note: Some(format!(
                    "`{id}` is used by {} nodes; rebuilding",
                    paths.len()
                )),
            };
        }
    }

    let mut kept = Vec::new();
    for node in &new.nodes {
        if let Some(id) = &node.id {
            // By id: survives a move, a rename of a sibling, and an insertion before it.
            if old.by_id.contains_key(id) {
                kept.push(Key::Id(id.clone()));
            }
            continue;
        }
        // No id: matched positionally, and only if the same widget is still there. A path
        // that now holds a different widget is a different node, and its state belongs to
        // the one that is gone.
        // An old node that had an `id` is not reusable positionally: it is matched by its id,
        // and letting a *different* node claim its old position would hand one widget's state
        // to another. That is the whole reason `id` exists, and getting it wrong looks like it
        // worked — an inserted row quietly inherits the scroll position of the row it pushed
        // down.
        let reusable = old.nodes.iter().any(|previous| {
            previous.id.is_none() && previous.path == node.path && previous.name == node.name
        });
        if reusable {
            kept.push(Key::Path(node.path.clone()));
        }
    }

    // A screen whose root changed is a different screen, whatever else matches.
    let root_changed = match (new.nodes.first(), old.nodes.first()) {
        (Some(new), Some(old)) => new.name != old.name,
        (None, None) => false,
        _ => true,
    };
    if root_changed {
        return Diff {
            kept: Vec::new(),
            rebuild: true,
            note: Some("the screen's root widget changed; rebuilding".to_string()),
        };
    }

    Diff {
        kept,
        rebuild: false,
        note: None,
    }
}

/// One node, flattened.
struct Flat {
    path: Vec<usize>,
    name: String,
    id: Option<String>,
}

/// Every node in a body, flat, with its path.
#[derive(Default)]
struct Tree {
    nodes: Vec<Flat>,
    by_id: BTreeMap<String, Vec<Vec<usize>>>,
}

/// Walks a body, collecting nodes.
fn collect(lines: &[ScreenLine], prefix: &mut Vec<usize>, out: &mut Tree) {
    for (index, line) in lines.iter().enumerate() {
        let ScreenLine::Node(node) = line else {
            // A conditional is not a node: it is a shape the tree takes at run time, so its
            // children are collected where they are rather than under a path nobody sees — every
            // arm's children, since which arm is drawn is a runtime question.
            for body in line.bodies() {
                collect(body, prefix, out);
            }
            continue;
        };
        let mut path = prefix.clone();
        path.push(index);

        let id = id_of(node);
        if let Some(id) = &id {
            out.by_id.entry(id.clone()).or_default().push(path.clone());
        }
        out.nodes.push(Flat {
            path: path.clone(),
            name: node.name.clone(),
            id,
        });
        collect(&node.children, &mut path, out);
    }
}

/// A node's explicit `id`, if it wrote one.
fn id_of(node: &ScreenNode) -> Option<String> {
    for arg in &node.args {
        if let ScreenArg::Named {
            name,
            value: Some(vela_syntax::Expr::Name { name: value, .. }),
            ..
        } = arg
        {
            if name == "id" {
                return Some(value.clone());
            }
        }
    }
    None
}
