//! Typing statements.

use std::collections::BTreeSet;

use vela_syntax::{Expr, MatchArm, Pattern, Stmt, WaitEvent};

use crate::check::run::Checker;
use crate::error;
use crate::lower::lower;
use crate::ty::Ty;

impl Checker<'_> {
    /// Checks a statement list.
    pub(crate) fn body(&mut self, statements: &[Stmt]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    /// Checks one statement, introducing any binding it declares.
    pub(crate) fn statement(&mut self, statement: &Stmt) {
        if self.nested(statement) {
            return;
        }

        match statement {
            Stmt::Var(stmt) => {
                let value = self.expr(&stmt.value);
                let ty = match &stmt.ty {
                    Some(annotation) => {
                        let declared = lower(annotation, self.env);
                        self.expect(&declared, &value, stmt.value.span());
                        declared
                    }
                    // An unwritten type is inferred from the initialiser, which is what
                    // `LANGUAGE.md §5.4` promises for locals.
                    None => value,
                };
                self.scope.insert(stmt.name.clone(), ty);
            }

            Stmt::Assign(stmt) => {
                let target = self.expr(&stmt.target);
                let value = self.expr(&stmt.value);
                // The same rule for `=` and `+=`: what is stored has to fit what holds it.
                self.expect(&target, &value, stmt.value.span());
            }

            Stmt::Expr(stmt) => {
                self.expr(&stmt.expr);
            }

            Stmt::Say(say) => {
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
                        self.condition(condition);
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

            // These carry paths, not expressions: a `jump` target is a label.
            Stmt::Jump(_) | Stmt::Call(_) | Stmt::Stage(_) | Stmt::With(_) | Stmt::Error { .. } => {
            }
        }
    }

    /// Checks a statement that contains bodies, reporting whether it handled one.
    fn nested(&mut self, statement: &Stmt) -> bool {
        match statement {
            Stmt::If(stmt) => {
                self.condition(&stmt.condition);
                self.body(&stmt.then_body);
                for clause in &stmt.elifs {
                    self.condition(&clause.condition);
                    self.body(&clause.body);
                }
                if let Some(body) = &stmt.else_body {
                    self.body(body);
                }
                true
            }

            Stmt::While(stmt) => {
                self.condition(&stmt.condition);
                self.body(&stmt.body);
                true
            }

            Stmt::For(stmt) => {
                let iterable = self.expr(&stmt.iterable);
                // The binding's type comes from what is being iterated, so a loop over
                // `list<int>` gives an `int` rather than an unknown.
                let element = match iterable {
                    Ty::List(element) => *element,
                    _ => Ty::Unknown,
                };

                let saved = self.scope.snapshot();
                self.scope.insert(stmt.binding.clone(), element);
                self.body(&stmt.body);
                self.scope.restore(saved);
                true
            }

            Stmt::Match(stmt) => {
                let scrutinee = self.expr(&stmt.scrutinee);
                self.match_arms(&scrutinee, &stmt.arms, stmt.span);
                true
            }

            _ => false,
        }
    }

    /// Checks a match's arms: whether each is reachable, and whether they cover the case.
    fn match_arms(&mut self, scrutinee: &Ty, arms: &[MatchArm], span: vela_span::Span) {
        let mut covered: BTreeSet<String> = BTreeSet::new();
        let mut open = false;

        for arm in arms {
            if open {
                // The body is still checked, so a mistake inside an unreachable arm is not
                // hidden — it is reported alongside a note that it never runs.
                self.report(error::unreachable_arm(arm.span));
            }

            match &arm.pattern {
                // `else` and `_` both cover everything that is left.
                None => open = true,
                Some(pattern) if pattern.path.is_empty() => open = true,
                Some(pattern) => {
                    if let Some(variant) = pattern.path.last() {
                        covered.insert(variant.clone());
                    }
                }
            }

            // A pattern's bindings belong to their own arm, so the scope is put back
            // after each one.
            let saved = self.scope.snapshot();
            if let Some(pattern) = &arm.pattern {
                self.bind_pattern(scrutinee, pattern);
            }
            if let Some(guard) = &arm.guard {
                self.expr(guard);
            }
            self.body(&arm.body);
            self.scope.restore(saved);
        }

        // Only an enum has cases to miss. Anything else is covered by whatever arms are
        // written, and reasoning further would need more of the language than exists yet.
        if open {
            return;
        }
        let Ty::Enum(enum_name) = scrutinee else {
            return;
        };
        let Some(shape) = self.env.enum_shape(enum_name) else {
            return;
        };

        let missing: Vec<String> = shape
            .variants
            .keys()
            .filter(|variant| !covered.contains(variant.as_str()))
            .cloned()
            .collect();
        if !missing.is_empty() {
            self.report(error::non_exhaustive(&missing, span));
        }
    }

    /// Checks that an expression is usable as a condition.
    fn condition(&mut self, expr: &Expr) {
        // A literal condition decides the branch where it is written, which is almost
        // always something left over rather than something meant.
        if let Expr::Bool { span, .. } = expr {
            self.report(error::constant_condition(*span));
        }

        let ty = self.expr(expr);
        if ty != Ty::Bool && ty != Ty::Unknown {
            self.report(error::mismatch(&Ty::Bool, &ty, expr.span()));
        }
    }

    /// Binds a pattern's names to the payload types of the variant it names.
    fn bind_pattern(&mut self, scrutinee: &Ty, pattern: &Pattern) {
        let Ty::Enum(enum_name) = scrutinee else {
            return;
        };
        let Some(variant) = pattern.path.last() else {
            return;
        };

        // Cloned out, so the borrow of the environment ends before the scope is written.
        let payload = self
            .env
            .enum_shape(enum_name)
            .and_then(|shape| shape.variants.get(variant))
            .and_then(|payload| payload.as_ref())
            .cloned();
        let Some(payload) = payload else {
            return;
        };

        for (binding, ty) in pattern.bindings.iter().zip(payload) {
            self.scope.insert(binding.clone(), ty);
        }
    }
}
