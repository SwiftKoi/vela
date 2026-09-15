//! Resolving the names a module's bodies refer to.
//!
//! This is what turns a variable typo into a diagnostic before the game runs — the case
//! that most often reaches a playtester today, because nothing in the language as it
//! stands can catch it.

use std::collections::BTreeSet;

use vela_diag::Diagnostic;
use vela_syntax::{Expr, Item, Program, Stmt, StrPart, WaitEvent};

use crate::collect::Module;
use crate::def::DefKind;
use crate::error;

/// Names the language provides rather than a project defining them.
///
/// Finalized with the standard library at M3; until then this is the small set the specs
/// already use: the conversions in `LANGUAGE.md §5.5` and the effects in `RUNTIME.md §9`.
const BUILTINS: &[&str] = &["str", "int", "float", "bool", "rand", "now"];

/// Resolves every name a module's bodies refer to, reporting the ones that mean nothing.
#[must_use]
pub fn resolve_names(module: &Module, tree: &Program) -> Vec<Diagnostic> {
    let mut resolver = Resolver {
        module,
        scope: BTreeSet::new(),
        diagnostics: Vec::new(),
    };

    for item in &tree.items {
        match item {
            Item::Label(decl) => resolver.body(&decl.body),
            Item::Function(decl) => {
                resolver.scope = decl.params.iter().map(|param| param.name.clone()).collect();
                resolver.body(&decl.body);
            }
            _ => {}
        }
    }

    resolver.diagnostics
}

/// Walks one module's bodies with the names currently in scope.
///
/// The scope is flat rather than block-scoped: a `var` is visible from where it is written
/// to the end of the body. That is laxer than the language, so it can only *miss* an
/// error, never invent one — and inventing one is what would teach people to turn the
/// diagnostic off.
struct Resolver<'a> {
    module: &'a Module,
    scope: BTreeSet<String>,
    diagnostics: Vec<Diagnostic>,
}

