//! Name resolution, scopes, the desugared AST, and the story graph.
//!
//! # Owns
//!
//! Hir, DefId, scopes, use/pub handling, StoryGraph construction.
//!
//! # Does not own
//!
//! Types (vela-types); lowering to IR (vela-mir).
//!
//! # Why module names live here
//!
//! A module name is a *semantic* identity, not a path — it is what `use` names and what
//! qualifies a label. Deriving one from a path is a convenience for the driver. Deciding
//! what names mean is this crate's job, and `vela-compile` sits above it, so the type has
//! to be defined below.
//!
//! # Two phases, for incrementality
//!
//! Collecting a module's definitions reads only that module's file. Resolving its
//! references reads the modules it names. Keeping those apart is what lets an edit
//! invalidate one module's symbols and the checks of the modules that *use* it, rather
//! than everything.

mod collect;
mod def;
mod error;
mod module;
mod names;
mod pragmas;
mod reach;
mod resolve;
mod story;

#[cfg(test)]
mod tests;

pub use collect::{Collected, Import, Module, collect};
pub use def::{DefKind, Definition};
pub use module::ModuleName;
pub use names::resolve_names;
pub use pragmas::format_pragmas;
pub use reach::{Entry, unreachable_labels};
pub use resolve::{Modules, resolve, target_of};
pub use story::{LabelNode, LabelRef, StoryGraph, Transfer};
