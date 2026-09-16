//! Typing statements.

use vela_syntax::{Expr, MatchArm, Pattern, Stmt, VarStmt, WaitEvent};

use crate::check::coverage;
use crate::check::run::Checker;
use crate::error;
use crate::lower::lower;
use crate::ty::Ty;

impl Checker<'_> {
    /// Checks a statement list, answering whether every path through it exits.
    ///
    /// The answer is what `E4002` asks of a function body, and it is computed **here**, in the walk
    /// that types the statements, because deciding it needs the type of a `match`'s scrutinee —
    /// and the scope that makes that type computable exists only while walking. A second,
    /// structural pass over the same body would have to re-derive the scope, and a second
    /// definition of "this path exits" is how a checker ends up reporting a function for not
    /// returning *and* accepting the very `match` it is complaining about.
    pub(crate) fn body(&mut self, statements: &[Stmt]) -> bool {
        let mut exits = false;
        for statement in statements {
            // Not `||=`: every statement is checked, including one written after a path that
            // always exits. A mistake in unreachable code is still a mistake.
            exits |= self.statement(statement);
        }
        exits
    }

    /// Checks one statement, answering whether every path through it exits.
    pub(crate) fn statement(&mut self, statement: &Stmt) -> bool {
        if let Some(exits) = self.nested(statement) {
            return exits;
        }
        self.simple(statement).unwrap_or(false)
    }

    /// Checks a statement that opens no body, answering whether it exits.
    ///
    /// Split from `statement` for two reasons: the arms are where the exit rule is decided for a
    /// leaf statement, and keeping them together makes that visible rather than spread through a
    /// long match. `None` means the statement is one `nested` handles — unreachable here, and
    /// written as an `Option` so that a statement added to one match and forgotten in the other is
    /// a compile error rather than a statement nobody checks.
    fn simple(&mut self, statement: &Stmt) -> Option<bool> {
        match statement {
            Stmt::Var(stmt) => Some(self.local(stmt)),

            Stmt::Assign(stmt) => {
                let target = self.expr(&stmt.target);
                let value = self.expr(&stmt.value);
                // The same rule for `=` and `+=`: what is stored has to fit what holds it.
                self.expect(&target, &value, stmt.value.span());
                Some(false)
            }

            Stmt::Expr(stmt) => {
                self.expr(&stmt.expr);
                Some(false)
            }

            Stmt::Say(say) => {
                self.expr(&say.line);
                for (_, value) in &say.options {
                    self.expr(value);
                }
                Some(false)
            }

            Stmt::Return(stmt) => {
                if let Some(value) = &stmt.value {
                    self.expr(value);
                }
                // The one statement that leaves a body *and* says so.
                Some(true)
            }

            Stmt::Wait(stmt) => {
                if let WaitEvent::Duration(value) = &stmt.event {
                    self.expr(value);
                }
                Some(false)
            }

            Stmt::Audio(stmt) => {
                if let Some(source) = &stmt.source {
                    self.expr(source);
                }
                if let Some(fade) = &stmt.fade {
                    self.expr(fade);
                }
                Some(false)
            }

            // These carry paths, not expressions: a `jump` target is a label. Staging, audio, and a
            // transition do not end a body, and a `jump` transfers between labels rather than out
            // of a function, so none of them is a `return`.
            Stmt::Jump(_) | Stmt::Call(_) | Stmt::Stage(_) | Stmt::With(_) | Stmt::Error { .. } => {
                Some(false)
            }

            // `nested` handles these before `simple` is reached.
            Stmt::Menu(_) | Stmt::If(_) | Stmt::While(_) | Stmt::For(_) | Stmt::Match(_) => None,
        }
    }

    /// Checks a local declaration, which is the one statement that introduces a name.
    ///
    /// Its own method because the two decisions it makes are worth reading together: where the type
    /// comes from — an annotation, or the initialiser — and that the name is recorded for a caller that
    /// asked about an offset (`check::at`). It never exits, so it answers `false`.
    fn local(&mut self, stmt: &VarStmt) -> bool {
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

        // Recorded against the *declaration* rather than the name: the tree keeps a `var` as one span,
        // so `var name: T = ` is the closest thing to "the name" there is. It stops before the
        // initialiser, which is what keeps a hover on `1` in `var n = 1` an answer about the literal
        // rather than about `n`.
        self.note(
            stmt.span.to(stmt.value.span()),
            crate::check::at::Found::Name {
                name: stmt.name.clone(),
                ty: ty.clone(),
            },
        );
        self.scope.insert(stmt.name.clone(), ty);
        false
    }

    /// Checks a statement that contains bodies, answering whether every path through it exits.
    ///
    /// `None` when the statement opens no body, which is how `statement` knows to handle it.
    fn nested(&mut self, statement: &Stmt) -> Option<bool> {
        match statement {
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
                // A menu offers a choice; which one is taken is not a property of the code.
                Some(false)
            }

            Stmt::If(stmt) => {
                self.condition(&stmt.condition);
                // Every body is checked before the answers are combined, so a branch that does
                // not exit is still checked as thoroughly as one that does. `&=` deliberately
                // does not short-circuit.
                let mut exits = self.body(&stmt.then_body);
                for clause in &stmt.elifs {
                    self.condition(&clause.condition);
                    exits &= self.body(&clause.body);
                }
                // No `else` leaves the condition being false as a way out.
                exits &= match &stmt.else_body {
                    Some(body) => self.body(body),
                    None => false,
                };
                Some(exits)
            }

            Stmt::While(stmt) => {
                self.condition(&stmt.condition);
                self.body(&stmt.body);
                // A loop may run no times at all.
                Some(false)
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
                // A sequence may be empty.
                Some(false)
            }

            Stmt::Match(stmt) => {
                let scrutinee = self.expr(&stmt.scrutinee);
                Some(self.match_arms(&scrutinee, &stmt.arms, stmt.span))
            }

            _ => None,
        }
    }

    /// Checks a match's arms: whether each is reachable, whether they cover the case, and whether
    /// every arm exits.
    ///
    /// The last answer is `E4002`'s: a `match` whose arms all exit is a path that always exits —
    /// whether it covers the cases with an `else` or by naming every variant. Requiring an `else`
    /// arm, as a purely structural reading does, reports a function for not returning while
    /// accepting the very match it is looking at.
    ///
    /// A missing arm is *not* part of the answer: `E4001` already reports it, and the function
    /// not returning is the same mistake seen from further away. One mistake, one diagnostic.
    fn match_arms(&mut self, scrutinee: &Ty, arms: &[MatchArm], span: vela_span::Span) -> bool {
        // Where an `else` or `_` arm is: everything written before it can fall through to it.
        let open = arms.iter().position(|arm| {
            arm.pattern
                .as_ref()
                .is_none_or(|pattern| pattern.path.is_empty())
        });
        let mut exits = !arms.is_empty();

        for (index, arm) in arms.iter().enumerate() {
            if open.is_some_and(|position| position < index) {
                // The body is still checked, so a mistake inside an unreachable arm is not
                // hidden — it is reported alongside a note that it never runs.
                self.report(error::unreachable_arm(arm.span));
            }

            // A pattern's bindings belong to their own arm, so the scope is put back
            // after each one.
            let saved = self.scope.snapshot();
            if let Some(pattern) = &arm.pattern {
                self.bind_pattern(scrutinee, pattern);
            }
            let guarded = arm.guard.is_some();
            if let Some(guard) = &arm.guard {
                self.expr(guard);
            }
            let arm_exits = self.body(&arm.body);
            self.scope.restore(saved);

            // The arm's own path exits when its body does. A guard adds a second path — the one
            // where it does not hold — and that one lands on whatever covers the rest, which is
            // only an `else` written after this arm.
            let falls_through_to_an_exit =
                !guarded || open.is_some_and(|position| position > index);
            exits &= arm_exits && falls_through_to_an_exit;
        }

        let missing = coverage::missing(self.env, scrutinee, arms);
        if !missing.is_empty() {
            self.report(error::non_exhaustive(&missing, span));
        }

        exits
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
