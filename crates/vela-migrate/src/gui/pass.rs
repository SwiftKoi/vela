//! The entry point: parse, decide, build, report.

use super::{names, theme, values};
use crate::report::Report;
use crate::rpy::Node;

/// What the GUI pass produced.
pub struct Skin {
    /// The theme and its styles, as a Vela source file.
    pub source: String,
    /// The frame `gui.init(width, height)` declared, when it declares one.
    pub design: Option<(u32, u32)>,
    /// The styles the theme declares, and the tokens it holds.
    ///
    /// A second pass needs them, and the source is where they are: `screens.rpy` refers to Ren'Py's
    /// `gui_button` and to `gui.accent_color`, and Vela's names for those are the ones this pass
    /// chose — `button` and `accent`. Reading them back from the emitted text is what keeps the two
    /// naming rules from drifting apart, since they would otherwise be written twice.
    pub names: Names,
}

/// The names a theme declares.
#[derive(Clone, Debug, Default)]
pub struct Names {
    /// The styles it declares.
    pub styles: std::collections::BTreeSet<String>,
    /// The colour and font tokens it holds.
    pub tokens: std::collections::BTreeSet<String>,
    /// What a `gui.<name>` reference becomes in Vela, by the name a screen writes.
    pub values: std::collections::BTreeMap<String, String>,
}

impl Names {
    /// The styles and tokens a theme source declares.
    fn of(source: &str) -> Self {
        let mut names = Names::default();
        for line in source.lines() {
            if let Some(style) = line.trim_end().strip_prefix("style ") {
                names.styles.insert(style.trim_end_matches(':').to_string());
            }
            let trimmed = line.trim_end();
            if let Some(rest) = trimmed
                .strip_prefix("color ")
                .or_else(|| trimmed.strip_prefix("font "))
            {
                if let Some((name, _)) = rest.split_once(' ') {
                    names.tokens.insert(name.to_string());
                }
            }
        }
        names
    }
}

/// Reports the `init python:` blocks a theme does not read.
///
/// One of them is *read*: `gui.init(width, height)` is the design frame, and it lands in
/// `vela.toml`'s `[project] size` (`SCREENS.md §2.6`). The rest are Python, and the sample's second
/// block is the case worth naming — `@gui.variant def touch():` re-sets about forty GUI values for
/// a device Ren'Py detects at load time. Vela has no per-variant *theme*: a `variant()` decides at
/// **draw** time (`§2.6`), so a second set of values is a screen's `if variant("...")` arms rather
/// than a second theme, and the migration says which block was left rather than dropping it.
fn python(project: &str, nodes: &[Node], report: &mut Report) {
    for node in nodes {
        let head = node.text.split_whitespace().next().unwrap_or_default();
        if head != "init" && head != "python" {
            continue;
        }
        // The header line is skipped: what decides whether the block is read is what is *in* it.
        let body = block_text(node);
        let statements = body.lines().skip(1);
        if statements.clone().all(|line| {
            let line = line.trim();
            line.is_empty() || line.starts_with('#') || line.starts_with("gui.init(")
        }) {
            continue;
        }
        let what = match statements.clone().any(|line| line.contains("@gui.variant")) {
            true => {
                "a `@gui.variant` block, which re-sets this file's GUI values for a device Ren'Py \
                     picks between at load time"
            }
            false => "a block of Python",
        };
        report.push(
            project,
            node.line,
            &format!("{project}: {}", node.text.trim().trim_end_matches(':')),
            format!(
                "{what}. Vela has no per-variant theme: `variant()` decides at draw time \
                 (`SCREENS.md §2.6`), so a second set of values is a screen's own `if variant(...)` \
                 arms. The theme and its styles are the values declared outside this block"
            ),
        );
    }
}

/// A block's own text and everything under it, as lines.
fn block_text(node: &Node) -> String {
    let mut out = format!("{}\n", node.text);
    for child in &node.children {
        out.push_str(&block_text(child));
    }
    out
}

/// Translates a `gui.rpy` into a theme, or `None` when the file declares no `gui.` variables.
///
/// `screens` is the project's screen-language source, and it is what decides whether a variable is
/// *live*: one a screen reads, or that a `gui.<x>_properties("group")` splat covers, is
/// translated; one nothing reads is reported rather than dropped quietly.
pub fn skin(project: &str, nodes: &[Node], screens: &str, report: &mut Report) -> Option<Skin> {
    let variables = values::variables(nodes);
    if variables.is_empty() {
        return None;
    }
    python(project, nodes, report);
    let groups = names::splat_groups(screens);
    let live = names::live_names(screens, &groups, &variables);
    let built = theme::Theme::build(&variables, &groups, &live);

    let source = built.finish(project, &variables, report)?;
    let mut names = Names::of(&source);
    names.values = names::references(&variables, &groups);
    Some(Skin {
        source,
        design: values::design(nodes),
        names,
    })
}
