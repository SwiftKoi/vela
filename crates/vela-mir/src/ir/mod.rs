//! The mid-level IR.
//!
//! A control-flow graph of basic blocks over typed slots, and the stable contract between
//! the front end and every back end (`BYTECODE.md §2`).
//!
//! # Where this deviates from `BYTECODE.md §2`, and why
//!
//! The spec is a draft written before MIR existed. Building it surfaced gaps that each have
//! a test behind them; they are recorded in `docs/roadmap/M03-mir.md` and applied to the
//! spec rather than left as a silent divergence:
//!
//! 1. **Statements carry spans** rather than a `Body::spans: SpanTable` indexed by an id
//!    only lowering knows. A table is genuinely needed at the *bytecode* layer, where it is
//!    indexed by instruction offset and read by a debugger; at MIR it was indirection with
//!    no consumer.
//! 2. **`Module` has a constant pool.** `Const(ConstId)` in the spec has no pool to index.
//! 3. **`Callee` may be indirect.** The spec's `Call { func: FuncRef }` cannot express
//!    calling a lambda that was stored in a variable, which the grammar allows.
//! 4. **`Dispatch` names its enum.** Rule 6 of `BYTECODE.md §4` requires a table to cover
//!    "the enum's declared variant range" — which nobody can check without knowing which
//!    enum.
//! 5. **Aggregates can be constructed.** The spec's statement set had no way to build a
//!    list, map, struct, or variant, so a program could not create any of them.
//! 6. **Optionals have operations.** `??` is in the language; `IsNone` and `Unwrap` did not
//!    exist, so a coalesce had no lowering.
//! 7. **There is a length operand**, which is what a `for` loop is lowered to.
//! 8. **A `default` is readable.** It is a place, not a value, so reading one needs its own
//!    operand.
//!
//! The spec's own rule — "change to MIR is internal, but all golden disassembly files are
//! re-blessed in one commit" — is why these are spec edits rather than silent drift.

mod body;
mod defs;
mod module;
mod stmt;
mod term;

pub use body::{Block, BlockId, Body, Symbol};
pub use defs::{
    AssetRefs, Const, ConstDef, ConstId, ConstPool, DefaultDef, DefaultId, EffectDef, EnumDef,
    FieldDef, FieldId, LocalDecl, Slot, StructDef, VariantDef, VariantId,
};
pub use module::Module;
pub use stmt::{Builtin, Callee, FuncRef, Operand, Place, Root, Stmt, StmtKind, Value};
pub use term::{LabelRef, Terminator, YieldSite};
