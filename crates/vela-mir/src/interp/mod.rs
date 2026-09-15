//! A reference interpreter over MIR.
//!
//! This is the *meaning* of MIR in runnable form. `BYTECODE.md §2.1` requires every
//! optimization pass to be observably semantics-preserving, and the only way to check
//! "observably" is to run both versions and compare. The oracle is this: a direct evaluator
//! over MIR, deliberately *not* a second bytecode VM, so that a pass bug cannot hide behind
//! a codegen bug that happens to cancel it out.
//!
//! It is not the engine. `vela-vm` (M5) is the engine: it runs opaque bytecode, schedules
//! effects, records an input log, and can be snapshotted. This runs the compiler's own IR in
//! the most obvious way that could possibly work, which is exactly what makes it a useful
//! thing to disagree with.

mod data;
mod exec;
mod machine;
mod ops;
mod steps;

pub use machine::{Answer, Execution, Outcome};
