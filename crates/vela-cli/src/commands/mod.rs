//! The built-in subcommands.
//!
//! A facade: each command lives in its own file, and registering one is a single line
//! in `crate::registry`.

pub mod analyze;
pub mod build;
pub mod bundle;
pub mod bundle_run;
pub mod check;
pub mod doc;
pub mod format;
pub mod frame;
pub mod lsp;
pub mod new;
pub mod patch;
pub mod play;
pub mod resolve;
pub mod run;
pub mod target;
pub mod test;
pub mod ui;

pub use analyze::Analyze;
pub use build::Build;
pub use check::Check;
pub use doc::Doc;
pub use format::Format;
pub use lsp::Lsp;
pub use new::NewProject;
pub use patch::Patch;
pub use run::Run;
pub use test::Test;
