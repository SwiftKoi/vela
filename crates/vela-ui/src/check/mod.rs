//! Checking a widget tree against the registry.
//!
//! This is where the parser's deliberate ambiguity gets resolved. A screen line is a name
//! followed by words, and only the registry knows whether that name is a widget, a prop of the
//! widget above it, or neither:
//!
//! ```vela
//! box at bottom:      // `box` is a widget; `at` is a prop of it
//!     pad 24          // `pad` is not a widget, and `box` takes it — so it is a prop line
//!     column gap 8:   // `column` is a widget, and `gap` is a prop of it
//! ```
//!
//! Which is why `E5005` ("no such widget") and `E5006` ("a widget does not accept this prop")
//! exist as *checker* diagnostics and could not have been parse errors.
//!
//! The checker lives here rather than in `vela-compile` for a rank reason that is also a
//! design reason: compiling is rank 7 and the widget vocabulary is rank 8, so the only crates
//! that can see both the tree and the registry are the ones that *consume* screens — the CLI
//! and the language server.
//!
//! Split by what each rule is about, since a screen's come in five kinds that share nothing but the
//! entry point: [`widgets`] is what a line *is*, [`actions`] what a call *asks for*, [`keys`] what
//! input a presentation *answers*, [`variables`] what a screen *owns* (`§2.5`), and [`screen`] the
//! question that asks all of them.

mod actions;
mod conditions;
mod keys;
mod screen;
mod variables;
mod walk;
mod widgets;

pub use screen::check_screen;
pub(crate) use screen::diag;
