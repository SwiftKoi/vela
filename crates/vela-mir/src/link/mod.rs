//! Linking a program's modules into one (`LANGUAGE.md §6`, `RUNTIME.md §8`).
//!
//! A story is written as several files and runs as one program. `vela-hir` resolves a qualified
//! reference to a module and a label; `vela-mir` records that reference *unresolved* — a
//! [`LabelRef`] naming a module that is not the one it was written in — because lowering one
//! module cannot see the module it names. This is where the whole program is put together.
//!
//! Every label becomes `module.label`, every function, struct, and enum takes its module's prefix,
//! and the constant pool, `default` table, and effect table are merged. What comes out is one
//! [`Module`], which is what the VM runs, what `vela build` writes, and what the reference
//! interpreter executes.
//!
//! # Why one image
//!
//! `vela-mir`'s own comment on [`LabelRef`] states the design: resolving a reference to a module
//! and a name *"is what keeps the runtime free of any notion of modules or imports"*. So the runtime
//! is handed one module whose label table already contains every label of the program, and a
//! cross-module `jump` is an ordinary call to a label with a qualified name.
//!
//! The alternative — a module table in the VM, resolved as modules load — is what a *streaming*
//! target wants, and it would put module identity into the container and the runtime. That is a
//! format decision with its own costs, so it is not taken here.
//!
//! # Linking is not compiling
//!
//! The MIR merged here is already checked, and the merge is a rename plus a table union: no
//! parsing, no name resolution, no type checking. A bundle therefore still loads without
//! compiling anything (`BUILD_AND_ASSETS.md §1`, `RUNTIME.md §8`) — the image it loads was linked
//! when it was built.
//!
//! # What it costs
//!
//! One image means a patch that touches any module replaces the whole script file, where the old
//! per-module layout replaced only the changed one. `VISION.md §5`'s acceptance bar — a text-only
//! change under 5% of the bundle — still holds for the story sizes the fixture covers, because
//! assets dominate a bundle, but a large story loses patch granularity. Sub-file chunks
//! (`BUILD_AND_ASSETS.md §6.2`) are the answer at this granularity, and the container has room for
//! it.
//!
//! [`LabelRef`]: crate::ir::LabelRef
//! [`Module`]: crate::ir::Module

mod error;
mod names;
mod program;
mod rewrite;
#[cfg(test)]
mod tests;

pub use error::LinkError;
pub use program::{Unit, link};
