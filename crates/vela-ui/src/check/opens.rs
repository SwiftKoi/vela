//! The screen a call opens: a name the whole *game* declares (`SCREENS.md §2.1`).
//!
//! Both of them — `open_screen` and `replace_screen` — because a menu page is a screen like any other and
//! a mistyped one is the same silent button (`§7`).
//!
//! The one screen question that is not per file. A `use` names a screen in the file that writes it, so
//! the checker is given that file's declarations (`check/screen.rs`) — but an `open_screen` names a
//! screen the *project* has: in `the_question`, `navigation` opens `preferences`, `save`, `load`, and
//! `about`, each declared elsewhere in `screens.rpy`, and a migrated project is one file per Ren'Py file.
//! So this is asked once, with every file's declarations, by the layer that has them all (`vela-lsp`
//! answers both the editor and `vela check`), the way the save schema is derived by the layer that has
//! every file.
//!
//! Two codes, the same two `use` answers to (`compose.rs`), because it is the same two mistakes: `E5009`
//! for a target nothing declares, `E5010` for arguments that do not fit it. What that buys is the failure
//! this milestone keeps meeting: a button whose `open_screen` names a screen that does not exist is a
//! press that does nothing, and nothing else says so.

use vela_diag::Diagnostic;
use vela_span::Span;
use vela_syntax::{Expr, ScreenDecl, StrPart};

use crate::actions::{OPEN_SCREEN, REPLACE_SCREEN};

use super::actions::each_call;
use super::screen::diag;

/// Checks every screen a call opens in a project against the screens the game declares.
///
/// `files` is one slice of declarations per source file: the callers are walked per file so a diagnostic
/// carries the span it was written at, and the targets are the union, because a screen may be declared
/// in any of them.
#[must_use]
pub fn check_open_screens(files: &[&[&ScreenDecl]]) -> Vec<Diagnostic> {
    let mut game: Vec<&ScreenDecl> = files.iter().copied().flatten().copied().collect();
    // The interface is a target too (`SCREENS.md §2.7`): `open_screen("preferences")` names a screen a
    // project that declares none of its own still has, and leaving the interface out of this list reported
    // exactly that call — the failure this check exists to catch, reported on a project that was right.
    game.extend(crate::interface::decls().iter().copied());

    let mut out = Vec::new();
    for file in files {
        for screen in *file {
            each_call(&screen.body, &mut |name, args, span| {
                if name == OPEN_SCREEN || name == REPLACE_SCREEN {
                    check_open(&game, args, span, &mut out);
                }
            });
        }
    }
    out
}

/// One `open_screen`: the screen it names is one the game declares, and the arguments fit.
fn check_open(game: &[&ScreenDecl], args: &[Expr], span: Span, out: &mut Vec<Diagnostic>) {
    let Some((name, written)) = target(args.first()) else {
        return;
    };
    let Some(screen) = game.iter().find(|screen| screen.name == name) else {
        let mut diagnostic = diag(
            "E5009",
            format!("no screen called `{name}`"),
            written,
            "no screen by this name is declared in this project, or in the interface",
        );
        if let Some(nearest) =
            vela_diag::closest(name, game.iter().map(|screen| screen.name.as_str()))
        {
            diagnostic = diagnostic.with_help(format!("did you mean `{nearest}`?"));
        }
        out.push(diagnostic);
        return;
    };

    // The arguments *after* the target are the screen's, and only too many of them are this check's: a
    // parameter the call omits keeps its declared default, which is the calling convention (`§2.1`).
    let given = args.len().saturating_sub(1);
    if let Some(problem) = crate::compose::too_many(screen, given) {
        out.push(
            diag(
                "E5010",
                format!("`{name}` cannot be called this way"),
                span,
                problem,
            )
            .with_help("match the parameters the screen declares"),
        );
    }
}

/// The screen a target argument names, and where it is written, when it names one.
///
/// **A string is the spelling**, the way a setting's name is a string (`SCREENS.md §7.1`): a bare word
/// in this position is an *expression*, and the language's name resolution is what reports one that
/// resolves to nothing (`E2001`) — so reading it here too would be two diagnostics for one mistake, and
/// this check's would be the second. What a bare word *cannot* be is a call that resolves: the argument
/// is a target, the runtime takes the name, and a name the screen binds is a value the screen computed
/// (which is why `open_screen(which)` is nobody's to report).
fn target(first: Option<&Expr>) -> Option<(&str, Span)> {
    match first? {
        Expr::Str { parts, span } => match parts.as_slice() {
            [StrPart::Literal { text, .. }] => Some((text, *span)),
            _ => None,
        },
        _ => None,
    }
}
