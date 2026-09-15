//! Inlining small function bodies.
//!
//! A call has a cost in per-frame logic, and a story's `fn`s are mostly a line or two —
//! a clamp, a lookup, a formatted string. Inlining them removes the frame without needing
//! an optimizer's worth of analysis.
//!
//! # What this inlines
//!
//! A body whose CFG is a single block ending in `Return`, with no call, no command, and no
//! more than [`BUDGET`] statements. That restriction is what makes the transformation a
//! *substitution* rather than a graph rewrite: the callee's slots are renamed to fresh
//! slots in the caller, its statements are spliced in front of the call, and the call is
//! replaced by a copy of its return value.
//!
//! Anything with a branch, a loop, or a jump needs block splicing and a rewrite of every
//! target, which is a different and much larger change. It is deliberately not done here:
//! a pass that is obviously correct is worth more than one that is clever, and the
//! differential harness is what proves the difference either way.

use crate::ir::{
    Body, Callee, FuncRef, LocalDecl, Module, Operand, Place, Slot, Stmt, StmtKind, Terminator,
    Value,
};
use crate::opt::{OptLevel, Pass};

/// The largest body that is worth inlining.
const BUDGET: usize = 12;

/// Inlines small calls.
pub struct InlineSmall;

impl Pass for InlineSmall {
    fn name(&self) -> &'static str {
        "inline_small"
    }

    fn level(&self) -> OptLevel {
        OptLevel::O2
    }

    fn run(&self, module: &mut Module) {
        // Candidates are copied out first. Inlining reads a callee while writing a caller,
        // and the two are the same `Vec` — so the readable set is taken before the mutable
        // walk begins.
        let candidates: Vec<(u32, Body)> = module
            .fns
            .iter()
            .enumerate()
            .filter(|(_, body)| inlinable(body))
            .filter_map(|(index, body)| Some((u32::try_from(index).ok()?, body.clone())))
            .collect();

        if candidates.is_empty() {
            return;
        }

        for body in module.fns.iter_mut().chain(module.labels.iter_mut()) {
            inline_into(body, &candidates);
        }
    }
}

/// Whether a body is small and simple enough to splice.
fn inlinable(body: &Body) -> bool {
    let [block] = body.blocks.as_slice() else {
        return false;
    };
    if !matches!(block.term, Terminator::Return(_)) {
        return false;
    }
    if block.stmts.len() > BUDGET {
        return false;
    }
    // A command belongs at its own yield site, and a call would make inlining recursive.
    block
        .stmts
        .iter()
        .all(|stmt| !matches!(stmt.kind, StmtKind::Call { .. } | StmtKind::Cmd { .. }))
}

/// Inlines every eligible call in one body.
fn inline_into(body: &mut Body, candidates: &[(u32, Body)]) {
    // The slot table is moved out so that fresh slots can be appended while a block is
    // being rewritten; putting it back is the last thing that happens.
    let mut locals = std::mem::take(&mut body.locals);

    for index in 0..body.blocks.len() {
        let mut incoming = std::mem::take(&mut body.blocks[index].stmts);
        let mut outgoing = Vec::with_capacity(incoming.len());

        for stmt in incoming.drain(..) {
            match splice(&stmt, candidates, &mut locals) {
                Some(replacement) => outgoing.extend(replacement),
                None => outgoing.push(stmt),
            }
        }

        body.blocks[index].stmts = outgoing;
    }

    body.locals = locals;
}

/// The statements a call expands to, or `None` if it cannot be inlined.
fn splice(
    stmt: &Stmt,
    candidates: &[(u32, Body)],
    locals: &mut Vec<LocalDecl>,
) -> Option<Vec<Stmt>> {
    let StmtKind::Call {
        dst,
        callee: Callee::Direct(FuncRef::Defined(index)),
        args,
    } = &stmt.kind
    else {
        return None;
    };
    let (_, target) = candidates
        .iter()
        .find(|(candidate, _)| candidate == index)?;

    // Every one of the callee's slots becomes a fresh slot in the caller, so that two
    // inlined copies of the same body cannot collide.
    let mut map: Vec<Slot> = Vec::with_capacity(target.locals.len());
    for local in &target.locals {
        map.push(Slot(u32::try_from(locals.len()).ok()?));
        locals.push(local.clone());
    }

    let mut out = Vec::new();

    // Arguments arrive in the parameter slots.
    for (position, param) in target.params.iter().enumerate() {
        let argument = *args.get(position)?;
        out.push(Stmt {
            kind: StmtKind::Load {
                dst: Place::Local(rename(*param, &map)),
                src: Operand::Value(argument),
            },
            span: stmt.span,
        });
    }

    let block = target.blocks.first()?;
    for inner in &block.stmts {
        out.push(rewrite_stmt(inner, &map)?);
    }

    let Terminator::Return(returned) = &block.term else {
        return None;
    };
    if let (Some(dst), Some(value)) = (dst, returned) {
        out.push(Stmt {
            kind: StmtKind::Load {
                dst: dst.clone(),
                src: Operand::Value(rename_value(*value, &map)),
            },
            span: stmt.span,
        });
    }

    Some(out)
}

