//! Lowering `match`.
//!
//! Arms are chained *per variant*, not globally: two arms can name the same variant with
//! different guards, and a guard that fails has to try the next arm for that variant before
//! falling through to the catch-all. That is enough machinery to be worth its own file.

use vela_syntax::MatchStmt;
use vela_types::Ty;

use crate::ir::{Place, StmtKind, Terminator, Value, VariantId};
use crate::lower::Lowerer;

impl Lowerer<'_> {
    /// `match`, dispatched on the enum's tag.
    ///
    /// Arms are chained *per variant*, not globally: two arms can name the same variant
    /// with different guards, and a guard that fails has to try the next arm for that
    /// variant before falling through to the catch-all. A `match` has no value
    /// (`LANGUAGE.md §10` defers match-as-expression), so every arm continues at the join,
    /// which is also where a catch-all's body lives.
    pub(crate) fn match_(&mut self, stmt: &MatchStmt) {
        let scrutinee = self.expr(&stmt.scrutinee);
        let enum_name = match self.type_of(&stmt.scrutinee) {
            Ty::Enum(name) => name,
            _ => String::new(),
        };

        let entry = self.body.current();
        let join = self.body.open();
        let fallback = self.body.open();
        let blocks: Vec<crate::ir::BlockId> = stmt.arms.iter().map(|_| self.body.open()).collect();

        let (chains, wildcard) = self.arm_groups(stmt, &enum_name);

        self.body.start(entry);
        let arms: Vec<(VariantId, crate::ir::BlockId)> = chains
            .iter()
            .filter_map(|(variant, indices)| Some((*variant, *blocks.get(*indices.first()?)?)))
            .collect();
        self.body.seal(Terminator::Dispatch {
            enum_name: enum_name.clone(),
            value: scrutinee,
            arms,
            // With a catch-all, the dispatch goes straight to the join, where its body is.
            else_: if wildcard.is_some() { join } else { fallback },
        });

        for (index, arm) in stmt.arms.iter().enumerate() {
            if wildcard == Some(index) {
                continue;
            }
            let Some(block) = blocks.get(index).copied() else {
                continue;
            };
            self.body.start(block);
            self.bind_arm(&scrutinee, arm, &enum_name);

            if let Some(guard) = &arm.guard {
                let condition = self.expr(guard);
                let taken = self.body.open();
                let next = self
                    .next_arm(&chains, &blocks, index, &enum_name, arm)
                    .unwrap_or(fallback);
                self.body.seal(Terminator::Branch {
                    cond: condition,
                    then_: taken,
                    else_: next,
                });
                self.body.start(taken);
            }

            self.statements(&arm.body);
            self.body.seal(Terminator::Goto(join));
        }

        // Nothing matched. Without guards the checker has proven this unreachable; with
        // them it is a fault rather than a silently skipped arm.
        self.body.start(fallback);
        self.body.seal(Terminator::Unreachable);

        self.body.start(join);
        if let Some(index) = wildcard
            && let Some(arm) = stmt.arms.get(index)
        {
            if let Some(guard) = &arm.guard {
                let condition = self.expr(guard);
                let taken = self.body.open();
                self.body.seal(Terminator::Branch {
                    cond: condition,
                    then_: taken,
                    else_: fallback,
                });
                self.body.start(taken);
            }
            self.statements(&arm.body);
        }
    }

    /// Groups arms by the variant they name, in source order, and finds the catch-all.
    pub(crate) fn arm_groups(
        &self,
        stmt: &MatchStmt,
        enum_name: &str,
    ) -> (Vec<(VariantId, Vec<usize>)>, Option<usize>) {
        let mut chains: Vec<(VariantId, Vec<usize>)> = Vec::new();
        let mut wildcard = None;

        for (index, arm) in stmt.arms.iter().enumerate() {
            let pattern = match &arm.pattern {
                Some(pattern) if !pattern.path.is_empty() => pattern,
                _ => {
                    wildcard = Some(index);
                    continue;
                }
            };
            let Some(name) = pattern.path.last() else {
                continue;
            };
            let id = self
                .module
                .enum_named(enum_name)
                .and_then(|definition| definition.index_of(name));
            if let Some(id) = id {
                match chains.iter_mut().find(|(known, _)| *known == id) {
                    Some((_, arms)) => arms.push(index),
                    None => chains.push((id, vec![index])),
                }
            }
        }

        (chains, wildcard)
    }

    /// The next arm to try when this one's guard fails.
    pub(crate) fn next_arm(
        &self,
        chains: &[(VariantId, Vec<usize>)],
        blocks: &[crate::ir::BlockId],
        index: usize,
        enum_name: &str,
        arm: &vela_syntax::MatchArm,
    ) -> Option<crate::ir::BlockId> {
        let name = arm.pattern.as_ref()?.path.last()?;
        let id = self.module.enum_named(enum_name)?.index_of(name)?;
        let (_, arms) = chains.iter().find(|(known, _)| *known == id)?;
        let position = arms.iter().position(|candidate| *candidate == index)?;
        blocks.get(*arms.get(position + 1)?).copied()
    }

    /// Binds an arm's payload names.
    pub(crate) fn bind_arm(
        &mut self,
        scrutinee: &Value,
        arm: &vela_syntax::MatchArm,
        enum_name: &str,
    ) {
        let Some(pattern) = &arm.pattern else {
            return;
        };
        if pattern.bindings.is_empty() {
            return;
        }
        let Some(variant) = pattern.path.last() else {
            return;
        };
        let payload: Vec<Ty> = self
            .module
            .enum_named(enum_name)
            .and_then(|definition| definition.index_of(variant))
            .map(|id| {
                self.module
                    .enum_named(enum_name)
                    .and_then(|definition| definition.variants.get(id.0 as usize))
                    .map(|definition| definition.fields.clone())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        for (position, binding) in pattern.bindings.iter().enumerate() {
            let ty = payload.get(position).cloned().unwrap_or(Ty::Unknown);
            let slot = self.declare(binding, ty);
            self.emit(
                StmtKind::EnumField {
                    dst: Place::Local(slot),
                    base: *scrutinee,
                    index: u32::try_from(position).unwrap_or(u32::MAX),
                },
                arm.span,
            );
        }
    }
}
