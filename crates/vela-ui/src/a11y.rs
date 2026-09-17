//! The accessibility tree, and the lint that keeps it useful.
//!
//! `SCREENS.md §10`: *"structural, not a mode."* Every interactive widget emits a node — role,
//! label, value, state, focus order — and self-voicing reads that tree rather than each widget
//! growing its own audio code. So a new widget is voiced the moment it is registered, which is
//! the same property the registry gives everything else.
//!
//! The tree is derived, not declared. A screen author writes a button; the node that describes
//! it is a consequence of the widget, its children, and its props — which means a screen cannot
//! forget to be accessible, only fail to be *labelled*, and that is one lint.
//!
//! **Focus order is tree order.** The spec says it is derived from tree order and `absolute`
//! positions; tree order alone is what this produces, and `absolute` is the escape hatch that
//! makes it a lie, which is why `W4007` lints that separately.

use vela_diag::{Code, Diagnostic};
use vela_syntax::{Expr, ScreenArg, ScreenDecl, ScreenLine, ScreenNode};

use crate::compose;
use crate::widgets::{Category, WidgetRegistry};

/// How far a `use` chain is followed before the walk gives up.
///
/// A cycle is refused by the checker (`E5011`), so this is a bound on a *crafted* pack rather than on
/// a file — the reader of a pack is total, and a total reader must not hand the walker something that
/// recurses until the stack ends.
const MAX_DEPTH: usize = 32;

/// What a node is, in terms a screen reader uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// Something that can be activated.
    Button,
    /// Something with a value in a range.
    Slider,
    /// Something that takes typed text.
    TextField,
    /// Static text.
    Text,
    /// A picture.
    Image,
    /// Anything that only groups.
    Group,
}

impl Role {
    /// The role for a registered widget.
    #[must_use]
    pub fn of(name: &str, category: Category) -> Self {
        // `category` is not consulted: a `text` in a container role is still text. It stays in
        // the signature because a widget registered by a plugin has no name we know, and the
        // category is the fallback that keeps an unknown interactive widget from being a group.
        match name {
            "button" => Self::Button,
            "bar" | "slider" => Self::Slider,
            "input" => Self::TextField,
            "text" => Self::Text,
            "image" | "video" | "live2d" => Self::Image,
            _ if category == Category::Interactive => Self::Button,
            _ => Self::Group,
        }
    }

    /// The name a screen reader announces.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Slider => "slider",
            Self::TextField => "text field",
            Self::Text => "text",
            Self::Image => "image",
            Self::Group => "group",
        }
    }
}

/// One node of the accessibility tree.
#[derive(Clone, PartialEq, Debug)]
pub struct A11yNode {
    /// What it is.
    pub role: Role,
    /// What it is called, if anything.
    pub label: Option<String>,
    /// Where it sits in the focus order, for something focusable.
    pub focus: Option<usize>,
    /// Its children, in reading order.
    pub children: Vec<A11yNode>,
}

/// Builds the accessibility tree for a screen body.
///
/// A screen is expanded where it is *used*, not read as a declaration: a screen reader is reading what
/// is drawn, so a `use` contributes the used screen's nodes and a `transclude` contributes the block
/// the caller passed (`compose.rs`).
#[must_use]
pub fn tree(
    lines: &[ScreenLine],
    registry: &WidgetRegistry,
    screens: &[&ScreenDecl],
) -> Vec<A11yNode> {
    let mut focus = 0usize;
    nodes(lines, registry, screens, None, 0, &mut focus)
}

/// Builds the nodes for a list of lines.
///
/// `pane` is the block this body places at its `transclude`, and it belongs to the body that passed
/// it — one level up the composition, which is why it travels separately from `lines`.
fn nodes(
    lines: &[ScreenLine],
    registry: &WidgetRegistry,
    screens: &[&ScreenDecl],
    pane: Option<&[ScreenLine]>,
    depth: usize,
    focus: &mut usize,
) -> Vec<A11yNode> {
    if depth > MAX_DEPTH {
        return Vec::new();
    }
    lines
        .iter()
        .filter_map(|line| match line {
            // A `layer` line and a `style_prefix` line place nothing: the first names where the
            // screen draws, the second how its widgets look.
            ScreenLine::Layer { .. } | ScreenLine::StylePrefix { .. } => None,
            // A conditional contributes its branches' nodes: what a screen reader reads is what
            // is on screen, and which branch that is is a runtime question.
            ScreenLine::If { body, .. } => Some(nodes(body, registry, screens, pane, depth, focus)),
            ScreenLine::Use { name, body, .. } => Some(match compose::find(screens, name) {
                Some(callee) => nodes(
                    &callee.body,
                    registry,
                    screens,
                    Some(body),
                    depth + 1,
                    focus,
                ),
                // A name that resolves to nothing contributes its block alone: the checker has
                // already reported the name, and dropping the author's nodes too would double the
                // damage.
                None => nodes(body, registry, screens, pane, depth, focus),
            }),
            ScreenLine::Transclude { .. } => {
                pane.map(|pane| nodes(pane, registry, screens, None, depth, focus))
            }
            ScreenLine::Node(node) => {
                Some(vec![node_of(node, registry, screens, pane, depth, focus)])
            }
        })
        .flatten()
        .collect()
}

