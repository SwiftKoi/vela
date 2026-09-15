//! Checking a widget tree against the registry.
//!
//! This is where the parser's deliberate ambiguity gets resolved. A screen line is a name
//! followed by words, and only the registry knows whether that name is a widget, a prop of the
//! widget above it, or neither:
//!
//! ```vela
//! box at bottom:      // `box` is a widget; `at` is a prop of it
//!     pad 24          // `pad` is not a widget, and `box` takes it — so it is a prop line
//!     column gap 8:   // `column` is a widget, and `gap` is a prop of it
//! ```
//!
//! Which is why `E5005` ("no such widget") and `E5006` ("a widget does not accept this prop")
//! exist as *checker* diagnostics and could not have been parse errors.
//!
//! The checker lives here rather than in `vela-compile` for a rank reason that is also a
//! design reason: compiling is rank 7 and the widget vocabulary is rank 8, so the only crates
//! that can see both the tree and the registry are the ones that *consume* screens — the CLI
//! and the language server.

use vela_diag::{Code, Diagnostic};
use vela_span::Span;
use vela_syntax::{ScreenArg, ScreenLine, ScreenNode};

use crate::widgets::{Widget, WidgetRegistry};

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

/// Checks a screen body against the registry.
#[must_use]
pub fn check_screen(lines: &[ScreenLine], registry: &WidgetRegistry) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    // A screen's top level has no parent widget, so a bare prop there has nothing to belong
    // to and a line must be a widget.
    check_lines(lines, None, registry, &mut diagnostics);
    diagnostics
}

/// Walks a body, with the widget its lines sit inside.
fn check_lines(
    lines: &[ScreenLine],
    parent: Option<&Widget>,
    registry: &WidgetRegistry,
    out: &mut Vec<Diagnostic>,
) {
    for line in lines {
        match line {
            ScreenLine::Layer { .. } => {}
            ScreenLine::If { body, .. } => check_lines(body, parent, registry, out),
            ScreenLine::Node(node) => check_node(node, parent, registry, out),
        }
    }
}

/// Checks one line, which is a widget, a prop of its parent, or a mistake.
fn check_node(
    node: &ScreenNode,
    parent: Option<&Widget>,
    registry: &WidgetRegistry,
    out: &mut Vec<Diagnostic>,
) {
    let Some(widget) = registry.get(&node.name) else {
        // Not a widget. If the widget above accepts it as a prop, this is a prop written on
        // its own line — which is how the spec writes one when a header would be crowded.
        if parent.is_some_and(|parent| parent.accepts(&node.name)) {
            check_children_of_prop(node, parent, registry, out);
            return;
        }
        out.push(unknown_widget(node, registry));
        // Its children are still walked, so one mistake does not hide the next.
        check_lines(&node.children, parent, registry, out);
        return;
    };

    check_args(node, widget, registry, out);

    // A widget that takes one child may not be given two. This is `E5004`, and it is the
    // diagnostic that catches the most common screen mistake after a typo: indenting a
    // sibling one level too far.
    let children = node
        .children
        .iter()
        .filter(|line| is_widget_line(line, registry));
    if widget.single_child && children.count() > 1 {
        out.push(wrong_arity(node, widget, node.children.len()));
    }

    check_lines(&node.children, Some(widget), registry, out);
}

/// The children of a prop line, which still belong to the parent widget.
fn check_children_of_prop(
    node: &ScreenNode,
    parent: Option<&Widget>,
    registry: &WidgetRegistry,
    out: &mut Vec<Diagnostic>,
) {
    check_lines(&node.children, parent, registry, out);
}

/// Whether a line names a widget rather than a prop.
fn is_widget_line(line: &ScreenLine, registry: &WidgetRegistry) -> bool {
    matches!(line, ScreenLine::Node(node) if registry.get(&node.name).is_some())
}

/// Checks the args after a name against what the widget declares.
fn check_args(
    node: &ScreenNode,
    widget: &Widget,
    registry: &WidgetRegistry,
    out: &mut Vec<Diagnostic>,
) {
    for (index, arg) in node.args.iter().enumerate() {
        let ScreenArg::Named { span, name, value } = arg else {
            continue;
        };
        if widget.accepts(name) {
            continue;
        }
        // A leaf's first arg may be its content rather than a prop, and the parser cannot tell
        // them apart: `text "hi"` is a value and `text line` is a name with no value, and both
        // mean "this is the text". Reporting the second as an unknown prop would reject a
        // screen the spec shows.
        let content = index == 0 && value.is_none() && widget.category == crate::Category::Leaf;
        if content {
            continue;
        }
        out.push(unknown_prop(node, widget, name, *span, registry));
    }
}

/// `E5005` — a line that is neither a widget nor a prop of the one above it.
fn unknown_widget(node: &ScreenNode, registry: &WidgetRegistry) -> Diagnostic {
    let mut diagnostic = diag(
        "E5005",
        format!("no widget called `{}`", node.name),
        node.span,
        "no widget by this name is registered",
    );
    let suggestions = suggestions_for(&node.name, registry.names());
    if !suggestions.is_empty() {
        diagnostic = diagnostic.with_help(format!("did you mean {}?", quoted(&suggestions)));
    }
    diagnostic
}

/// `E5006` — a prop the widget does not declare.
fn unknown_prop(
    node: &ScreenNode,
    widget: &Widget,
    name: &str,
    span: Span,
    registry: &WidgetRegistry,
) -> Diagnostic {
    let mut diagnostic = diag(
        "E5006",
        format!("`{}` does not take a prop called `{name}`", widget.name),
        span,
        "this widget does not declare it",
    );
    let suggestions = suggestions_for(name, widget.prop_names());
    if !suggestions.is_empty() {
        diagnostic = diagnostic.with_help(format!("did you mean {}?", quoted(&suggestions)));
    }
    let _ = registry;
    let _ = node;
    diagnostic
}

/// `E5004` — a single-child widget given more than one child.
fn wrong_arity(node: &ScreenNode, widget: &Widget, count: usize) -> Diagnostic {
    diag(
        "E5004",
        format!("`{}` takes one child, but was given {count}", widget.name),
        node.span,
        "this widget holds a single child",
    )
    .with_help("wrap the children in a `column` or `row`")
}

/// The nearest names, closest first.
fn suggestions_for(name: &str, candidates: Vec<&'static str>) -> Vec<&'static str> {
    let mut scored: Vec<(usize, &'static str)> = candidates
        .into_iter()
        .map(|candidate| (vela_diag::edit_distance(name, candidate), candidate))
        .filter(|(distance, _)| *distance <= (name.chars().count() / 3).clamp(1, 3))
        .collect();
    scored.sort_by_key(|(distance, _)| *distance);
    scored.into_iter().map(|(_, name)| name).collect()
}

/// Renders names for a help message.
fn quoted(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(" or ")
}
