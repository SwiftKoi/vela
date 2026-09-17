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
use vela_syntax::{Expr, ScreenArg, ScreenDecl, ScreenLine, ScreenNode};

use crate::actions::ActionRegistry;
use crate::compose;
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

/// Checks a screen body against the registry and the file's other screens.
///
/// `screens` is the file's declarations rather than a lookup built here, because composition is
/// per-file (`SCREENS.md §5`) and a caller that already has the parsed items should not have to build
/// a second index for the same question.
#[must_use]
pub fn check_screen(
    lines: &[ScreenLine],
    registry: &WidgetRegistry,
    screens: &[&ScreenDecl],
    actions: &ActionRegistry,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    // A screen's top level has no parent widget, so a bare prop there has nothing to belong
    // to and a line must be a widget.
    check_lines(lines, None, registry, &mut diagnostics);
    // `use` and `transclude` are a relationship between declarations, so their rules live together
    // in one place rather than being spread through this walk (`compose.rs`). The graph *between*
    // screens is a per-file question, so it is `compose::check_cycles`, asked once.
    compose::check_uses(screens, lines, &mut diagnostics);
    // Every call in a body is an action (`SCREENS.md §7`), and the registry is what says so.
    check_actions(lines, actions, &mut diagnostics);
    diagnostics
}

/// Checks every action a body calls against the registry.
///
/// A *walk over expressions* rather than a check of the `action` prop, because that prop is not the
/// only place an action is written: `use confirm("Stop?", quit(), close_screen())` passes two of them
/// as arguments, and a misspelled one there is the same mistake. The registry was a docs source
/// before this — nothing consulted it, so `action quitt()` was accepted and did nothing at all.
fn check_actions(lines: &[ScreenLine], actions: &ActionRegistry, out: &mut Vec<Diagnostic>) {
    for line in lines {
        match line {
            ScreenLine::Layer { .. }
            | ScreenLine::StylePrefix { .. }
            | ScreenLine::Transclude { .. } => {}
            ScreenLine::If {
                condition, body, ..
            } => {
                check_action_expr(condition, actions, out);
                check_actions(body, actions, out);
            }
            ScreenLine::Use { args, body, .. } => {
                for arg in args {
                    if let Some(value) = arg_value(arg) {
                        check_action_expr(value, actions, out);
                    }
                }
                check_actions(body, actions, out);
            }
            ScreenLine::Node(node) => {
                for arg in &node.args {
                    if let Some(value) = arg_value(arg) {
                        check_action_expr(value, actions, out);
                    }
                }
                check_actions(&node.children, actions, out);
            }
        }
    }
}

/// The expression an argument carries, if it carries one.
fn arg_value(arg: &ScreenArg) -> Option<&Expr> {
    match arg {
        ScreenArg::Value(value) => Some(value),
        ScreenArg::Named {
            value: Some(value), ..
        } => Some(value),
        // A bare name is a flag (`stretch_x`) or a leaf's content (`text line`) — not a value, and
        // so not an action.
        ScreenArg::Named { value: None, .. } => None,
    }
}

/// Checks one expression, and every expression inside it.
fn check_action_expr(expr: &Expr, actions: &ActionRegistry, out: &mut Vec<Diagnostic>) {
    match expr {
        Expr::Call { callee, args, span } => {
            // `foo.bar()` is a call on a value the screen holds, not a registry name: the language
            // has no such action, so there is nothing here to check.
            if let Expr::Name { name, .. } = callee.as_ref() {
                check_action(name, args.len(), *span, actions, out);
            }
            check_action_expr(callee, actions, out);
            for arg in args {
                check_action_expr(arg, actions, out);
            }
        }
        Expr::Str { parts, .. } => {
            for part in parts {
                if let vela_syntax::StrPart::Interpolation { expr, .. } = part {
                    check_action_expr(expr, actions, out);
                }
            }
        }
        Expr::Paren { inner, .. } | Expr::Field { base: inner, .. } => {
            check_action_expr(inner, actions, out);
        }
        Expr::Unary { operand, .. } => check_action_expr(operand, actions, out),
        Expr::Binary { lhs, rhs, .. } => {
            check_action_expr(lhs, actions, out);
            check_action_expr(rhs, actions, out);
        }
        Expr::Index { base, index, .. } => {
            check_action_expr(base, actions, out);
            check_action_expr(index, actions, out);
        }
        Expr::List { items, .. } => {
            for item in items {
                check_action_expr(item, actions, out);
            }
        }
        Expr::Map { entries, .. } => {
            for (key, value) in entries {
                check_action_expr(key, actions, out);
                check_action_expr(value, actions, out);
            }
        }
        Expr::If {
            cond, then_, else_, ..
        } => {
            check_action_expr(cond, actions, out);
            check_action_expr(then_, actions, out);
            check_action_expr(else_, actions, out);
        }
        Expr::Int { .. }
        | Expr::Float { .. }
        | Expr::Bool { .. }
        | Expr::None { .. }
        | Expr::Path { .. }
        | Expr::Name { .. }
        // A lambda's body runs when something calls it, which no screen does; an action cannot be
        // written in one.
        | Expr::Lambda { .. }
        | Expr::Error { .. } => {}
    }
}

/// One action call: the name is registered, and it takes this many arguments.
fn check_action(
    name: &str,
    given: usize,
    span: Span,
    actions: &ActionRegistry,
    out: &mut Vec<Diagnostic>,
) {
    let Some(action) = actions.get(name) else {
        let mut diagnostic = diag(
            "E5012",
            format!("no action called `{name}`"),
            span,
            "no action by this name is registered",
        );
        if let Some(nearest) = actions.closest(name) {
            diagnostic = diagnostic.with_help(format!("did you mean `{nearest}`?"));
        }
        out.push(diagnostic);
        return;
    };

    if action.arity() != given {
        out.push(
            diag(
                "E5013",
                format!(
                    "`{}` takes {} argument(s), but was given {given}",
                    action.name,
                    action.arity()
                ),
                span,
                "the arguments do not match the action",
            )
            .with_help(format!("write it as `{}`", action.signature())),
        );
    }
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
            ScreenLine::Layer { .. }
            | ScreenLine::StylePrefix { .. }
            | ScreenLine::Transclude { .. } => {}
            ScreenLine::If { body, .. } => check_lines(body, parent, registry, out),
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
