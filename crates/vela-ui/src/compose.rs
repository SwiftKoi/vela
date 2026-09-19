//! Screen composition: `use`, `transclude`, and the rules that keep them honest.
//!
//! `SCREENS.md §2`. One screen *uses* another, and may hand it a block that the used screen places
//! where it writes `transclude`. That is the one part of the screen language that is a relationship
//! between declarations rather than a property of one, and it is why the rules live here instead of
//! being spelled out again in the checker, the instantiator, the dependency walk, and the
//! accessibility walk — four copies of "which screen does this name mean" is four chances for them
//! to disagree about the same file.
//!
//! Three questions are answered once, here:
//!
//! * **Which screen a `use` names** — this file's own, by name. The same scope a `style` and a theme
//!   resolve in (`§5`), so there is no project-wide screen table and a `use` cannot reach another
//!   file.
//! * **Whether a screen places its caller's block**, which is recursive: the `transclude` may sit
//!   under an `if` or inside a widget, and the sample's `game_menu` places it once per branch of a
//!   three-way `if`.
//! * **Whether a call's arguments fit**, so the checker reports it and everyone else may rely on it.
//!
//! An absent parameter type is `Unknown` (`LANGUAGE.md §5.4`), so a screen parameter that was never
//! typed accepts any argument — which is what makes a migrated screen callable at all.

use vela_diag::{Code, Diagnostic};
use vela_span::Span;
use vela_syntax::{ScreenArg, ScreenDecl, ScreenLine};

use crate::eval::{self, Args};

/// Builds a diagnostic for a registered code.
///
/// # Panics
///
/// Panics if the code is not registered, which is a bug here rather than anything a user can
/// trigger: `check-diag-codes` rejects an unregistered code before it can be committed.
fn diag(
    code: &str,
    message: impl Into<String>,
    span: Span,
    label: impl Into<String>,
) -> Diagnostic {
    let code =
        Code::new(code).unwrap_or_else(|| panic!("`{code}` is not in crates/vela-diag/codes.txt"));
    Diagnostic::new(code, message, span, label)
}

/// The screen a `use` names, among this file's declarations.
#[must_use]
pub(crate) fn find<'a>(screens: &[&'a ScreenDecl], name: &str) -> Option<&'a ScreenDecl> {
    screens.iter().find(|screen| screen.name == name).copied()
}

/// Every screen name a body uses, directly.
///
/// Not a recursive walk of the *used* screens: this is the edges out of one body, which is what a
/// graph is built from.
pub(crate) fn uses_in(lines: &[ScreenLine], out: &mut Vec<String>) {
    for line in lines {
        match line {
            ScreenLine::Use { name, body, .. } => {
                out.push(name.clone());
                uses_in(body, out);
            }
            ScreenLine::If { .. } | ScreenLine::For { .. } => {
                // One arm is drawn at run time and no walk knows which, so every arm's `use` is a
                // real edge — and a loop's body is drawn once per element, so its `use` is one too.
                for body in line.bodies() {
                    uses_in(body, out);
                }
            }
            ScreenLine::Node(node) => uses_in(&node.children, out),
            // A binding names no screen and holds no block: it is an input answer, not a placement. A
            // variable names no screen either.
            ScreenLine::Key { .. }
            | ScreenLine::Timer { .. }
            | ScreenLine::Layer { .. }
            | ScreenLine::StylePrefix { .. }
            | ScreenLine::Default { .. }
            | ScreenLine::Transclude { .. } => {}
        }
    }
}

