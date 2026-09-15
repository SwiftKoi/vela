//! Collecting a module's definitions and label graph.
//!
//! This is phase one of resolution, and it deliberately reads *only* the module's own
//! file. That is what makes per-module incrementality possible: collecting symbols for one
//! module cannot be disturbed by an edit to another, so the query is invalidated by
//! exactly one file.

use std::collections::BTreeMap;

use vela_diag::Diagnostic;
use vela_span::{FileId, Span};
use vela_syntax::{Item, LabelDecl, Program, Stmt, UseDecl};

use crate::def::{DefKind, Definition};
use crate::error;
use crate::module::ModuleName;
use crate::story::{LabelNode, LabelRef, StoryGraph, Transfer};

/// A module: what it defines, and how its labels connect.
#[derive(Debug)]
pub struct Module {
    /// The module's dotted name.
    pub name: ModuleName,
    /// The file it came from.
    pub file: FileId,
    /// Everything it defines, by name. On a duplicate, the first definition wins.
    pub definitions: BTreeMap<String, Definition>,
    /// Names usable for other modules, from `use` — the alias, or the full path.
    pub aliases: BTreeMap<String, Import>,
    /// The labels and the transfers between them.
    pub story: StoryGraph,
}

impl Module {
    /// Whether this module defines a label with that name.
    #[must_use]
    pub fn has_label(&self, name: &str) -> bool {
        self.story.find(name).is_some()
    }
}

/// A module this one imports.
#[derive(Debug)]
pub struct Import {
    /// The module the import names.
    pub module: ModuleName,
    /// Where the `use` wrote it.
    pub span: Span,
}

/// What collecting a module produced.
#[derive(Debug)]
pub struct Collected {
    /// The module.
    pub module: Module,
    /// Problems found while collecting it.
    pub diagnostics: Vec<Diagnostic>,
}

/// Collects a module's definitions and label graph from its syntax tree.
#[must_use]
pub fn collect(name: ModuleName, file: FileId, program: &Program) -> Collected {
    let mut collector = Collector {
        module: Module {
            name,
            file,
            definitions: BTreeMap::new(),
            aliases: BTreeMap::new(),
            story: StoryGraph::default(),
        },
        diagnostics: Vec::new(),
    };

    for item in &program.items {
        collector.item(item);
    }

    Collected {
        module: collector.module,
        diagnostics: collector.diagnostics,
    }
}

/// Walks a syntax tree, filling in one module.
struct Collector {
    module: Module,
    diagnostics: Vec<Diagnostic>,
}

impl Collector {
    /// Records one top-level item.
    fn item(&mut self, item: &Item) {
        match item {
            Item::Use(decl) => self.import(decl),
            // Recorded under its *dotted* name, because that is how a call names it and two
            // capabilities can share a leaf: `audio.position` and `input.position` would
            // collide as bare names.
            Item::Effect(decl) => {
                self.define(&decl.dotted(), DefKind::Effect, decl.span);
            }
            Item::Const(decl) => {
                self.define(&decl.name, DefKind::Constant, decl.span);
            }
            Item::Default(decl) => {
                self.define(&decl.name, DefKind::Default, decl.span);
            }
            Item::Struct(decl) => {
                self.define(&decl.name, DefKind::Struct, decl.span);
            }
            Item::Enum(decl) => {
                self.define(&decl.name, DefKind::Enum, decl.span);
            }
            Item::Character(decl) => {
                self.define(&decl.name, DefKind::Character, decl.span);
            }
            Item::Transform(decl) => {
                self.define(&decl.name, DefKind::Transform, decl.span);
            }
            Item::Screen(decl) => {
                self.define(&decl.name, DefKind::Screen, decl.span);
            }
            Item::Style(decl) => {
                self.define(&decl.name, DefKind::Style, decl.span);
            }
            Item::Theme(decl) => {
                self.define(&decl.name, DefKind::Theme, decl.span);
            }
            Item::Function(decl) => {
                self.define(&decl.name, DefKind::Function, decl.span);
            }
            // An image's name is a dotted path: `bg.forest` defines `bg.forest`.
            Item::Image(decl) => {
                self.define(&decl.name.join("."), DefKind::Image, decl.span);
            }
            Item::Label(decl) => self.label(decl),
            // A syntax error already reported itself; there is no name to record.
            Item::Error { .. } => {}
        }
    }

