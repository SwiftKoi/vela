//! What a screen's layout depends on, computed once at screen-compile time.
//!
//! `SCREENS.md §8.2`: *"dependency sets are computed statically at screen-compile time, so
//! there is no runtime dependency-tracking machinery and no way for a binding to silently
//! miss an update."*
//!
//! Both halves of that matter, and the second is the reason it is static. Runtime tracking is
//! machinery that can be *wrong* — a path someone forgot to instrument means a widget that
//! shows stale text, and nothing reports it. Walking the tree once cannot forget: every `Name`
//! in the body is collected, whether or not anyone remembered to instrument it.
//!
//! The set is ordered, not hashed. It is small, it is compared, and a project that forbids
//! hash iteration should not make an exception for the thing that decides whether to repaint.

use std::collections::BTreeSet;

use vela_syntax::{Expr, ScreenArg, ScreenDecl, ScreenLine, StrPart};

use crate::compose;

/// The names a screen body reads.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct DepSet {
    names: BTreeSet<String>,
}

impl DepSet {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a name.
    pub fn insert(&mut self, name: impl Into<String>) {
        self.names.insert(name.into());
    }

    /// A name read by this screen.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    /// How many names are read.
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether nothing is read — which is what makes a screen *static*.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The names, in order.
    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.names.iter()
    }

    /// Whether any of `changed` is read by this screen.
    ///
    /// The whole question a frame asks. A static screen reads nothing, so this is `false` for
    /// every change — which is why it never relays out, rather than because a flag happens to
    /// be clear.
    #[must_use]
    pub fn is_hit_by(&self, changed: &[String]) -> bool {
        changed.iter().any(|name| self.names.contains(name))
    }
}

/// Collects the names a screen body reads, including what the screens it uses read.
///
/// Following `use` is not optional. A screen that draws `use navigation` has drawn whatever
/// `navigation` reads, so a change to a field only the used screen mentions still makes this screen's
/// frame stale — and a dependency set whose whole job is that a binding never silently misses an
/// update would have a hole exactly the shape of composition.
#[must_use]
pub fn deps_of(screens: &[&ScreenDecl], lines: &[ScreenLine]) -> DepSet {
    let mut deps = DepSet::new();
    let mut path = Vec::new();
    collect_lines(screens, lines, &mut deps, &mut path);
    deps
}

/// Walks the body.
///
/// `path` is the `use` chain being walked. A cycle is refused by the checker (`E5011`), so this is a
/// bound on a crafted pack rather than on a file — the same backstop the accessibility walk keeps.
fn collect_lines(
    screens: &[&ScreenDecl],
    lines: &[ScreenLine],
    out: &mut DepSet,
    path: &mut Vec<String>,
) {
    for line in lines {
        match line {
            ScreenLine::Layer { .. } | ScreenLine::Transclude { .. } => {}
            ScreenLine::If {
                condition, body, ..
            } => {
                collect_expr(condition, out);
                collect_lines(screens, body, out, path);
            }
            ScreenLine::Use {
                name, args, body, ..
            } => {
                // Only the argument *values* are reads here. A named argument's name is a parameter
                // of the used screen, not a field this screen mentions — the opposite of a widget's
                // arg, where a bare name is the content (`text line`).
                for arg in args {
                    match arg {
                        ScreenArg::Value(value) => collect_expr(value, out),
                        ScreenArg::Named {
                            value: Some(value), ..
                        } => collect_expr(value, out),
                        ScreenArg::Named { value: None, .. } => {}
                    }
                }
                // The block is this screen's own code, wherever the used screen places it.
                collect_lines(screens, body, out, path);

                if !path.iter().any(|seen| seen == name)
                    && let Some(callee) = compose::find(screens, name)
                {
                    path.push(name.clone());
                    collect_lines(screens, &callee.body, out, path);
                    path.pop();
                }
            }
            ScreenLine::Node(node) => {
                for arg in &node.args {
                    match arg {
                        ScreenArg::Value(value) => collect_expr(value, out),
                        // A bare name is collected too, even though most of them are prop
                        // names like `stretch_x` that read nothing. `text line` is the same
                        // shape and *is* a read, and the two cannot be told apart here — so
                        // this over-approximates, which is the safe direction: an extra name
                        // costs a relayout, a missing one shows stale text forever.
                        ScreenArg::Named { name, value, .. } => {
                            out.insert(name.clone());
                            if let Some(value) = value {
                                collect_expr(value, out);
                            }
                        }
                    }
                }
                collect_lines(screens, &node.children, out, path);
            }
        }
    }
}

/// Walks an expression.
///
/// Exhaustive on purpose: a new `Expr` variant should be a compile error here rather than a
/// binding that silently stops being tracked, which is the failure this whole module exists to
/// make impossible.
fn collect_expr(expr: &Expr, out: &mut DepSet) {
    match expr {
        Expr::Name { name, .. } => out.insert(name.clone()),
        // A path is a qualified name: `forest.confession` is one thing, and recording it whole
        // is what lets a change to `forest` be told from a change to `forest.confession`.
        Expr::Path { value, .. } => out.insert(value.clone()),
        Expr::Field { base, name, .. } => {
            collect_expr(base, out);
            out.insert(name.clone());
        }
        Expr::Call { callee, args, .. } => {
            collect_expr(callee, out);
            for arg in args {
                collect_expr(arg, out);
            }
        }
        Expr::Index { base, index, .. } => {
            collect_expr(base, out);
            collect_expr(index, out);
        }
        Expr::Unary { operand, .. } => collect_expr(operand, out),
        Expr::Binary { lhs, rhs, .. } => {
            collect_expr(lhs, out);
            collect_expr(rhs, out);
        }
        Expr::Paren { inner, .. } => collect_expr(inner, out),
        Expr::If {
            cond, then_, else_, ..
        } => {
            collect_expr(cond, out);
            collect_expr(then_, out);
            collect_expr(else_, out);
        }
        // An interpolation is a read like any other: `"Trust: {bind trust}"` depends on
        // `trust`, and a walker that only looked at statements would miss it.
        Expr::Str { parts, .. } => {
            for part in parts {
                if let StrPart::Interpolation { expr: inner, .. } = part {
                    collect_expr(inner, out);
                }
            }
        }
        Expr::List { items, .. } => {
            for item in items {
                collect_expr(item, out);
            }
        }
        Expr::Map { entries, .. } => {
            for (key, value) in entries {
                collect_expr(key, out);
                collect_expr(value, out);
            }
        }
        Expr::Lambda { body, params, .. } => {
            // A lambda's parameters shadow, so they are removed after the walk rather than
            // skipped during it — a name read inside is still a read if it is not a parameter.
            let mut inner = DepSet::new();
            collect_expr(body, &mut inner);
            for param in params {
                inner.names.remove(&param.name);
            }
            for name in inner.names {
                out.insert(name);
            }
        }
        Expr::Int { .. }
        | Expr::Float { .. }
        | Expr::Bool { .. }
        | Expr::None { .. }
        | Expr::Error { .. } => {}
    }
}
