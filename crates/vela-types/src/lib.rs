//! Types, inference, and the checking rules.
//!
//! # Owns
//!
//! Ty, TypeCtx, inference, exhaustiveness and reachability analysis.
//!
//! # Does not own
//!
//! Name resolution (vela-hir); code generation (vela-mir).
//!
//! # One mistake, one diagnostic
//!
//! Checking never fails outright. An expression that cannot be typed is `Ty::Unknown`,
//! which fits everywhere and is never reported against. That is deliberate: an untypeable
//! expression should produce one diagnostic where it went wrong, not one for every place
//! its result is then used.

mod check;
mod env;
mod error;
mod lower;
mod ty;

#[cfg(test)]
mod tests;

pub use check::{check, type_of};
pub use env::{EnumShape, Env, Scope};
pub use lower::lower;
pub use ty::Ty;
