//! The headless story runner: scripted input, assertions, and coverage.
//!
//! # Owns
//!
//! Reading `test` items, compiling their expressions into the program, driving the VM through a script,
//! and reporting what failed.
//!
//! # Does not own
//!
//! The VM itself; vela-test drives the real one.
//!
//! # What is here, and what is not
//!
//! Running a `test` item (`TOOLING.md §5`): scripted input, world assertions, story-graph coverage of
//! the labels a run reaches, and the interface a run clicks (`stage`). A test's expressions are
//! compiled as functions and read out of the live story (`plan`), and the run is a step loop over
//! `vela-vm`'s `Session` rather than `driver::run`, because a test answers a bounded number of
//! commands and then asserts instead of playing to the end (`run`).
//!
//! Not here yet, and named rather than quietly absent: **golden frames** (rendering at a scripted point
//! and comparing to a stored image, which is `vela-render`'s to produce), the **locale sweep**, and
//! `cover variants` — which needs the machine to record which enum variants a `match` chose, which
//! nothing does today. A directive this crate cannot honour is reported as a note on the outcome rather
//! than skipped, so a test never checks less than it says it does.

mod plan;
mod report;
mod run;
mod stage;
#[cfg(test)]
mod tests;

pub use plan::{Plan, Prepared, Step, StepKind, prepare};
pub use report::{Failure, Outcome, Reason, Report};
pub use run::run;
pub use stage::{FRAME, Stage};