/// Whether a body places its caller's block somewhere.
///
/// Recursive, and it descends into a nested `use`'s block on purpose: that block is *this* screen's
/// code, so a `transclude` written in it places this screen's own caller's block. Presence anywhere
/// is what the question means — a block handed to a screen that never places it is content that goes
/// nowhere, which is `W4012`.
#[must_use]
pub(crate) fn transcludes(lines: &[ScreenLine]) -> bool {
    lines.iter().any(|line| match line {
        ScreenLine::Transclude { .. } => true,
        // A `transclude` in any arm of an `if`, or inside a loop, places the caller's block: which arm
        // draws and how many elements there are are runtime questions, and a block that reaches one of
        // them has somewhere to land.
        ScreenLine::If { .. } | ScreenLine::For { .. } => {
            line.bodies().iter().any(|body| transcludes(body))
        }
        ScreenLine::Use { body, .. }
        | ScreenLine::Node(vela_syntax::ScreenNode { children: body, .. }) => transcludes(body),
        ScreenLine::Layer { .. }
        | ScreenLine::StylePrefix { .. }
        | ScreenLine::Key { .. }
        | ScreenLine::Timer { .. }
        | ScreenLine::Default { .. } => false,
    })
}

/// Checks every `use` in one body.
///
/// Per body, because that is the unit the checker and the editor work in. The graph *between* screens
/// is a per-file question and is [`check_cycles`], asked once — the same division as styles, where
/// `check_screen_styles` is per screen and `check_inheritance` is per file.
pub(crate) fn check_uses(screens: &[&ScreenDecl], lines: &[ScreenLine], out: &mut Vec<Diagnostic>) {
    walk_uses(screens, lines, out);
}

/// Checks each `use` line, recursing into the blocks it passes on.
fn walk_uses(screens: &[&ScreenDecl], lines: &[ScreenLine], out: &mut Vec<Diagnostic>) {
    for line in lines {
        match line {
            ScreenLine::Use {
                span,
                name,
                args,
                body,
            } => {
                check_use(screens, *span, name, args, body, out);
                walk_uses(screens, body, out);
            }
            ScreenLine::If { .. } | ScreenLine::For { .. } => {
                for body in line.bodies() {
                    walk_uses(screens, body, out);
                }
            }
            ScreenLine::Node(node) => walk_uses(screens, &node.children, out),
            ScreenLine::Layer { .. }
            | ScreenLine::StylePrefix { .. }
            | ScreenLine::Key { .. }
            | ScreenLine::Timer { .. }
            | ScreenLine::Default { .. }
            | ScreenLine::Transclude { .. } => {}
        }
    }
}

