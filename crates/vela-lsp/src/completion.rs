//! What could be typed at a position.
//!
//! Three lists, chosen by where the cursor is rather than by what has been typed so far — an editor
//! filters by prefix itself, and a server that guessed at the prefix would offer different things for
//! the same position depending on how far the author had got.
//!
//! - at the start of a line inside a `screen`: the widgets the registry knows, because that is the only
//!   thing that can begin such a line;
//! - after `jump` or `call`: labels — this module's, and the ones an import makes reachable, qualified
//!   by the name the import gives them. Deliberately not every label in the project: a flat namespace
//!   is what `LANGUAGE.md §6` exists to avoid, and a completion list is where that would hurt most;
//! - anywhere else: the names in scope (locals and parameters, by the checker's own rule) and the names
//!   the module declares.
//!
//! What is *not* here yet, and is in `TOOLING.md §4`: props after a widget name, enum variants after a
//! `.`, and screen actions. Each needs the vocabulary of whatever precedes the cursor, which is a
//! different question from "what is in scope" and belongs in its own pass.

use vela_compile::Session;
use vela_hir::ModuleName;
use vela_span::FileId;
use vela_syntax::{Item, Program};

/// One thing that could be typed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Item_ {
    /// The text to insert.
    pub label: String,
    /// What kind of thing it is, for the list an editor shows beside it.
    pub detail: String,
}

/// Everything that could be typed at `offset`.
#[must_use]
pub fn at(session: &mut Session, file: FileId, offset: u32) -> Vec<Item_> {
    let parsed = session.parse(file);

    if widgets_wanted(session, file, &parsed.program, offset) {
        return widgets();
    }
    if labels_wanted(session, file, offset) {
        return labels(session, file);
    }

    let mut found = names(session, file, offset, &parsed.program);
    found.extend(declared(session, file));
    found.sort_by(|left, right| left.label.cmp(&right.label));
    found.dedup_by(|left, right| left.label == right.label);
    found
}

/// The widgets, which are what a screen body's line may begin with.
fn widgets() -> Vec<Item_> {
    vela_ui::WidgetRegistry::builtin()
        .names()
        .into_iter()
        .map(|name| Item_ {
            label: name.to_string(),
            detail: "widget".to_string(),
        })
        .collect()
}

/// The names in scope where the cursor is, by the checker's own rule.
fn names(session: &mut Session, file: FileId, offset: u32, tree: &Program) -> Vec<Item_> {
    let compiled = session.mir(file);
    vela_types::scope_at(tree, &compiled.env, offset)
        .into_iter()
        .map(|(name, ty)| Item_ {
            label: name,
            detail: ty.to_string(),
        })
        .collect()
}

/// The names the module declares, which are visible everywhere in it.
fn declared(session: &mut Session, file: FileId) -> Vec<Item_> {
    let collected = session.symbols(file);
    collected
        .module
        .definitions
        .values()
        .map(|definition| Item_ {
            label: definition.name.clone(),
            detail: definition.kind.keyword().to_string(),
        })
        .collect()
}

/// The labels reachable from here: this module's, and the ones an import names.
///
/// Qualified for the imported ones, because that is how a reference has to be written — offering a bare
/// name that does not resolve in this module would be a completion that produces a diagnostic.
fn labels(session: &mut Session, file: FileId) -> Vec<Item_> {
    let collected = session.symbols(file);
    let mut found: Vec<Item_> = collected
        .module
        .story
        .nodes
        .iter()
        .map(|node| Item_ {
            label: node.name.clone(),
            detail: "label".to_string(),
        })
        .collect();

    for (alias, import) in &collected.module.aliases {
        // A module the program does not have has no labels to offer; the `use` itself is still worth
        // completing, and is offered as the qualifier's own name.
        for name in labels_of(session, &import.module) {
            found.push(Item_ {
                label: format!("{alias}.{name}"),
                detail: format!("label · {}", import.module),
            });
        }
        found.push(Item_ {
            label: alias.clone(),
            detail: format!("module · {}", import.module),
        });
    }

    found.sort_by(|left, right| left.label.cmp(&right.label));
    found
}

