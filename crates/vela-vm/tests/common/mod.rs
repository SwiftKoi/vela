//! Shared test harness. A facade, so the logic lives beside it in `harness.rs`.

mod harness;

// Each test binary uses one of these, not both, and a re-export that is unused *in this binary*
// is not an unused import — it is the shared harness doing its job.
#[allow(unused_imports)]
pub use harness::{compile, compile_with};
