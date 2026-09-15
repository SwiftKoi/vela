//! Emitting statements.
//!
//! Dispatched by *family* rather than by variant, the same way the interpreter executes
//! them: arithmetic, copies, calls, aggregate construction, and payload reads are five
//! different jobs that happen to arrive through one function, and each is small enough to
//! read on its own.
//!
//! The one thing worth knowing before reading any of it: an aggregate is a **value**, so
//! `xs[0] = v` is a rebuild rather than a mutation, and `store` walks outward from the
//! innermost place to put each level back.

use vela_mir::{Callee, FuncRef, Place, Stmt, StmtKind, Value as MirValue};
use vela_types::Ty;

use crate::op::{Op, Operand};

use super::body::Emitter;

impl Emitter<'_> {
    /// Emits one statement.
    pub(super) fn statement(&mut self, stmt: &Stmt) {
        self.span = stmt.span;

        match &stmt.kind {
            StmtKind::Assign { .. } | StmtKind::AssignUn { .. } => self.assign(stmt),
            StmtKind::Load { .. } => self.copy(stmt),
            StmtKind::Call { .. } => self.call_statement(stmt),
            StmtKind::Cmd { .. } => self.command(stmt),
            StmtKind::ListNew { .. }
            | StmtKind::MapNew { .. }
            | StmtKind::StructNew { .. }
            | StmtKind::EnumNew { .. } => self.construct(stmt),
            StmtKind::EnumField { .. } | StmtKind::IsNone { .. } | StmtKind::Unwrap { .. } => {
                self.optional(stmt)
            }
        }
    }

    /// `dst = a op b`, and its prefix form.
    fn assign(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Assign { dst, op, a, b } => {
                let (a, b, op) = (*a, *b, *op);
                let binary = self.binary_op(op, self.ty_of(a));
                let dst = dst.clone();
                self.store(&dst, &mut |this| {
                    this.value(a);
                    this.value(b);
                    this.emit(binary, Operand::None);
                });
            }
            StmtKind::AssignUn { dst, op, a } => {
                let (a, op) = (*a, *op);
                let unary = self.unary_op(op, self.ty_of(a));
                let dst = dst.clone();
                self.store(&dst, &mut |this| {
                    this.value(a);
                    this.emit(unary, Operand::None);
                });
            }
            _ => {}
        }
    }

    /// `dst = src`
    fn copy(&mut self, stmt: &Stmt) {
        let StmtKind::Load { dst, src } = &stmt.kind else {
            return;
        };
        let (dst, src) = (dst.clone(), src.clone());
        self.load(&dst, &src);
    }

    /// A call whose result is kept, or discarded.
    fn call_statement(&mut self, stmt: &Stmt) {
        let StmtKind::Call { dst, callee, args } = &stmt.kind else {
            return;
        };
        let (dst, callee, args) = (dst.clone(), *callee, args.clone());
        self.call(dst.as_ref(), callee, &args);
    }

    /// Building a presentation command, which the next `Yield` hands over.
    fn command(&mut self, stmt: &Stmt) {
        let StmtKind::Cmd { kind, args } = &stmt.kind else {
            return;
        };
        let (kind, args) = (*kind, args.clone());

        for arg in &args {
            self.value(*arg);
        }
        let types: Vec<Ty> = args.iter().map(|arg| self.ty_of(*arg)).collect();
        let variant = self.builder.command_schema(kind, &types);
        let count = u32::try_from(args.len()).unwrap_or(u32::MAX);
        self.emit(Op::Cmd, Operand::Pair(variant, count));
    }

    /// Building a list, a map, a struct, or a variant.
    fn construct(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::ListNew { dst, items } => {
                let (dst, items) = (dst.clone(), items.clone());
                let count = u32::try_from(items.len()).unwrap_or(u32::MAX);
                self.store(&dst, &mut |this| {
                    for item in &items {
                        this.value(*item);
                    }
                    this.emit(Op::ListNew, Operand::U32(count));
                });
            }
            StmtKind::MapNew { dst, entries } => {
                let (dst, entries) = (dst.clone(), entries.clone());
                let count = u32::try_from(entries.len()).unwrap_or(u32::MAX);
                self.store(&dst, &mut |this| {
                    for (key, value) in &entries {
                        this.value(*key);
                        this.value(*value);
                    }
                    this.emit(Op::MapNew, Operand::U32(count));
                });
            }
            StmtKind::StructNew { dst, name, fields } => {
                let (dst, name, fields) = (dst.clone(), name.clone(), fields.clone());
                let id = self
                    .builder
                    .struct_ids
                    .get(&name)
                    .copied()
                    .unwrap_or(u32::MAX);
                let count = u32::try_from(fields.len()).unwrap_or(u32::MAX);
                self.store(&dst, &mut |this| {
                    for (_, value) in &fields {
                        this.value(*value);
                    }
                    this.emit(Op::StructNew, Operand::Pair(id, count));
                });
            }
            StmtKind::EnumNew {
                dst,
                enum_name,
                variant,
                args,
            } => {
                let dst = dst.clone();
                let (enum_name, variant, args) = (enum_name.clone(), variant.clone(), args.clone());
                let enum_id = self
                    .builder
                    .enum_ids
                    .get(&enum_name)
                    .copied()
                    .unwrap_or(u32::MAX);
                let variant_id = self.variant_index(&enum_name, &variant);
                self.store(&dst, &mut |this| {
                    for arg in &args {
                        this.value(*arg);
                    }
                    this.emit(Op::EnumNew, Operand::Pair(enum_id, variant_id));
                });
            }
            _ => {}
        }
    }

    /// Reading a variant's payload, or asking whether an optional has one.
    fn optional(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::EnumField { dst, base, index } => {
                let (dst, base, index) = (dst.clone(), *base, *index);
                self.store(&dst, &mut |this| {
                    this.value(base);
                    this.emit(Op::EnumField, Operand::U32(index));
                });
            }
            StmtKind::IsNone { dst, base } => {
                let (dst, base) = (dst.clone(), *base);
                self.store(&dst, &mut |this| {
                    this.value(base);
                    this.emit(Op::IsNone, Operand::None);
                });
            }
            StmtKind::Unwrap { dst, base } => {
                let (dst, base) = (dst.clone(), *base);
                self.store(&dst, &mut |this| {
                    this.value(base);
                    this.emit(Op::Unwrap, Operand::None);
                });
            }
            _ => {}
        }
    }

    /// Emits a call, storing its result if it has one.
    fn call(&mut self, dst: Option<&Place>, callee: Callee, args: &[MirValue]) {
        let arity = u32::try_from(args.len()).unwrap_or(u32::MAX);

        match callee {
            Callee::Direct(FuncRef::Builtin(builtin)) => {
                let op = match builtin {
                    vela_mir::Builtin::Str => Op::ToStr,
                    vela_mir::Builtin::Int => Op::ToInt,
                    vela_mir::Builtin::Float => Op::ToFloat,
                    vela_mir::Builtin::Bool => Op::ToBool,
                };
                let first = args.first().copied();
                self.store_or_discard(dst, &mut |this| {
                    if let Some(argument) = first {
                        this.value(argument);
                    }
                    this.emit(op, Operand::None);
                });
            }
            Callee::Direct(FuncRef::Defined(index)) => {
                self.store_or_discard(dst, &mut |this| {
                    for arg in args {
                        this.value(*arg);
                    }
                    this.emit(Op::CallFn, Operand::U32(index));
                });
            }
            Callee::Direct(FuncRef::Effect(index)) => {
                self.store_or_discard(dst, &mut |this| {
                    for arg in args {
                        this.value(*arg);
                    }
                    this.emit(Op::CallEffect, Operand::Pair(index, arity));
                });
            }
            Callee::Indirect(function) => {
                // The arguments first, then the function, because `CallValue` pops the
                // function last — which is where the stack puts it without a swap.
                self.store_or_discard(dst, &mut |this| {
                    for arg in args {
                        this.value(*arg);
                    }
                    this.value(function);
                    this.emit(Op::CallValue, Operand::U32(arity));
                });
            }
        }
    }

    /// Stores a computed value, or discards it when nothing wants it.
    fn store_or_discard(&mut self, dst: Option<&Place>, value: &mut dyn FnMut(&mut Self)) {
        match dst {
            Some(place) => {
                let place = place.clone();
                self.store(&place, value);
            }
            None => {
                value(self);
                // A call whose result nobody keeps still leaves it on the stack. Popping is
                // not an optimization: rule 1 requires every path into a block to arrive
                // with the same depth.
                self.emit(Op::Pop, Operand::None);
            }
        }
    }
}
