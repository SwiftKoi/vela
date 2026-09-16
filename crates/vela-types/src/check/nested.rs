//! The statements that open a body.
//!
//! Split from `stmt` when that file reached the size budget, and the seam is the language's own: `if`,
//! `while`, `for`, `match`, and the choices of a `menu` all introduce statements, where assignments,
//! dialogue, and transfers do not. The interesting rule in here is the one about `match` — whether an
//! arm's pattern covers the payload of the value being matched, and what an arm's body sees.

use vela_syntax::{MatchArm, Stmt};

use crate::check::coverage;
use crate::check::run::Checker;
use crate::error;
use crate::ty::Ty;

impl Checker<'_> {
    /// Checks a statement that contains bodies, answering whether every path through it exits.
    ///
    /// `None` when the statement opens no body, which is how `statement` knows to handle it.
    pub(super) fn nested(&mut self, statement: &Stmt) -> Option<bool> {
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
}
