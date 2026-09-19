//! Staged images, checked against every module's declarations (`LANGUAGE.md §6.1`).
//!
//! A whole-program question, and it sits beside `assets::missing_assets` for the same reason: the
//! answer is not in any one file. The picture table is one table for a project — `vela build` bakes a
//! `scene`'s name into a texture, and a bundle ships no modules at all — so `scene bg.room` may name
//! an image another file declares, and only the driver sees every file at once.
//!
//! It was written as an arm in `vela-hir`'s per-module name resolver first, next to `E5001` and
//! `E5003` where its siblings live, and it reported `vela migrate`'s own output: the migrated story
//! stages `bg.lecturehall`, declared in `src/images.vela`. The two layers disagreed — a reference
//! resolved per module, a picture resolved per project — and *the runtime's reading is the one the
//! language means*: §6.1 says there is no module left at run time. So the check moved here, and
//! nothing about the modules changed.

use std::collections::BTreeSet;

use vela_diag::Diagnostic;

use crate::session::Session;

/// Every staged image whose name nothing in the project declares, in source order.
pub(crate) fn undeclared(files: &[vela_span::FileId], session: &mut Session) -> Vec<Diagnostic> {
    let mut declared: BTreeSet<String> = BTreeSet::new();
    for file in files {
        declared.extend(vela_hir::declared(&session.parse(*file).program));
    }

    let mut out = Vec::new();
    for file in files {
        let parsed = session.parse(*file);
        for staged in vela_hir::staged(&parsed.program) {
            if !resolves(&staged.name, &declared) {
                out.push(vela_hir::undefined_image(&staged.name, staged.span));
            }
        }
    }
    out.sort_by_key(|diagnostic| {
        (
            diagnostic.primary.span.file().as_raw(),
            diagnostic.primary.span.start(),
        )
    });
    out
}

/// Whether a staged name is declared, or is the *tag* of something that is.
///
/// A tag resolves because that is what a bare name means: `hide sylvie` names a sprite group rather
/// than a variant — Ren'Py's own rule for a tag, and the rule the runtime follows when it keeps the
/// face a `show sylvie` did not change.
fn resolves(name: &str, declared: &BTreeSet<String>) -> bool {
    declared.contains(name)
        || declared
            .iter()
            .any(|candidate| candidate.starts_with(&format!("{name}.")))
}
