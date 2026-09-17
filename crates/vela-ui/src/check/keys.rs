use vela_diag::Diagnostic;
use vela_syntax::ScreenLine;

use crate::input::SemanticActions;

use super::diag;

/// `E5014` — a `key` naming something the host does not deliver.
///
/// Checked, because the alternative is a binding that looks live and answers nothing. Input arrives as
/// a *semantic* action (`SCREENS.md §11`), so a screen that misspells one is a screen no input
/// reaches — and nothing about the screen would look wrong.
pub(super) fn check_keys(
    lines: &[ScreenLine],
    inputs: &SemanticActions,
    out: &mut Vec<Diagnostic>,
) {
    for line in lines {
        match line {
            ScreenLine::Key { span, name, .. } => {
                if inputs.contains(name) {
                    continue;
                }
                let mut diagnostic = diag(
                    "E5014",
                    format!("no input action called `{name}`"),
                    *span,
                    "the host delivers no action by this name",
                );
                if let Some(nearest) = inputs.closest(name) {
                    diagnostic = diagnostic.with_help(format!("did you mean `{nearest}`?"));
                }
                out.push(diagnostic);
            }
            ScreenLine::If { .. } | ScreenLine::For { .. } => {
                for body in line.bodies() {
                    check_keys(body, inputs, out);
                }
            }
            // Written here, so a `key` in it is this screen's own binding.
            ScreenLine::Use { body, .. } => check_keys(body, inputs, out),
            ScreenLine::Node(node) => check_keys(&node.children, inputs, out),
            // The rest hold no binding: a layer, a prefix, a passthrough, a screen variable, and a timer
            // whose name is a piece of time rather than an action.
            ScreenLine::Layer { .. }
            | ScreenLine::StylePrefix { .. }
            | ScreenLine::Timer { .. }
            | ScreenLine::Default { .. }
            | ScreenLine::Transclude { .. } => {}
        }
    }
}
