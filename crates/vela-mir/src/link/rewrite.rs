//! Rewriting one unit's MIR into the linked module's names and tables.
//!
//! Every function here is total and infallible: a qualifier that does not resolve is refused before
//! any rewriting starts (`program::check`), so a rewrite never has to fail halfway and leave a
//! half-linked module behind.

use vela_types::Ty;

use crate::ir::{
    Block, Body, Callee, Const, FuncRef, LocalDecl, Operand, Place, Stmt, StmtKind, Symbol,
    Terminator, Value, YieldSite,
};

use super::names::Names;

/// One body, under its module's prefix.
pub(super) fn body(body: &Body, names: &Names<'_>) -> Body {
    Body {
        name: Symbol(names.qualified(body.name.as_str())),
        params: body.params.clone(),
        ret: ty(&body.ret, names),
        locals: body
            .locals
            .iter()
            .map(|local| LocalDecl {
                name: local.name.clone(),
                ty: ty(&local.ty, names),
            })
            .collect(),
        blocks: body.blocks.iter().map(|held| block(held, names)).collect(),
        entry: body.entry,
    }
}

/// One block. Block ids are per-body, so they are copied.
fn block(block: &Block, names: &Names<'_>) -> Block {
    Block {
        id: block.id,
        stmts: block
            .stmts
            .iter()
            .map(|stmt| statement(stmt, names))
            .collect(),
        term: terminator(&block.term, names),
    }
}

fn statement(stmt: &Stmt, names: &Names<'_>) -> Stmt {
    Stmt {
        kind: kind(&stmt.kind, names),
        span: stmt.span,
    }
}

fn kind(kind: &StmtKind, names: &Names<'_>) -> StmtKind {
    match kind {
        StmtKind::Assign { dst, op, a, b } => StmtKind::Assign {
            dst: place(dst, names),
            op: *op,
            a: value(*a, names),
            b: value(*b, names),
        },
        StmtKind::AssignUn { dst, op, a } => StmtKind::AssignUn {
            dst: place(dst, names),
            op: *op,
            a: value(*a, names),
        },
        StmtKind::Load { dst, src } => StmtKind::Load {
            dst: place(dst, names),
            src: operand(src, names),
        },
        StmtKind::Call { dst, callee, args } => StmtKind::Call {
            dst: dst.as_ref().map(|dst| place(dst, names)),
            callee: callee_of(callee, names),
            args: values(args, names),
        },
        StmtKind::Cmd { kind, args } => StmtKind::Cmd {
            kind: *kind,
            args: values(args, names),
        },
        StmtKind::ListNew { dst, items } => StmtKind::ListNew {
            dst: place(dst, names),
            items: values(items, names),
        },
        StmtKind::MapNew { dst, entries } => StmtKind::MapNew {
            dst: place(dst, names),
            entries: entries
                .iter()
                .map(|(key, entry)| (value(*key, names), value(*entry, names)))
                .collect(),
        },
        StmtKind::StructNew { dst, name, fields } => StmtKind::StructNew {
            dst: place(dst, names),
            name: names.qualified(name),
            fields: fields
                .iter()
                .map(|(field, entry)| (field.clone(), value(*entry, names)))
                .collect(),
        },
        StmtKind::EnumNew {
            dst,
            enum_name,
            variant,
            args,
        } => StmtKind::EnumNew {
            dst: place(dst, names),
            enum_name: names.qualified(enum_name),
            variant: variant.clone(),
            args: values(args, names),
        },
        StmtKind::EnumField { dst, base, index } => StmtKind::EnumField {
            dst: place(dst, names),
            base: value(*base, names),
            index: *index,
        },
        StmtKind::IsNone { dst, base } => StmtKind::IsNone {
            dst: place(dst, names),
            base: value(*base, names),
        },
        StmtKind::Unwrap { dst, base } => StmtKind::Unwrap {
            dst: place(dst, names),
            base: value(*base, names),
        },
    }
}

fn values(values: &[Value], names: &Names<'_>) -> Vec<Value> {
    values.iter().map(|value_| value(*value_, names)).collect()
}

