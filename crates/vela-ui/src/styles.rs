//! Styles, their inheritance, and the checks that keep them typed.
//!
//! `SCREENS.md §5`: *"styles are typed, cascading, and materialized into tokens — no
//! stringly-typed style soup."* A `style` may inherit through `from`, and the checks that
//! keeps honest are two:
//!
//! * **`E5007`** — a `style <name>` prop, or a `from`, naming a style that does not exist.
//! * **`E5008`** — inheritance that loops back on itself. Without this a style chain is a
//!   hang rather than an error: resolution would recurse until the stack ends, and the
//!   diagnostic would be a crash in whichever tool happened to resolve it first.
//!
//! Resolution returns a *chain* — the base first, then the derived — rather than a merged map.
//! Merging here would have to guess at types it does not have yet, and the order is the useful
//! part anyway: a caller that wants the value of a key takes the last one that sets it.

use vela_diag::{Code, Diagnostic};
use vela_span::Span;
use vela_syntax::{ScreenArg, ScreenLine, StyleDecl};

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

/// The style names declared, for a suggestion and for a lookup.
#[must_use]
pub fn names<'a>(styles: &[&'a StyleDecl]) -> Vec<&'a str> {
    styles.iter().map(|style| style.name.as_str()).collect()
}

/// Whether a style is declared.
#[must_use]
pub fn resolve<'a>(styles: &[&'a StyleDecl], name: &str) -> Option<&'a StyleDecl> {
    styles.iter().copied().find(|style| style.name == name)
}

/// Checks inheritance across every `style` declaration.
///
/// Cycles are reported *once per style in the loop* rather than once per declaration, because
/// a two-style cycle has two declarations at fault and fixing one without the other leaves the
/// loop.
#[must_use]
pub fn check_inheritance(styles: &[&StyleDecl]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for style in styles {
        let Some(base) = &style.from else {
            continue;
        };

        // A style inheriting from itself is the shortest cycle, and saying so directly beats
        // reporting "cycle" for `style a from a`.
        if *base == style.name {
            diagnostics.push(
                diag(
                    "E5008",
                    format!("`{}` inherits from itself", style.name),
                    style.span,
                    "this style is its own base",
                )
                .with_help("remove the `from`, or name a different style"),
            );
            continue;
        }

        if resolve(styles, base).is_none() {
            let mut diagnostic = diag(
                "E5007",
                format!("no style called `{base}`"),
                style.span,
                "no style by this name is declared",
            );
            if let Some(nearest) = nearest(base, &names(styles)) {
                diagnostic = diagnostic.with_help(format!("did you mean `{nearest}`?"));
            }
            diagnostics.push(diagnostic);
            continue;
        }

        if let Some(cycle) = cycle_through(styles, &style.name) {
            diagnostics.push(
                diag(
                    "E5008",
                    format!(
                        "`{}` inherits from itself through {}",
                        style.name,
                        show(&cycle)
                    ),
                    style.span,
                    "this inheritance loops",
                )
                .with_help("break the loop by removing one `from`"),
            );
        }
    }

    diagnostics
}

/// Follows `from` from `name` and returns the chain if it comes back round.
fn cycle_through(styles: &[&StyleDecl], name: &str) -> Option<Vec<String>> {
    let mut chain = vec![name.to_string()];
    let mut current = name.to_string();

    // A style has one base, so the chain is a list and the walk is linear. The bound is the
    // declaration count: a walk longer than that has visited something twice, which *is* the
    // cycle, and it ends the loop even if the bookkeeping above is wrong.
    for _ in 0..styles.len() {
        let base = resolve(styles, &current)?.from.clone()?;
        if base == name {
            chain.push(base);
            return Some(chain);
        }
        chain.push(base.clone());
        current = base;
    }
    None
}

/// Checks a screen's `style` props against the declared styles.
#[must_use]
pub fn check_screen_styles(lines: &[ScreenLine], styles: &[&StyleDecl]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let known = names(styles);
    check_lines(lines, &known, &mut diagnostics);
    diagnostics
}

/// Walks the body looking for `style` props.
fn check_lines(lines: &[ScreenLine], known: &[&str], out: &mut Vec<Diagnostic>) {
    for line in lines {
        match line {
            ScreenLine::Layer { .. }
            | ScreenLine::StylePrefix { .. }
            | ScreenLine::Key { .. }
            | ScreenLine::Timer { .. }
            | ScreenLine::Transclude { .. } => {}
            ScreenLine::If { .. } => {
                for arm in line.arms() {
                    check_lines(arm, known, out);
                }
            }
            // A `use` block is written here, so its styles are this file's — which is the same
            // reason the block is checked as this screen's own code.
            ScreenLine::Use { body, .. } => check_lines(body, known, out),
            ScreenLine::Node(node) => {
                for arg in &node.args {
                    let ScreenArg::Named { span, name, value } = arg else {
                        continue;
                    };
                    if name != "style" {
                        continue;
                    }
                    let Some(vela_syntax::Expr::Name { name: style, .. }) = value else {
                        continue;
                    };
                    if known.contains(&style.as_str()) {
                        continue;
                    }
                    let mut diagnostic = diag(
                        "E5007",
                        format!("no style called `{style}`"),
                        *span,
                        "no style by this name is declared",
                    );
                    if let Some(nearest) = nearest(style, known) {
                        diagnostic = diagnostic.with_help(format!("did you mean `{nearest}`?"));
                    }
                    out.push(diagnostic);
                }
                check_lines(&node.children, known, out);
            }
        }
    }
}

/// The nearest declared name, if one is close enough to be worth offering.
fn nearest<'a>(name: &str, candidates: &[&'a str]) -> Option<&'a str> {
    candidates
        .iter()
        .copied()
        .map(|candidate| (vela_diag::edit_distance(name, candidate), candidate))
        .filter(|(distance, _)| *distance <= (name.chars().count() / 3).clamp(1, 3))
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

/// Renders a chain for a message.
fn show(chain: &[String]) -> String {
    chain
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(" -> ")
}