/// The labels a module declares.
fn labels_of(session: &mut Session, module: &ModuleName) -> Vec<String> {
    let Some(file) = session
        .file_ids()
        .into_iter()
        .find(|file| session.module_of(*file) == Some(module))
    else {
        return Vec::new();
    };

    let collected = session.symbols(file);
    collected
        .module
        .story
        .nodes
        .iter()
        .map(|node| node.name.clone())
        .collect()
}

/// Whether the cursor is where a widget name may go: the start of a line inside a screen.
///
/// Two questions, and both are needed: inside a screen, because that is where the widget vocabulary
/// applies; and at the start of a line, because inside an `if` condition within that screen the answer
/// is an expression, not a widget.
fn widgets_wanted(session: &Session, file: FileId, tree: &Program, offset: u32) -> bool {
    let inside_a_screen = tree.items.iter().any(|item| match item {
        Item::Screen(screen) => contains(screen.span, offset),
        _ => false,
    });
    inside_a_screen && at_line_start(session, file, offset)
}

/// Whether the cursor is in a `jump` or `call`'s target, where only a label can go.
fn labels_wanted(session: &mut Session, file: FileId, offset: u32) -> bool {
    let parsed = session.parse(file);
    let mut in_a_transfer = false;
    each_statement(&parsed.program, &mut |statement| {
        let (span, is_transfer) = match statement {
            vela_syntax::Stmt::Jump(jump) => (jump.span, true),
            vela_syntax::Stmt::Call(call) => (call.span, true),
            _ => (statement.span(), false),
        };
        if is_transfer && contains(span, offset) {
            in_a_transfer = true;
        }
    });
    if in_a_transfer {
        return true;
    }

    // And the text, for the case the tree cannot answer at all: `jump ` with nothing after it yet does
    // not parse, and that is exactly the moment the author is asking what can go there. A completion
    // that disappears while the target is being typed is worse than one that never appeared.
    let source = session.sources().file(file);
    let text = source.text();
    let before = text.get(..offset as usize).unwrap_or(text);

    matches!(before.split_whitespace().next_back(), Some("jump" | "call"))
}

/// Visits every statement in a program, including the ones inside bodies.
fn each_statement(tree: &Program, visit: &mut impl FnMut(&vela_syntax::Stmt)) {
    for item in &tree.items {
        match item {
            Item::Label(decl) => statements(&decl.body, visit),
            Item::Function(decl) => statements(&decl.body, visit),
            _ => {}
        }
    }
}

/// Visits a statement list and everything nested in it.
fn statements(body: &[vela_syntax::Stmt], visit: &mut impl FnMut(&vela_syntax::Stmt)) {
    for statement in body {
        visit(statement);
        match statement {
            vela_syntax::Stmt::If(stmt) => {
                statements(&stmt.then_body, visit);
                for clause in &stmt.elifs {
                    statements(&clause.body, visit);
                }
                if let Some(else_body) = &stmt.else_body {
                    statements(else_body, visit);
                }
            }
            vela_syntax::Stmt::While(stmt) => statements(&stmt.body, visit),
            vela_syntax::Stmt::For(stmt) => statements(&stmt.body, visit),
            vela_syntax::Stmt::Match(stmt) => {
                for arm in &stmt.arms {
                    statements(&arm.body, visit);
                }
            }
            vela_syntax::Stmt::Menu(menu) => {
                for choice in &menu.choices {
                    statements(&choice.body, visit);
                }
            }
            _ => {}
        }
    }
}

/// Whether the cursor is at the start of what it is on: only whitespace before it on the line.
fn at_line_start(session: &Session, file: FileId, offset: u32) -> bool {
    let source = session.sources().file(file);
    let text = source.text();
    let start = text[..offset.min(text.len() as u32) as usize]
        .rfind('\n')
        .map_or(0, |index| index + 1);

    text.get(start..offset as usize)
        .is_some_and(|before| before.trim().is_empty())
}

/// Whether a span covers an offset.
fn contains(span: vela_span::Span, offset: u32) -> bool {
    span.start() <= offset && offset < span.end()
}
