//! The server: a loop, a session, and the answers the lower layers already know how to give.
//!
//! # What is served, and what is deliberately not
//!
//! Diagnostics, document sync, goto-definition, and references — `publishDiagnostics` is what the
//! parity criterion in `M10-tooling.md` is about, and the other two read the symbol index. Hover,
//! completion, and rename are *not* declared: `initialize` advertises a capability only once it
//! answers, because an editor told a capability exists will call it, and a capability that answers
//! nothing is worse than one that is absent — the editor stops looking, and the feature reads as broken
//! rather than as missing.
//!
//! # Why the session is handed over rather than built here
//!
//! A session is a project's sources, its entry point, and its asset manifest, and two of those come
//! from a `vela.toml` this crate cannot read — the protocol knows nothing about projects, and the command
//! line already knows everything. So the caller passes a loader, and this crate stays what it claims to
//! be: an adapter that decides nothing about what is wrong with a program.
//!
//! # Shape
//!
//! One file per responsibility, because the whole server in one `impl` block outgrew the file budget —
//! and splitting it by what each part is for was the honest reading of that: the struct and its
//! lifecycle, the dispatch of a message, the documents and their diagnostics, the two index queries, and
//! the shape of a reply.

mod dispatch;
mod documents;
mod locations;
mod reply;
mod state;

pub use state::Server;
