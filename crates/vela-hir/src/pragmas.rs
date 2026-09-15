//! `W4011` — a formatting pragma is in the file.
//!
//! `# fmt: off` tells the formatter to leave a region as written, which is occasionally the right
//! answer and always worth seeing: a region the formatter cannot reach is a region no future edit
//! will tidy, and the reason for it is in somebody's head. `TOOLING.md §3` asks for the pragma to be
//! *linted* so that its use is visible in review rather than discovered by surprise.
//!
//! Both halves are reported, not only `off`. A `# fmt: on` with no `# fmt: off` before it does
//! nothing at all, and a warning is the cheapest way for its author to find that out.

use vela_diag::Diagnostic;
use vela_syntax::Program;

use crate::error;

/// One diagnostic per formatting pragma in a file.
#[must_use]
pub fn format_pragmas(tree: &Program) -> Vec<Diagnostic> {
    tree.comments
        .iter()
        .filter_map(|comment| {
            let pragma = comment.pragma()?;
            Some(error::format_pragma(pragma, comment.span))
        })
        .collect()
}
