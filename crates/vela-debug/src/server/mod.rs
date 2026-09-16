//! The server: a loop, a program, and the answers the machine and the checker already know how to
//! give.
//!
//! # What is served, and what is refused
//!
//! `TOOLING.md §6` lists the debugger's surface, and this adapter implements that list and no
//! more: breakpoints by line and by label, stepping over/into/out and backwards, the stack and
//! its variables, `World`, and expression checking. Everything else — `reverseContinue`,
//! conditional breakpoints, `setVariable` — is *refused explicitly* rather than half-implemented,
//! which is the mitigation `M11-debugger.md` records for DAP being a large surface.
//!
//! # Shape
//!
//! One file per responsibility: the struct and its lifecycle, the dispatch of a request, the
//! requests that move the story, the questions about a stopped one, and the shape of a reply.

mod dispatch;
mod inspect;
mod reply;
mod requests;
mod state;

pub use state::Server;
