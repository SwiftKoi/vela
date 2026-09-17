//! The magic-colour lint.
//!
//! `SCREENS.md §5`: *"only tokens and typed values are permitted; a raw magic color in a
//! screen is `W4008`."*
//!
//! The reason is not tidiness. A colour written into a screen is a colour that cannot respond
//! to a theme, cannot be contrast-checked (`W4009` measures theme tokens), and cannot be
//! changed in one place when the art changes. It is also the one edit an author makes at 2am
//! and forgets, in a screen nobody looks at again — which is exactly the kind of thing a lint
//! is for and a code review is not.
//!
//! **How a colour is recognised.** By the prop's name, not its value. `color = 0x112233` is a
//! literal where a token belongs; `grow = 8` is not, and the parser does not record that one
//! number was written in hex and the other was not — the same limitation that made a theme's
//! type word load-bearing. A prop whose name says it is a colour is the rule that survives.

use vela_diag::{Code, Diagnostic};
use vela_syntax::{Expr, ScreenArg, ScreenLine};

/// Whether a prop name means "this is a colour".
///
/// A suffix rule rather than a list, so `background_color`, `tint`, and `color` are all
/// covered without the lint having to be told about each widget that gains one.
#[must_use]
pub fn is_colour_prop(name: &str) -> bool {
    name == "color"
        || name == "colour"
        || name == "tint"
        || name == "background"
        || name.ends_with("_color")
}

/// Reports literal colours written into a screen.
#[must_use]
pub fn check_magic_colours(lines: &[ScreenLine]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    check_lines(lines, &mut diagnostics);
    diagnostics
}

/// Walks the body looking for colour props with literal values.
fn check_lines(lines: &[ScreenLine], out: &mut Vec<Diagnostic>) {
    for line in lines {
        match line {
            ScreenLine::Layer { .. }
            | ScreenLine::StylePrefix { .. }
            | ScreenLine::Transclude { .. } => {}
            ScreenLine::If { .. } => {
                for arm in line.arms() {
                    check_lines(arm, out);
                }
            }
            // Written here, so a literal colour in it is this screen's literal.
            ScreenLine::Use { body, .. } => check_lines(body, out),
            ScreenLine::Node(node) => {
                for arg in &node.args {
                    let ScreenArg::Named {
                        span,
                        name,
                        value: Some(Expr::Int { value, .. }),
                    } = arg
                    else {
                        continue;
                    };
                    if !is_colour_prop(name) {
                        continue;
                    }
                    out.push(
                        diag(
                            "W4008",
                            format!("`{name}` is a raw colour (#{value:06x})"),
                            *span,
                            "a literal colour cannot follow a theme",
                        )
                        .with_help("use a theme token: `color = theme.accent`")
                        .with_note(
                            "`W4009` checks contrast between theme tokens, so a literal is \
                             outside every accessibility check as well",
                        ),
                    );
                }
                check_lines(&node.children, out);
            }
        }
    }
}

/// Builds a diagnostic for a registered code.
///
/// # Panics
///
/// Panics if the code is not registered, which is a bug here rather than anything a user can
/// trigger: `check-diag-codes` rejects an unregistered code before it can be committed.
fn diag(
    code: &str,
    message: impl Into<String>,
    span: vela_span::Span,
    label: impl Into<String>,
) -> Diagnostic {
    let code =
        Code::new(code).unwrap_or_else(|| panic!("`{code}` is not in crates/vela-diag/codes.txt"));
    Diagnostic::new(code, message, span, label)
}
