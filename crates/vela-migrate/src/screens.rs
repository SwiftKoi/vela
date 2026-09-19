//! `screens.rpy` → a Vela screen module (`SCREENS.md §2`).
//!
//! Ren'Py's screen language and Vela's look alike and are not the same, and the difference is worth
//! stating once rather than discovering per line:
//!
//! * **The widgets mostly rename.** `hbox`/`vbox` are `row`/`column`, `add` is `image`, `null` is
//!   `spacer`, `fixed` is `stack`, `frame`/`window` are `box`, and `label` is `text` — because a
//!   Vela `label` is a *prop*, not a widget. `textbutton "Yes"` becomes a `button` holding a `text`,
//!   which is what Vela's tree means by one, and `vpgrid` a `viewport` around a `grid`.
//! * **The styling comes across whole**, because the naming convention is the style (`gui.rs`):
//!   `style_prefix` and a file's `style` declarations are the same in both, with `style X is Y`
//!   becoming `style X from Y`, and `gui.accent_color` becoming `theme.accent` once the theme's
//!   names are known — which is why this pass is told them rather than re-deriving them.
//! * **The placement does not, and that is by design.** `SCREENS.md §4.2`'s rule is that a layout
//!   which needs arithmetic on pixel coordinates is the wrong layout: Vela places by `anchor` and
//!   `align` over nine *named* positions. So `xpos 240` is reported rather than translated — a file
//!   that kept the pixels would look migrated and would not scale with the frame. An `xalign`/
//!   `yalign` *pair* of fractions does come across, as the one of the nine positions it names; a
//!   lone axis has no Vela spelling, because `center` is a point rather than an axis.
//! * **What Vela has no system for is reported, and the entry names the system**: `tag`, `zorder`
//!   and `modal` are the screen stack and input capture; `key` is the input map, which knows a
//!   semantic action rather than a keycode; `transform`/`at`/`on` are animation, which
//!   `SCREENS.md §6` states as not yet built; a `python:` block is Python; a displayable is a
//!   picture a screen places.
//!
//! One rule matters more than any of them: **a line is written only if it still says something.**
//! A widget whose picture was reported, a block whose every line was, a style with nothing left —
//! each goes rather than being emitted empty, because `image` with an anchor after it is a parse
//! error and one of those hides every entry below it in the report.

mod body;
mod globals;
mod props;
mod report;
pub(crate) mod tables;
mod words;
pub use tables::known_actions;

use std::collections::{BTreeMap, BTreeSet};

use crate::gui::Names;
use crate::gui::{PAINTED, strip_state};
use crate::report::Report;
use crate::rpy::Node;

// One file per subject of the pass, so a reader of any of them can see what it is standing on: the
// walk (`body`), the props and actions (`props`), the words a line is made of (`words`), the tables
// that say what Ren'Py's names become (`tables`), the engine's own globals as a migrated screen reads
// them (`globals`, which the walk asks directly), and the report (`report`).
use body::*;
use props::*;
use report::*;
use tables::*;
use words::*;

/// What a lowering needs to know about the module it writes into.
///
/// The theme and this file end up in *one* module — a style resolves per file (`SCREENS.md §5`), so a
/// screen that reads `theme.accent` has to live beside the theme that declares it — which is why a
/// screen pass is told the theme's names rather than deriving them, and why the names a screen may
/// refer to are one set rather than two.
struct Ctx<'a> {
    /// The theme's tokens, styles and `gui.<name>` values (`gui.rs`).
    theme: &'a Names,
    /// The styles the module declares, after the renames a collision forced.
    styles: &'a BTreeSet<String>,
    /// `style X` → the name it kept, for a style a screen has taken the name of.
    renames: &'a BTreeMap<String, String>,
    /// What this screen or style could not write.
    gaps: Gaps,
}

impl Ctx<'_> {
    /// One style's Vela name, when the module declares that style.
    fn style(&self, written: &str) -> Option<String> {
        let name = unquote(written.trim().trim_end_matches(':'));
        let name = self.renames.get(&name).cloned().unwrap_or(name);
        self.styles.contains(&name).then_some(name)
    }

    /// A written value, with a `gui.<name>` reference resolved to what the theme holds for it.
    ///
    /// A reference the theme has no value for is *reported* rather than written: the migrated file
    /// would name something it does not declare, and a name that resolves to nothing is the one
    /// thing this report exists to prevent.
    fn resolved(&mut self, written: &str) -> Option<String> {
        let trimmed = written.trim();
        if trimmed.starts_with("gui.") && is_name(trimmed) {
            return match self.theme.values.get(trimmed) {
                Some(value) => Some(value.clone()),
                None => {
                    self.gaps
                        .unknown
                        .push(format!("`{trimmed}`, which no value in the theme holds"));
                    None
                }
            };
        }
        Some(value(trimmed))
    }
}

