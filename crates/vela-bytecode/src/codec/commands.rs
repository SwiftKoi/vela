//! Command schemas against what a runtime knows.
//!
//! `BYTECODE.md §3.3`: a command is a *schema registration*, not an instruction. The module
//! carries the variants it uses, so a runtime that does not know one can reject the module
//! with `E7102` instead of executing something that means the wrong thing.
//!
//! This is the mechanism behind "adding a command variant requires no change to the
//! interpreter": the set of known variants is a table, and both sides — the compiler that
//! writes the module and the runtime that reads it — consult the same one.

use vela_diag::Diagnostic;
use vela_span::{FileId, Span};
use vela_world::CommandKind;

use crate::error;
use crate::module::Module;

/// Checks that every command a module uses is one this build knows.
///
/// Returns one diagnostic per unknown variant, because a module built against a different
/// set of schemas usually differs in more than one place and knowing all of them is what
/// makes the fix obvious.
#[must_use]
pub fn known_commands(module: &Module) -> Vec<Diagnostic> {
    let known: Vec<&str> = CommandKind::all()
        .iter()
        .map(|kind| kind.as_str())
        .collect();

    let mut diagnostics = Vec::new();
    for schema in &module.cmds {
        let name = module.strings.get(schema.name).unwrap_or_default();
        if !known.contains(&name) {
            diagnostics.push(error::unknown_command(
                name,
                Span::new(FileId::from_raw(0), 0, 0),
            ));
        }
    }
    diagnostics
}
