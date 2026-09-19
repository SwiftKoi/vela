//! What a project stages, and what it declares (`LANGUAGE.md §6.1`, `§7.5`).
//!
//! An image name is a **build** fact rather than a module's: the picture table is one table for a
//! project — `vela build` bakes a name into a texture and a bundle ships no modules at all — so
//! `scene bg.room` may name an image another file declares, and the reference has to be resolved
//! against every module rather than against one.
//!
//! That is why this lives here and is *asked* from `vela-compile`'s whole-program analysis: this
//! crate owns the statement walk (the same nesting rules `names` uses), and the driver is the layer
//! that can see every module at once. Neither alone can answer the question, which is why the two
//! halves are separated this way rather than by putting the check in the per-module resolver — where
//! it was tried, and where it reported `vela migrate`'s own output.

use std::collections::BTreeSet;

use vela_span::Span;
use vela_syntax::{Item, Program, Stmt};

/// One `scene`/`show`/`hide`, and the name it stages.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Staged {
    /// Where it is written, which carries the file.
    pub span: Span,
    /// The image's dotted name, as written.
    pub name: String,
}

/// Every image a program declares, by dotted name.
#[must_use]
pub fn declared(tree: &Program) -> BTreeSet<String> {
    tree.items
        .iter()
        .filter_map(|item| match item {
            Item::Image(decl) => Some(decl.name.join(".")),
            _ => None,
        })
        .collect()
}

/// Every image a program stages, in source order.
#[must_use]
pub fn staged(tree: &Program) -> Vec<Staged> {
    let mut found = Vec::new();
    for item in &tree.items {
        match item {
            Item::Label(decl) => walk(&decl.body, &mut found),
            Item::Function(decl) => walk(&decl.body, &mut found),
            _ => {}
        }
    }
    found.sort_by_key(|staged| (staged.span.file().as_raw(), staged.span.start()));
    found
}

/// Walks a statement list, descending into every body it opens.
fn walk(body: &[Stmt], found: &mut Vec<Staged>) {
    for statement in body {
        match statement {
            Stmt::Stage(stage) => found.push(Staged {
                span: stage.span,
                name: stage.image.join("."),
            }),
            Stmt::If(if_) => {
                walk(&if_.then_body, found);
                for clause in &if_.elifs {
                    walk(&clause.body, found);
                }
                if let Some(else_body) = &if_.else_body {
                    walk(else_body, found);
                }
            }
            Stmt::While(while_) => walk(&while_.body, found),
            Stmt::For(for_) => walk(&for_.body, found),
            Stmt::Match(match_) => {
                for arm in &match_.arms {
                    walk(&arm.body, found);
                }
            }
            Stmt::Menu(menu) => {
                for choice in &menu.choices {
                    walk(&choice.body, found);
                }
            }
            _ => {}
        }
    }
}
