//! The debugger: a Debug Adapter Protocol server over the VM's step interface.
//!
//! # Owns
//!
//! The DAP protocol, breakpoints, the step and time-travel policy, variable and `World`
//! inspection, and expression evaluation for a paused frame.
//!
//! # Does not own
//!
//! The machine it drives ([`vela_vm`]); the snapshot ring it steps backwards on
//! ([`vela_replay`]); the checker it evaluates with ([`vela_types`]); the command line and the
//! project loader ([`vela_cli`]).
//!
//! # Shape
//!
//! ```text
//!   vela debug ──► Server::new(Program, loader)         the CLI builds the program
//!                      │
//!                      ├─ transport   Content-Length framing, the same shape as LSP's
//!                      ├─ dispatch    one DAP request in, one response out
//!                      ├─ debuggee    the Timeline, and where it should stop
//!                      └─ evaluate    the real checker, so a bad expression is a normal Exxx
//! ```
//!
//! A story is compiled once, by the command line, and handed over as a [`Program`]: the
//! bytecode, its entry label, and the sources its spans point into. Nothing here re-reads a
//! project, which is what keeps the debugger's view of a story the *same* view the build has.

mod breakpoints;
mod debuggee;
mod evaluate;
mod program;
mod server;
mod transport;

pub use breakpoints::Breakpoints;
pub use debuggee::{Debuggee, Mode, StopReason};
pub use evaluate::evaluate;
pub use program::{Position, Program};
pub use server::Server;
pub use transport::{read, write};
