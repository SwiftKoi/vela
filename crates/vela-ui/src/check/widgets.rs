use vela_diag::Diagnostic;
use vela_span::Span;
use vela_syntax::{ScreenArg, ScreenLine, ScreenNode};

use crate::widgets::{Widget, WidgetRegistry};

use super::diag;

/// Walks a body, with the widget its lines sit inside.
pub(super) fn check_lines(
    lines: &[ScreenLine],
    parent: Option<&Widget>,
    registry: &WidgetRegistry,
    out: &mut Vec<Diagnostic>,
) {
    for line in lines {
        match line {
            ScreenLine::Layer { .. }
            | ScreenLine::StylePrefix { .. }
            | ScreenLine::Key { .. }
            | ScreenLine::Timer { .. }
            | ScreenLine::Transclude { .. } => {}
            // A variable is not a widget, and its own rules are `variables.rs`'.
            ScreenLine::Default { .. } => {}
            ScreenLine::If { .. } | ScreenLine::For { .. } => {
                for body in line.bodies() {
                    check_lines(body, parent, registry, out);
                }
            }
            // A `use`'s block is this screen's own code, sitting where the `use` is — so it is
            // checked against the same parent widget, and its own widgets and props are checked
            // like any other line. The used screen's body is checked when *it* is checked.
            ScreenLine::Use { body, .. } => check_lines(body, parent, registry, out),
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
        .filter(|line| is_child_line(line, registry));
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

/// Whether a line is a child of the widget above it rather than a prop of it.
///
/// A `use` is one child: what it draws is decided by the screen it names, and a single-child widget
/// given a composition cannot be told statically how many nodes that produces. Counting it as one is
/// the answer the arity rule needs — it catches a `box` with a `use` *and* a `text`, which is the
/// mistake the rule exists for, without pretending to know the used screen's shape.
fn is_child_line(line: &ScreenLine, registry: &WidgetRegistry) -> bool {
    match line {
        ScreenLine::Use { .. } => true,
        ScreenLine::Node(node) => registry.get(&node.name).is_some(),
        _ => false,
    }
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