/// What a screen file became: the module's text, and the styles a screen took the name of.
pub struct Lowered {
    /// The screens and styles, as Vela.
    pub text: String,
    /// `style X` → the name it kept, which the merge applies to the theme as well.
    pub renames: BTreeMap<String, String>,
}

/// Lowers every `screen` and `style` in a file, or `None` when it declares neither.
///
/// Three passes, in this order, because each one answers a question the next one asks: which styles
/// this file declares *and* can write (a screen's `style` prop has to resolve, and the screen comes
/// first in the file), which of those names a screen has taken (the rename), and then the screens
/// themselves. Doing it in one pass would resolve a reference against a set that is not complete yet.
pub fn lower(
    relative: &str,
    nodes: &[Node],
    theme: &Names,
    report: &mut Report,
) -> Option<Lowered> {
    let renames = collisions(nodes, theme, &screen_names(nodes));
    for (from, to) in &renames {
        report.push(
            relative,
            1,
            &format!("{from} is both a screen and a style"),
            format!(
                "Ren'Py keeps screens and styles in separate namespaces and Vela keeps them in one, \
                 so the style is renamed `{to}` rather than lost. The screen keeps the name, because \
                 that is what a `use` and an `open_screen` refer to"
            ),
        );
    }
    let declared = survivors(nodes, theme, &renames);
    let styles_declared: BTreeSet<String> = theme
        .styles
        .iter()
        .map(|name| renamed(name, &renames))
        .chain(declared.iter().cloned())
        .collect();
    let mut writing = Writing {
        theme,
        styles_declared: &styles_declared,
        renames: &renames,
        relative,
        declared: &declared,
        seen: BTreeSet::new(),
        styles: Vec::new(),
        out: String::new(),
        count: 0,
    };
    for node in nodes {
        let (head, rest) = split_head(&node.text);
        match head.as_str() {
            "screen" => writing.screen(node, rest, report),
            "style" => writing.style(node, rest, report),
            _ => {}
        }
    }
    // The styles last, under the screens: a file reads top-down, and what draws with them comes
    // first. `mem::take` rather than a borrow, because the text is appended as it is drained.
    let styles = std::mem::take(&mut writing.styles);
    for (_, text) in &styles {
        writing.out.push_str(text);
    }
    (writing.count > 0).then_some(Lowered {
        text: writing.out,
        renames,
    })
}

/// Pass two's own state: the names it resolves against, what it has accumulated, and the file it
/// reports against.
struct Writing<'a> {
    theme: &'a Names,
    styles_declared: &'a BTreeSet<String>,
    renames: &'a BTreeMap<String, String>,
    relative: &'a str,
    declared: &'a BTreeSet<String>,
    seen: BTreeSet<String>,
    styles: Vec<(String, String)>,
    out: String,
    count: usize,
}

impl Writing<'_> {
    /// One `screen`: its body, or the entry saying why it was not written.
    fn screen(&mut self, node: &Node, rest: &str, report: &mut Report) {
        let name = declared_name(rest);
        // A second declaration of one name is Ren'Py's way of giving a *variant* its own body — the
        // sample replaces `quick_menu` for touch devices — and Vela has one screen per name with a
        // `variant()` condition inside it (`SCREENS.md §2.6`). The first declaration is kept and the
        // second reported: which body an author wants is a question for a person, and a variant
        // Vela cannot answer (`touch`) is `E5018`.
        if !self.seen.insert(name.clone()) {
            report.push(
                self.relative,
                1,
                &format!("screens.rpy: `screen {name}`, declared twice"),
                "Ren'Py's later declaration is the one a variant draws. Vela has one screen per name \
                 and asks the host about a variant inside it, so the second declaration is not \
                 translated — merge the two by hand, or write the variant as a condition \
                 (`SCREENS.md §2.6`)",
            );
            return;
        }
        // The references are copied out of `self` rather than borrowed through it, so the walk can
        // push to `self.out` while its context is alive.
        let mut ctx = Ctx {
            theme: self.theme,
            styles: self.styles_declared,
            renames: self.renames,
            gaps: Gaps::default(),
        };
        let mut inner = String::new();
        body(&node.children, 1, &mut inner, &mut ctx);
        if inner.trim().is_empty() {
            // Every line named a system Vela has no counterpart for. Reported rather than written:
            // a screen with an empty body is a parse error.
            ctx.gaps.unknown.push("the screen's whole body".to_string());
        } else {
            self.out
                .push_str(&format!("screen {}:\n{inner}", value(rest)));
            self.count += 1;
        }
        if !ctx.gaps.is_empty() {
            ctx.gaps.report(self.relative, &name, report);
        }
    }

    /// One `style`: the settings Vela paints, accumulated under the name, and the report for the
    /// rest. A style that has nothing to say is left out rather than written empty.
    fn style(&mut self, node: &Node, rest: &str, report: &mut Report) {
        let name = declared_name(rest);
        let mut ctx = Ctx {
            theme: self.theme,
            styles: self.styles_declared,
            renames: self.renames,
            gaps: Gaps::default(),
        };
        let mut text = String::new();
        style_body(&node.children, 1, &mut text, &mut ctx);
        // Reported before the decision to write it, because a style Vela cannot express is exactly
        // the one whose entries matter: its placement, its pictures and its splats are the report's
        // work list for the skin.
        if !ctx.gaps.is_empty() {
            ctx.gaps.report(self.relative, &name, report);
        }
        if !self.declared.contains(&renamed(&name, self.renames)) || text.trim().is_empty() {
            return;
        }
        match self
            .styles
            .iter_mut()
            .find(|(declared, _)| *declared == name)
        {
            Some((_, body)) => body.push_str(&text),
            None => self.styles.push((
                name.clone(),
                format!("{}\n{text}", style_header(&name, rest, &ctx)),
            )),
        }
        self.count += 1;
    }
}