/// One `use`: the screen exists, the arguments fit, and a block reaches a `transclude`.
fn check_use(
    screens: &[&ScreenDecl],
    span: Span,
    name: &str,
    args: &[ScreenArg],
    body: &[ScreenLine],
    out: &mut Vec<Diagnostic>,
) {
    let Some(callee) = find(screens, name) else {
        let mut diagnostic = diag(
            "E5009",
            format!("no screen called `{name}`"),
            span,
            "no screen by this name is declared in this file",
        );
        if let Some(nearest) = nearest(name, screens) {
            diagnostic = diagnostic.with_help(format!("did you mean `{nearest}`?"));
        }
        out.push(diagnostic);
        return;
    };

    if let Some(problem) = misfit(callee, args) {
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

    // A block with nowhere to land is content the author wrote that would never be drawn. Reported
    // rather than refused: a screen can legitimately be mid-edit, and a build that failed would
    // teach people to delete the block rather than write the `transclude`.
    if !body.is_empty() && !transcludes(&callee.body) {
        out.push(
            diag(
                "W4012",
                format!("`{name}` has no `transclude`"),
                span,
                "this block is never placed",
            )
            .with_help("either write `transclude` in the used screen, or call it without a block"),
        );
    }
}

/// How a call's arguments do not fit the screen's parameters, if they do not.
///
/// Positional arguments bind in order and named ones by name, which is the whole calling convention
/// (`§2`): there is no `*args`, no keyword-only parameter, and no default *expression* the callee can
/// be observed not having.
fn misfit(callee: &ScreenDecl, args: &[ScreenArg]) -> Option<String> {
    let positional = args
        .iter()
        .filter(|arg| matches!(arg, ScreenArg::Value(_)))
        .count();
    if positional > callee.params.len() {
        return Some(format!(
            "it takes {} parameter(s), but {positional} were given",
            callee.params.len()
        ));
    }
    let unknown = args.iter().find_map(|arg| match arg {
        ScreenArg::Named { name, .. } if !callee.params.iter().any(|param| param.name == *name) => {
            Some(name.clone())
        }
        _ => None,
    });
    unknown.map(|name| format!("it has no parameter called `{name}`"))
}

/// `E5011` — screens that use each other in a loop.
///
/// A cycle is not a slow screen, it is an unbounded one: drawing `a` would draw `b`, which would draw
/// `a` again. The dependency walk and the accessibility walk both bound themselves for the same
/// reason, but truncating a drawing is the wrong fix — the file is refused instead.
///
/// Asked once per file rather than once per screen, because the answer is about the *graph*: an
/// implementation that ran inside the per-screen check would report a two-screen cycle twice, and the
/// editor would show the same diagnostic on every screen in the loop.
///
/// Reported once per screen *in* the loop, as `E5008` is for styles: two declarations are at fault,
/// and fixing one without the other leaves the loop.
#[must_use]
pub fn check_cycles(screens: &[&ScreenDecl]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for screen in screens {
        let mut seen = Vec::new();
        if reaches(screens, &screen.name, &screen.name, &mut seen) {
            diagnostics.push(
                diag(
                    "E5011",
                    format!("`{}` uses itself", screen.name),
                    screen.span,
                    "this composition loops",
                )
                .with_help("break the loop by removing one `use`"),
            );
        }
    }
    diagnostics
}

/// Whether `root` is reachable from `current` by following `use`.
///
/// `seen` is the visited set: without it a diamond — `a` using `b` and `c`, both using `d` — would be
/// walked twice per level, and the cost would grow with the shape of the graph rather than its size.
fn reaches(screens: &[&ScreenDecl], root: &str, current: &str, seen: &mut Vec<String>) -> bool {
    let Some(screen) = find(screens, current) else {
        return false;
    };
    let mut used = Vec::new();
    uses_in(&screen.body, &mut used);

    for next in used {
        if next == root {
            return true;
        }
        if seen.contains(&next) {
            continue;
        }
        seen.push(next.clone());
        if reaches(screens, root, &next, seen) {
            return true;
        }
    }
    false
}

/// Fills in the parameters a call did not mention, each from its own default.
///
/// Shared by the two ways a screen is called — `use`, whose arguments are *syntax* bound in the
/// caller's scope, and an `open_screen` a runtime performs, whose arguments are already values
/// (`SCREENS.md §2.1`, §2.3) — because a default that meant one thing for one and something else for
/// the other would be two calling conventions in one language.
///
/// Defaults go last, and each is evaluated against what is already bound, so a default may name a
/// parameter that *was* passed rather than only a literal. A parameter with neither a value nor a
/// default is left unbound: the checker has already reported the mismatch, and a screen somebody is
/// midway through editing should draw something rather than nothing.
#[must_use]
pub(crate) fn with_defaults(callee: &ScreenDecl, mut bound: Args) -> Args {
    for param in &callee.params {
        if bound.get(&param.name).is_some() {
            continue;
        }
        let Some(default) = &param.default else {
            continue;
        };
        let value = eval::value_of(default, &bound);
        bound.set(param.name.clone(), value);
    }
    bound
}

/// The nearest declared screen name, if one is close enough to be worth offering.
fn nearest(name: &str, screens: &[&ScreenDecl]) -> Option<String> {
    screens
        .iter()
        .map(|screen| {
            (
                vela_diag::edit_distance(name, &screen.name),
                screen.name.clone(),
            )
        })
        .filter(|(distance, _)| *distance <= (name.chars().count() / 3).clamp(1, 3))
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, name)| name)
}
