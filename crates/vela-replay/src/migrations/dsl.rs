//! `migration!` — how a version step is written (`RUNTIME.md §6.1`).
//!
//! A migration file is a filename, a version pair, and a short list of what changed:
//!
//! ```rust,ignore
//! // crates/vela-replay/src/migrations/v0007_trust_to_affection.rs
//! migration! {
//!     from = 6,
//!     to = 7,
//!     rename_field = ("trust", "affection"),
//!     add_default = ("affection_cap", Value::Int(10)),
//!     transform = |w| { /* … */ },
//! }
//! ```
//!
//! The operations run in the order they are written, and each is optional. The macro
//! expands to a function named `migration`, which is what the chain registry
//! ([`super::registry`]) calls — that is why one version step is one file, and why adding
//! one is a new file plus a line in the registry rather than an edit to a growing `match`.

/// Declares the migration that brings a world from one save version to the next.
///
/// Expands to `pub fn migration() -> Migration`. The operations run in the order written;
/// `rename_field`, `add_default`, and `transform` may each appear any number of times.
///
/// ```rust,ignore
/// migration! {
///     from = 6,
///     to = 7,
///     rename_field = ("trust", "affection"),
///     add_default = ("affection_cap", Value::Int(10)),
///     transform = |w| { /* … */ },
/// }
/// ```
#[macro_export]
macro_rules! migration {
    (
        from = $from:expr,
        to = $to:expr
        $(, $($steps:tt)*)?
    ) => {
        $crate::migration!(@build $from, $to $(; $($steps)*)?);
    };

    (@build $from:expr, $to:expr $(; $($steps:tt)*)?) => {
        /// The migration this module declares.
        pub fn migration() -> $crate::Migration {
            #[allow(unused_mut)]
            let mut migration = $crate::Migration::new($from, $to);
            $($crate::migration!(@steps migration; $($steps)*);)?
            migration
        }
    };

    (@steps $target:ident;) => {};

    (@steps $target:ident; rename_field = ($old:literal, $new:literal) $(, $($rest:tt)*)?) => {
        $target = $target.rename_field($old, $new);
        $crate::migration!(@steps $target; $($($rest)*)?);
    };

    (@steps $target:ident; add_default = ($name:literal, $value:expr) $(, $($rest:tt)*)?) => {
        $target = $target.add_default($name, $value);
        $crate::migration!(@steps $target; $($($rest)*)?);
    };

    (@steps $target:ident; transform = $rewrite:expr $(, $($rest:tt)*)?) => {
        $target = $target.transform($rewrite);
        $crate::migration!(@steps $target; $($($rest)*)?);
    };
}