    /// Records a definition, reporting a name defined twice.
    ///
    /// Returns whether it was accepted, so callers can skip work that only makes sense
    /// for a definition that survived.
    fn define(&mut self, name: &str, kind: DefKind, span: Span) -> bool {
        let definition = Definition {
            name: name.to_string(),
            kind,
            span,
        };

        if let Some(previous) = self.module.definitions.get(name) {
            let diagnostic =
                error::duplicate_definition(&self.module.name.clone(), &definition, previous);
            self.diagnostics.push(diagnostic);
            return false;
        }

        self.module.definitions.insert(name.to_string(), definition);
        true
    }

    /// Records a `use`.
    fn import(&mut self, decl: &UseDecl) {
        let path = ModuleName::new(decl.path.join("."));
        // Without `as`, the module is named by its full dotted path; with it, by the
        // alias. Both are what a qualified reference writes before the dot.
        let name = decl
            .alias
            .clone()
            .unwrap_or_else(|| path.as_str().to_string());
        self.module.aliases.insert(
            name,
            Import {
                module: path,
                span: decl.span,
            },
        );
    }

    /// Records a label and the transfers out of it.
    fn label(&mut self, decl: &LabelDecl) {
        if !self.define(&decl.name, DefKind::Label, decl.span) {
            // The duplicate is already reported; a second node would make `find` return
            // whichever came first for no benefit.
            return;
        }

        let mut targets = Vec::new();
        transfers(&decl.body, &mut targets);
        self.module.story.nodes.push(LabelNode {
            name: decl.name.clone(),
            span: decl.span,
            targets,
        });

        if !terminates(&decl.body) {
            // Point at the last statement, not the `label` line: the reader needs to see
            // where control runs out, which is the end of the body.
            let end = decl.body.last().map_or(decl.span, Stmt::span);
            self.diagnostics
                .push(error::label_falls_through(&decl.name, end));
        }
    }
}

/// Whether every path through these statements ends in a transfer or a return.
///
/// Structural rather than a data-flow analysis: it answers "can control reach the end of
/// this block?", which is exactly what the warning is about. A label that can is usually
/// a mistake — the story simply stops — but it is a warning rather than an error, because
/// ending the story is occasionally what was meant.
fn terminates(statements: &[Stmt]) -> bool {
    let Some(last) = statements.last() else {
        // Nothing at all: control falls straight off the end.
        return false;
    };

    match last {
        Stmt::Jump(_) | Stmt::Call(_) | Stmt::Return(_) => true,
        // A branch only terminates if *every* way out of it does, and an `if` with no
        // `else` always has a way out that does not.
        Stmt::If(stmt) => {
            stmt.else_body.as_ref().is_some_and(|body| terminates(body))
                && stmt.elifs.iter().all(|clause| terminates(&clause.body))
                && terminates(&stmt.then_body)
        }
        Stmt::Menu(menu) => {
            !menu.choices.is_empty() && menu.choices.iter().all(|choice| terminates(&choice.body))
        }
        Stmt::Match(stmt) => {
            stmt.arms.iter().any(|arm| arm.pattern.is_none())
                && stmt.arms.iter().all(|arm| terminates(&arm.body))
        }
        _ => false,
    }
}

/// Collects the label transfers inside a statement list, descending into nested blocks.
///
/// A menu choice, an `if` branch, or a loop body can all contain a `jump`, and a graph
/// that missed them would report reachable labels as dead.
fn transfers(statements: &[Stmt], out: &mut Vec<LabelRef>) {
    for statement in statements {
        match statement {
            Stmt::Jump(jump) => out.push(LabelRef {
                path: jump.target.clone(),
                span: jump.span,
                transfer: Transfer::Jump,
            }),
            Stmt::Call(call) => out.push(LabelRef {
                path: call.target.clone(),
                span: call.span,
                transfer: Transfer::Call,
            }),
            Stmt::Menu(menu) => {
                for choice in &menu.choices {
                    transfers(&choice.body, out);
                }
            }
            Stmt::If(stmt) => {
                transfers(&stmt.then_body, out);
                for clause in &stmt.elifs {
                    transfers(&clause.body, out);
                }
                if let Some(body) = &stmt.else_body {
                    transfers(body, out);
                }
            }
            Stmt::While(stmt) => transfers(&stmt.body, out),
            Stmt::For(stmt) => transfers(&stmt.body, out),
            Stmt::Match(stmt) => {
                for arm in &stmt.arms {
                    transfers(&arm.body, out);
                }
            }
            _ => {}
        }
    }
}