/// Pass one: which of a file's styles survive.
///
/// A style whose body says nothing is left out — Ren'Py's own `style X is default` sets nothing, and
/// a Vela `style X:` with an empty body is a parse error — and the set is what the screens' `style`
/// props and the file's own `from` bases are resolved against. It is a pass of its own because a
/// screen comes *before* the styles it names in the file, so the answer has to exist before the
/// screens are walked.
fn survivors(
    nodes: &[Node],
    theme: &Names,
    renames: &BTreeMap<String, String>,
) -> BTreeSet<String> {
    let all = style_names(nodes);
    let mut declared = BTreeSet::new();
    for node in nodes {
        let (head, rest) = split_head(&node.text);
        if head != "style" {
            continue;
        }
        let mut ctx = Ctx {
            theme,
            styles: &all,
            renames,
            gaps: Gaps::default(),
        };
        let mut text = String::new();
        style_body(&node.children, 1, &mut text, &mut ctx);
        if !text.trim().is_empty() {
            declared.insert(renamed(&declared_name(rest), renames));
        }
    }
    declared
}

/// The styles a screen has taken the name of, and the names they keep.
///
/// Vela keeps screens and styles in one namespace and Ren'Py keeps them apart, so a screen called
/// `main_menu` and a style called `main_menu` cannot both keep the name. The *screen* keeps it —
/// that is what an `open_screen` call and a `use` refer to — and the style is renamed, with a report
/// entry saying so, because both are real names in the file they came from.
fn collisions(
    nodes: &[Node],
    theme: &Names,
    screens: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    let mut renames = BTreeMap::new();
    let styles = style_names(nodes)
        .into_iter()
        .chain(theme.styles.iter().cloned());
    for name in styles {
        if screens.contains(&name) {
            renames.insert(name.clone(), format!("{name}_style"));
        }
    }
    renames
}

/// A style's name under the renames.
fn renamed(name: &str, renames: &BTreeMap<String, String>) -> String {
    renames
        .get(name)
        .cloned()
        .unwrap_or_else(|| name.to_string())
}

/// The `style` names a file declares, whether or not their bodies survive.
fn style_names(nodes: &[Node]) -> BTreeSet<String> {
    nodes
        .iter()
        .filter_map(|node| {
            let text = node.text.trim_start();
            let rest = text.strip_prefix("style ")?;
            let name = declared_name(rest);
            (!name.is_empty()).then_some(name)
        })
        .collect()
}

/// The name a declaration header starts with: up to a `(`, a `:` or the ` is ` of a base.
fn declared_name(rest: &str) -> String {
    let rest = rest.trim();
    let end = rest
        .find(['(', ':'])
        .or_else(|| rest.find(" is "))
        .unwrap_or(rest.len());
    rest[..end].trim().to_string()
}

/// The screen names a file declares, for the rename a merge has to do.
///
/// The name ends at the `(` of a parameter list or at the `:` of the header: `screen main_menu():`
/// declares `main_menu`, and reading it by whitespace alone would return `main_menu():` — which
/// never matches a style, so the collision this exists to find went unfound.
pub fn screen_names(nodes: &[Node]) -> BTreeSet<String> {
    nodes
        .iter()
        .filter_map(|node| {
            let text = node.text.trim_start();
            let rest = text.strip_prefix("screen ")?;
            let end = rest.find(['(', ':']).unwrap_or(rest.len());
            Some(rest[..end].trim().to_string())
        })
        .filter(|name| !name.is_empty())
        .collect()
}
