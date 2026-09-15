//! Constant folding.
//!
//! Evaluates arithmetic whose operands are already constants. The visible win is string
//! interpolation of constants — `"Score: {MAX}"` collapsing to one string rather than a
//! conversion and a concatenation — which is what makes the disassembly of a story read
//! like the story.

use crate::ir::{Const, ConstPool, Module, Operand, Place, StmtKind, Value};
use crate::lower::decl::{fold_binary, fold_unary};
use crate::opt::{OptLevel, Pass};

/// Evaluates constant expressions.
pub struct ConstFold;

impl Pass for ConstFold {
    fn name(&self) -> &'static str {
        "const_fold"
    }

    fn level(&self) -> OptLevel {
        OptLevel::O1
    }

    fn run(&self, module: &mut Module) {
        // The bodies and the pool are disjoint fields, so both can be borrowed at once.
        let Module {
            fns, labels, pool, ..
        } = module;

        for body in fns.iter_mut().chain(labels.iter_mut()) {
            for block in &mut body.blocks {
                for stmt in &mut block.stmts {
                    let Some((dst, value)) = foldable(&stmt.kind, pool) else {
                        continue;
                    };
                    let id = pool.add(value);
                    stmt.kind = StmtKind::Load {
                        dst,
                        src: Operand::Value(Value::Const(id)),
                    };
                }
            }
        }
    }
}

/// The constant a statement computes, if it computes one.
fn foldable(kind: &StmtKind, pool: &ConstPool) -> Option<(Place, Const)> {
    match kind {
        StmtKind::Assign { dst, op, a, b } => {
            let left = constant(*a, pool)?;
            let right = constant(*b, pool)?;
            Some((dst.clone(), fold_binary(*op, left, right)?))
        }
        StmtKind::AssignUn { dst, op, a } => {
            let operand = constant(*a, pool)?;
            Some((dst.clone(), fold_unary(*op, operand)?))
        }
        _ => None,
    }
}

/// The constant a value already is.
fn constant(value: Value, pool: &ConstPool) -> Option<Const> {
    match value {
        Value::Const(id) => pool.get(id).cloned(),
        Value::Slot(_) => None,
    }
}
