//! The typed mid-level IR — the stable compilation contract — and its optimization passes.
//!
//! # Owns
//!
//! `Module`, `Body`, `Block`, `Stmt`, `Terminator`, the lowering from typed syntax, the
//! optimization pass registry, and a reference interpreter.
//!
//! # Does not own
//!
//! Bytecode encoding (`vela-bytecode`), execution of bytecode (`vela-vm`), and the
//! diagnostics that belong to checking (`vela-types`).
//!
//! # Why there is an interpreter here
//!
//! MIR is a *contract*, and a contract with no executable meaning is prose. The reference
//! interpreter is that meaning in runnable form, and it is the oracle the optimizer is
//! tested against: `BYTECODE.md §2.1` requires every pass to be observably
//! semantics-preserving, and the only way to check "observably" is to run both versions
//! and compare. It is deliberately a direct evaluator over MIR rather than a second
//! bytecode VM, so that a pass bug cannot hide behind a codegen bug that happens to
//! cancel it out.

mod error;
mod interp;
mod ir;
mod lower;
mod opt;
mod print;
#[cfg(test)]
mod tests;

pub use interp::{Answer, Execution, Outcome};
pub use ir::{
    AssetRefs, Block, BlockId, Body, Builtin, Callee, Const, ConstDef, ConstId, ConstPool,
    DefaultDef, DefaultId, EnumDef, FieldDef, FieldId, FuncRef, LabelRef, LocalDecl, Module,
    Operand, Place, Root, Slot, Stmt, StmtKind, StructDef, Symbol, Terminator, Value, VariantDef,
    VariantId, YieldSite,
};
pub use lower::lower;
pub use opt::{OptLevel, Pipeline, optimize, passes};
pub use print::{print_body, print_module};
