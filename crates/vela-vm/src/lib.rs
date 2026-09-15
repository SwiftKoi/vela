//! The bytecode interpreter: a deterministic state machine that yields presentation commands.
//!
//! # Owns
//!
//! Vm, Frame, the step loop, the effect and command registries.
//!
//! # Does not own
//!
//! Rendering (vela-render); save files and migrations (vela-replay); state (vela-world).
//!
//! A *snapshot* of the machine is [`VmState`] and lives here — it is the machine's own state,
//! copied. What turns that into a file on disk is `vela-replay`'s.
//!
//! # The command boundary
//!
//! The VM produces `Command` values and consumes answers. It never calls a renderer, which
//! is decision D7 in `ARCHITECTURE.md §7` and the reason headless testing, replay, and
//! save/restore are the same mechanism rather than three subsystems.
//!
//! # Work in progress
//!
//! The step loop and the data instructions are here. The effect registry, the input log,
//! replay, and `vela run --headless` are not written yet.

mod access;
mod aggregate;
mod command_schema;
mod driver;
mod exec;
mod fault;
#[cfg(not(target_arch = "wasm32"))]
mod load;
mod machine;
mod ops;
mod session;
mod state;

pub use driver::{Execution, Host, Scripted, TakeFirst, replay, run};
pub use fault::Fault;
#[cfg(not(target_arch = "wasm32"))]
pub use load::LoadError;
pub use machine::{Step, Vm};
pub use session::{Session, Snapshot};
pub use state::{FrameState, Resume, VmState};
