//! The built-in subcommands.
//!
//! A facade: each command lives in its own file, and registering one is a single line
//! in `crate::registry`.

pub mod build;
pub mod check;
pub mod new;
pub mod patch;
pub mod play;
pub mod run;
pub mod test;
pub mod ui;

pub use build::Build;
pub use check::Check;
pub use new::NewProject;
pub use patch::Patch;
pub use run::Run;
pub use test::Test;
