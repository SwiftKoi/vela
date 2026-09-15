//! The diagnostic model: stable codes, spans, severities, and rendering.
//!
//! # Owns
//!
//! The code registry, Diagnostic/Label/Suggestion types, the human renderer.
//!
//! # Does not own
//!
//! Deciding when to emit a diagnostic; each phase owns its own checks.
//!
//! # Why codes are a registry
//!
//! Every diagnostic carries a stable code (`E5003`, `W4002`) that is never reused or
//! renumbered, so a code in a CI log from a year ago still means the same thing. The
//! registry of codes lives in `codes.txt` — a plain file, so the `check-diag-codes`
//! xtask check can validate it without linking this crate.

mod code;
mod diagnostic;
mod render;
mod suggest;

#[cfg(test)]
mod tests;

pub use code::{Code, Severity};
pub use diagnostic::{Diagnostic, Label, Suggestion};
pub use render::render;

pub use suggest::{closest, distance as edit_distance};