/// Builds one node, and its children.
fn node_of(
    node: &ScreenNode,
    registry: &WidgetRegistry,
    screens: &[&ScreenDecl],
    pane: Option<&[ScreenLine]>,
    depth: usize,
    focus: &mut usize,
) -> A11yNode {
    let widget = registry.get(&node.name);
    let category = widget.map_or(Category::Container, |widget| widget.category);
    let role = Role::of(&node.name, category);
    let label = label_of(node, registry);

    // Focus order is assigned in tree order, only to things that can be focused. An index that
    // counted containers too would make "tab three times" land somewhere surprising.
    let focusable = category == Category::Interactive;
    let index = if focusable {
        let index = *focus;
        *focus += 1;
        Some(index)
    } else {
        None
    };

    A11yNode {
        role,
        label,
        focus: index,
        children: nodes(&node.children, registry, screens, pane, depth, focus),
    }
}

/// What a node is called.
///
/// An explicit `label` wins; failing that, a single `text` child is the label, because a button
/// whose whole content is the words "Tell the truth" is named by those words. Anything else has
/// no label, which for an interactive widget is `W4010`.
fn label_of(node: &ScreenNode, registry: &WidgetRegistry) -> Option<String> {
    for arg in &node.args {
        if let ScreenArg::Named {
            name,
            value: Some(Expr::Str { parts, .. }),
            ..
        } = arg
        {
            if name == "label" {
                return literal_text(parts);
            }
        }
    }

    let mut texts: Vec<String> = Vec::new();
    collect_text(&node.children, registry, &mut texts);
    if texts.len() == 1 {
        return texts.pop();
    }
    None
}

/// The static text of a string literal.
fn literal_text(parts: &[vela_syntax::StrPart]) -> Option<String> {
    let text: String = parts
        .iter()
        .filter_map(|part| match part {
            vela_syntax::StrPart::Literal { text, .. } => Some(text.clone()),
            // An interpolation is not a label: what it reads is a runtime value, and a screen
            // reader needs something it can announce before the story starts.
            vela_syntax::StrPart::Interpolation { .. } => None,
        })
        .collect();
    // A string that is *only* an interpolation has no static text, and an empty label is worse
    // than none: a screen reader would announce nothing rather than falling back.
    (!text.is_empty()).then_some(text)
}

/// Collects the text of `text` children.
fn collect_text(children: &[ScreenLine], registry: &WidgetRegistry, out: &mut Vec<String>) {
    for line in children {
        let ScreenLine::Node(child) = line else {
            continue;
        };
        if registry.get(&child.name).map(|w| w.name) == Some("text") {
            if let Some(ScreenArg::Value(Expr::Str { parts, .. })) = child.args.first() {
                if let Some(text) = literal_text(parts) {
                    out.push(text);
                }
                continue;
            }
        }
        collect_text(&child.children, registry, out);
    }
}

/// `W4010` — an interactive widget with nothing to announce.
///
/// A warning rather than an error: an unlabelled button is navigable, it is just unusable by
/// someone who cannot see it. And it is a lint rather than a hard requirement because a screen
/// mid-authoring has unlabelled buttons all the time, and a build that refused would make
/// people disable the check rather than write the label.
#[must_use]
pub fn check_labels(lines: &[ScreenLine], registry: &WidgetRegistry) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    check_lines(lines, registry, &mut diagnostics);
    diagnostics
}

/// Walks the body looking for unlabelled interactive widgets.
fn check_lines(lines: &[ScreenLine], registry: &WidgetRegistry, out: &mut Vec<Diagnostic>) {
    for line in lines {
        match line {
            ScreenLine::Layer { .. }
            | ScreenLine::StylePrefix { .. }
            | ScreenLine::Transclude { .. } => {}
            ScreenLine::If { body, .. } => check_lines(body, registry, out),
            // The block is this screen's own code, so a button written in it is checked here. The
            // used screen's body is checked when that screen is checked — per screen, like every
            // other per-file rule.
            ScreenLine::Use { body, .. } => check_lines(body, registry, out),
            ScreenLine::Node(node) => {
                let interactive = registry
                    .get(&node.name)
                    .is_some_and(|widget| widget.category == Category::Interactive);
                if interactive && label_of(node, registry).is_none() {
                    out.push(unlabelled(node));
                }
                check_lines(&node.children, registry, out);
            }
        }
    }
}

/// The `W4010` diagnostic.
fn unlabelled(node: &ScreenNode) -> Diagnostic {
    let code = Code::new("W4010").expect("`W4010` is in crates/vela-diag/codes.txt");
    Diagnostic::new(
        code,
        format!("`{}` has no label", node.name),
        node.span,
        "a screen reader has nothing to announce",
    )
    .with_help("add a `label`, or put a single `text` inside it")
    .with_note("`vela test --a11y` reports these as a group")
}