fn callee_of(callee: &Callee, names: &Names<'_>) -> Callee {
    match callee {
        Callee::Direct(FuncRef::Defined(index)) => {
            Callee::Direct(FuncRef::Defined(names.function(*index)))
        }
        Callee::Direct(FuncRef::Effect(index)) => {
            Callee::Direct(FuncRef::Effect(names.effect(*index)))
        }
        Callee::Direct(FuncRef::Builtin(builtin)) => Callee::Direct(FuncRef::Builtin(*builtin)),
        Callee::Indirect(held) => Callee::Indirect(value(*held, names)),
    }
}

/// A value: only a constant pool id moves.
pub(super) fn value(value: Value, names: &Names<'_>) -> Value {
    match value {
        Value::Slot(slot) => Value::Slot(slot),
        Value::Const(id) => Value::Const(names.constant(id)),
    }
}

fn place(at: &Place, names: &Names<'_>) -> Place {
    match at {
        Place::Local(slot) => Place::Local(*slot),
        Place::Default(id) => Place::Default(names.default_id(*id)),
        Place::Field { base, field } => Place::Field {
            base: Box::new(place(base, names)),
            field: field.clone(),
        },
        Place::Index { base, index } => Place::Index {
            base: Box::new(place(base, names)),
            index: value(*index, names),
        },
    }
}

fn operand(operand: &Operand, names: &Names<'_>) -> Operand {
    match operand {
        Operand::Value(held) => Operand::Value(value(*held, names)),
        Operand::Default(id) => Operand::Default(names.default_id(*id)),
        Operand::Field { base, field } => Operand::Field {
            base: value(*base, names),
            field: field.clone(),
        },
        Operand::Index { base, index } => Operand::Index {
            base: value(*base, names),
            index: value(*index, names),
        },
        Operand::Len { base } => Operand::Len {
            base: value(*base, names),
        },
    }
}

fn terminator(term: &Terminator, names: &Names<'_>) -> Terminator {
    match term {
        Terminator::Goto(target) => Terminator::Goto(*target),
        Terminator::Branch { cond, then_, else_ } => Terminator::Branch {
            cond: value(*cond, names),
            then_: *then_,
            else_: *else_,
        },
        Terminator::Return(held) => Terminator::Return(held.map(|held| value(held, names))),
        Terminator::JumpLabel(target) => Terminator::JumpLabel(names.label(target)),
        Terminator::CallLabel { target, ret } => Terminator::CallLabel {
            target: names.label(target),
            ret: *ret,
        },
        Terminator::Dispatch {
            enum_name,
            value: held,
            arms,
            else_,
        } => Terminator::Dispatch {
            enum_name: names.qualified(enum_name),
            value: value(*held, names),
            arms: arms.clone(),
            else_: *else_,
        },
        Terminator::Yield(site) => Terminator::Yield(YieldSite {
            command: site.command,
            resume: site.resume,
            result: site.result,
        }),
        Terminator::Unreachable => Terminator::Unreachable,
    }
}

/// A type: a declared struct or enum takes its module's prefix, like every other name.
///
/// A type name written from another module lowers to `Ty::Unknown` (`M02`'s known limitation), so
/// every `Struct` and `Enum` here belongs to the unit being rewritten.
pub(super) fn ty(declared: &Ty, names: &Names<'_>) -> Ty {
    match declared {
        Ty::Optional(inner) => Ty::Optional(Box::new(ty(inner, names))),
        Ty::List(element) => Ty::List(Box::new(ty(element, names))),
        Ty::Map(key, entry) => Ty::Map(Box::new(ty(key, names)), Box::new(ty(entry, names))),
        Ty::Struct(name) => Ty::Struct(names.qualified(name)),
        Ty::Enum(name) => Ty::Enum(names.qualified(name)),
        Ty::Fn(params, ret) => Ty::Fn(
            params.iter().map(|param| ty(param, names)).collect(),
            Box::new(ty(ret, names)),
        ),
        Ty::Int | Ty::Float | Ty::Bool | Ty::Str | Ty::Unit | Ty::None | Ty::Unknown => {
            declared.clone()
        }
    }
}

/// A pooled constant: the names inside it are the unit's, so they take its prefix.
pub(super) fn constant(constant: &Const, names: &Names<'_>) -> Const {
    match constant {
        Const::Variant { enum_name, variant } => Const::Variant {
            enum_name: names.qualified(enum_name),
            variant: variant.clone(),
        },
        Const::Function(name) => Const::Function(names.qualified(name)),
        other => other.clone(),
    }
}