/// A slot's new number in the caller.
fn rename(slot: Slot, map: &[Slot]) -> Slot {
    map.get(slot.0 as usize).copied().unwrap_or(slot)
}

/// A value, renamed.
fn rename_value(value: Value, map: &[Slot]) -> Value {
    match value {
        Value::Slot(slot) => Value::Slot(rename(slot, map)),
        Value::Const(id) => Value::Const(id),
    }
}

/// A place, renamed.
fn rename_place(place: &Place, map: &[Slot]) -> Place {
    match place {
        Place::Local(slot) => Place::Local(rename(*slot, map)),
        Place::Default(id) => Place::Default(*id),
        Place::Field { base, field } => Place::Field {
            base: Box::new(rename_place(base, map)),
            field: field.clone(),
        },
        Place::Index { base, index } => Place::Index {
            base: Box::new(rename_place(base, map)),
            index: rename_value(*index, map),
        },
    }
}

/// An operand, renamed.
fn rename_operand(operand: &Operand, map: &[Slot]) -> Operand {
    match operand {
        Operand::Value(value) => Operand::Value(rename_value(*value, map)),
        Operand::Default(id) => Operand::Default(*id),
        Operand::Field { base, field } => Operand::Field {
            base: rename_value(*base, map),
            field: field.clone(),
        },
        Operand::Index { base, index } => Operand::Index {
            base: rename_value(*base, map),
            index: rename_value(*index, map),
        },
        Operand::Len { base } => Operand::Len {
            base: rename_value(*base, map),
        },
    }
}

/// A statement, renamed.
fn rewrite_stmt(stmt: &Stmt, map: &[Slot]) -> Option<Stmt> {
    let kind = match &stmt.kind {
        StmtKind::Assign { dst, op, a, b } => StmtKind::Assign {
            dst: rename_place(dst, map),
            op: *op,
            a: rename_value(*a, map),
            b: rename_value(*b, map),
        },
        StmtKind::AssignUn { dst, op, a } => StmtKind::AssignUn {
            dst: rename_place(dst, map),
            op: *op,
            a: rename_value(*a, map),
        },
        StmtKind::Load { dst, src } => StmtKind::Load {
            dst: rename_place(dst, map),
            src: rename_operand(src, map),
        },
        StmtKind::ListNew { dst, items } => StmtKind::ListNew {
            dst: rename_place(dst, map),
            items: items.iter().map(|item| rename_value(*item, map)).collect(),
        },
        StmtKind::MapNew { dst, entries } => StmtKind::MapNew {
            dst: rename_place(dst, map),
            entries: entries
                .iter()
                .map(|(key, value)| (rename_value(*key, map), rename_value(*value, map)))
                .collect(),
        },
        StmtKind::StructNew { dst, name, fields } => StmtKind::StructNew {
            dst: rename_place(dst, map),
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(field, value)| (field.clone(), rename_value(*value, map)))
                .collect(),
        },
        StmtKind::EnumNew {
            dst,
            enum_name,
            variant,
            args,
        } => StmtKind::EnumNew {
            dst: rename_place(dst, map),
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            args: args.iter().map(|arg| rename_value(*arg, map)).collect(),
        },
        StmtKind::EnumField { dst, base, index } => StmtKind::EnumField {
            dst: rename_place(dst, map),
            base: rename_value(*base, map),
            index: *index,
        },
        StmtKind::IsNone { dst, base } => StmtKind::IsNone {
            dst: rename_place(dst, map),
            base: rename_value(*base, map),
        },
        StmtKind::Unwrap { dst, base } => StmtKind::Unwrap {
            dst: rename_place(dst, map),
            base: rename_value(*base, map),
        },
        // A call or a command would have made the body ineligible, so reaching one here
        // means `inlinable` and this disagree — which is a bug, not a case to handle.
        StmtKind::Call { .. } | StmtKind::Cmd { .. } => return None,
    };

    Some(Stmt {
        kind,
        span: stmt.span,
    })
}