impl Resolver<'_> {
    /// Walks a statement list.
    fn body(&mut self, statements: &[Stmt]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    /// Walks one statement, adding any binding it introduces to the scope.
    fn statement(&mut self, statement: &Stmt) {
        if self.nested(statement) {
            return;
        }

        match statement {
            Stmt::Say(say) => {
                if let Some(speaker) = &say.speaker {
                    // A speaker must be a declared character. Without this, a typo'd
                    // speaker silently becomes narration attributed to nobody, which is
                    // the kind of thing only a playtester notices.
                    let declared = self
                        .module
                        .definitions
                        .get(speaker)
                        .is_some_and(|definition| definition.kind == DefKind::Character);
                    if !declared {
                        self.diagnostics
                            .push(error::undefined_character(speaker, say.span));
                    }
                }

                self.expr(&say.line);
                for (_, value) in &say.options {
                    self.expr(value);
                }
            }
            Stmt::Menu(menu) => {
                if let Some(prompt) = &menu.prompt {
                    self.expr(prompt);
                }
                for choice in &menu.choices {
                    if let Some(condition) = &choice.condition {
                        self.expr(condition);
                    }
                    self.body(&choice.body);
                }
            }
            Stmt::Return(stmt) => {
                if let Some(value) = &stmt.value {
                    self.expr(value);
                }
            }
            Stmt::Wait(stmt) => {
                if let WaitEvent::Duration(value) = &stmt.event {
                    self.expr(value);
                }
            }
            Stmt::Audio(stmt) => {
                if let Some(source) = &stmt.source {
                    self.expr(source);
                }
                if let Some(fade) = &stmt.fade {
                    self.expr(fade);
                }
            }
            // Handled by `nested` before this match runs.
            Stmt::If(_) | Stmt::While(_) | Stmt::For(_) | Stmt::Match(_) => {}
            Stmt::Var(stmt) => {
                self.expr(&stmt.value);
                // Declared after its own initialiser, so `var x = x` refers to an outer
                // `x` rather than to itself.
                self.scope.insert(stmt.name.clone());
            }
            Stmt::Assign(stmt) => {
                self.expr(&stmt.target);
                self.expr(&stmt.value);
            }
            Stmt::Expr(stmt) => self.expr(&stmt.expr),
            // These carry paths, not expressions: a `jump` target is a label.
            Stmt::Jump(_) | Stmt::Call(_) | Stmt::Stage(_) | Stmt::With(_) | Stmt::Error { .. } => {
            }
        }
    }

    /// Walks a statement that contains nested bodies, reporting whether it handled one.
    ///
    /// Split out because control flow descends: these are the long arms, and the scope
    /// rules they carry — a loop binding, a match binding — are worth reading together
    /// rather than buried among the statements that do not open a body.
    fn nested(&mut self, statement: &Stmt) -> bool {
        match statement {
            Stmt::If(stmt) => {
                self.expr(&stmt.condition);
                self.body(&stmt.then_body);
                for clause in &stmt.elifs {
                    self.expr(&clause.condition);
                    self.body(&clause.body);
                }
                if let Some(body) = &stmt.else_body {
                    self.body(body);
                }
            }
            Stmt::While(stmt) => {
                self.expr(&stmt.condition);
                self.body(&stmt.body);
            }
            Stmt::For(stmt) => {
                self.expr(&stmt.iterable);
                self.scope.insert(stmt.binding.clone());
                self.body(&stmt.body);
            }
            Stmt::Match(stmt) => {
                self.expr(&stmt.scrutinee);
                for arm in &stmt.arms {
                    if let Some(guard) = &arm.guard {
                        self.expr(guard);
                    }
                    if let Some(pattern) = &arm.pattern {
                        for binding in &pattern.bindings {
                            self.scope.insert(binding.clone());
                        }
                    }
                    self.body(&arm.body);
                }
            }
            _ => return false,
        }
        true
    }

    /// Walks an expression.
    fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Name { span, name } => {
                if !self.known(name) {
                    self.diagnostics.push(error::undefined_name(name, *span));
                }
            }
            // Only the base of a field is a name; the field itself is not in scope.
            Expr::Field { base, .. } => self.expr(base),
            Expr::Call { callee, args, .. } => {
                self.expr(callee);
                for argument in args {
                    self.expr(argument);
                }
            }
            Expr::Index { base, index, .. } => {
                self.expr(base);
                self.expr(index);
            }
            Expr::Unary { operand, .. } => self.expr(operand),
            Expr::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            Expr::Paren { inner, .. } => self.expr(inner),
            Expr::If {
                cond, then_, else_, ..
            } => {
                self.expr(cond);
                self.expr(then_);
                self.expr(else_);
            }
            Expr::List { items, .. } => {
                for item in items {
                    self.expr(item);
                }
            }
            Expr::Map { entries, .. } => {
                for (key, value) in entries {
                    self.expr(key);
                    self.expr(value);
                }
            }
            Expr::Str { parts, .. } => {
                for part in parts {
                    if let StrPart::Interpolation { expr, .. } = part {
                        self.expr(expr);
                    }
                }
            }
            Expr::Lambda { params, body, .. } => {
                // A lambda's parameters shadow, so the scope is restored afterwards.
                let outer = self.scope.clone();
                for param in params {
                    self.scope.insert(param.name.clone());
                }
                self.expr(body);
                self.scope = outer;
            }
            Expr::Int { .. }
            | Expr::Float { .. }
            | Expr::Path { .. }
            | Expr::Bool { .. }
            | Expr::None { .. }
            | Expr::Error { .. } => {}
        }
    }

    /// Whether a name means something here.
    fn known(&self, name: &str) -> bool {
        BUILTINS.contains(&name)
            || self.scope.contains(name)
            || self.module.definitions.contains_key(name)
            || self.module.aliases.contains_key(name)
    }
}
