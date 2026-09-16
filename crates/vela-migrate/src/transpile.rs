//! Turning a Ren'Py tree into Vela text.
//!
//! Two rules shape every arm. **Say what changed**: a construct translated with a loss — a
//! `Character` keyword with no Vela counterpart, a label renamed to escape a collision — leaves a
//! report entry naming the line, so the loss is a work item rather than a surprise. **Never
//! translate by resemblance**: [`Kind::Unsupported`] is reported, not inspected for something that
//! looks like Vela, because a line that happens to parse is the failure mode this crate exists to
//! avoid.
//!
//! Indentation is the structure, so it is generated rather than reproduced: the output is built
//! from the tree, which means a file migrated from irregular Ren'Py indentation comes out canonical
//! — and `project` runs it through the formatter too, so what lands is what `vela fmt` would write.
//!
//! Statements are one function per *family* — say, menu, flow, staging, audio, Python — which is how
//! the parser is arranged as well; declarations live next door in [`decl`].

mod decl;

use std::collections::BTreeMap;

use crate::expr;
use crate::report::Report;
use crate::rpy::{Kind, Node};

use decl::{default, define};

/// How far one level of nesting is indented.
const STEP: usize = 4;

/// The header every migrated file starts with.
const HEADER: &str = "# Migrated from Ren'Py by `vela migrate`. The report beside this file lists\n\
                      # everything the migration refused to guess at.\n";

/// The Vela source for one Ren'Py file.
///
/// `renames` is the map from a label's Ren'Py name to the name it had to take in Vela — see
/// `project::collisions` — and it is applied here, where a label is declared, and to every
/// `jump`/`call` that names one.
#[must_use]
pub fn file(
    nodes: &[Node],
    name: &str,
    report: &mut Report,
    renames: &BTreeMap<String, String>,
) -> String {
    let mut out = String::from(HEADER);
    block(nodes, 0, name, report, &mut out, renames);
    out
}

/// Emits a block of statements at a depth.
pub(super) fn block(
    nodes: &[Node],
    depth: usize,
    file: &str,
    report: &mut Report,
    out: &mut String,
    renames: &BTreeMap<String, String>,
) {
    for node in nodes {
        statement(node, depth, file, report, out, renames);
    }
}

/// Emits one statement, and its block if it opens one.
fn statement(
    node: &Node,
    depth: usize,
    file: &str,
    report: &mut Report,
    out: &mut String,
    renames: &BTreeMap<String, String>,
) {
    let pad = indent(depth);

    match &node.kind {
        Kind::Blank => out.push('\n'),
        Kind::Comment(text) => out.push_str(&format!("{pad}#{text}\n")),
        Kind::Label(name) => {
            out.push_str(&format!("{pad}label {}:\n", renamed(name, renames)));
            block(&node.children, depth + 1, file, report, out, renames);
        }
        Kind::Say {
            speaker,
            text,
            transition,
        } => say(speaker, text, transition.as_deref(), &pad, out),
        Kind::Menu => menu(node, depth, file, report, out, renames),
        Kind::If(condition) | Kind::Elif(condition) => {
            let keyword = if matches!(node.kind, Kind::If(_)) {
                "if"
            } else {
                "elif"
            };
            branch(keyword, condition, node, depth, file, report, out, renames);
        }
        Kind::Else => {
            out.push_str(&format!("{pad}else:\n"));
            block(&node.children, depth + 1, file, report, out, renames);
        }
        Kind::Jump(_) | Kind::Call(_) => transfer(node, &pad, report, file, out, renames),
        Kind::Return => out.push_str(&format!("{pad}return\n")),
        Kind::Scene(_) | Kind::Show(_) | Kind::Hide(_) => stage(node, &pad, out),
        Kind::With(transition) => out.push_str(&format!("{pad}with {}\n", transition.trim())),
        Kind::Play(_) | Kind::Stop(_) | Kind::Queue(_) => audio_line(node, &pad, file, report, out),
        Kind::Pause(seconds) => pause(seconds, &pad, node, file, report, out),
        Kind::Python(code) => python(code, &pad, node, file, report, out),
        Kind::Define { name, value } => define(node, name, value, depth, file, report, out),
        Kind::Default { name, value } => default(node, name, value, &pad, file, report, out),
        Kind::Unsupported => report_line(
            node,
            file,
            report,
            "no Vela equivalent: this line has to be ported by hand",
        ),
        Kind::Choice { .. } => {
            report_line(node, file, report, "a menu choice outside a `menu` block");
        }
        Kind::Window(_) => report_line(
            node,
            file,
            report,
            "`window` has no Vela equivalent: a screen decides what is on screen",
        ),
    }
}

/// `speaker "text"`, with a transition on the same line if there is one.
///
/// The text migrates **verbatim, tags and all**: `{b}` is Vela syntax, and the presenter reads the
/// tag rather than the compiler (`BYTECODE.md §3.3`), so a migrated line is byte-identical to the
/// one its author wrote.
fn say(speaker: &str, text: &str, transition: Option<&str>, pad: &str, out: &mut String) {
    let space = if speaker.is_empty() { "" } else { " " };
    let with = transition.map_or(String::new(), |name| format!(" with {name}"));
    out.push_str(&format!("{pad}{speaker}{space}{text}{with}\n"));
}

/// A `menu`, whose caption is a line of dialogue inside the block.
///
/// Ren'Py's caption is either a bare string or a character speaking it. Vela's is `menu "…":` — a
/// bare string — so a *spoken* prompt is emitted as the say it is, immediately before the menu. The
/// line is the same and in the same place, one command earlier; what differs is that Ren'Py keeps
/// it on screen beside the choices.
fn menu(
    node: &Node,
    depth: usize,
    file: &str,
    report: &mut Report,
    out: &mut String,
    renames: &BTreeMap<String, String>,
) {
    let pad = indent(depth);
    let caption = node.children.iter().position(|child| {
        matches!(
            child.kind,
            Kind::Say {
                transition: None,
                ..
            }
        )
    });

    if let Some(index) = caption
        && let Kind::Say { speaker, text, .. } = &node.children[index].kind
        && !speaker.is_empty()
    {
        out.push_str(&format!("{pad}{speaker} {text}\n"));
    }

    let head = match caption.map(|index| &node.children[index].kind) {
        Some(Kind::Say { speaker, text, .. }) if speaker.is_empty() => format!("menu {text}"),
        _ => "menu".to_string(),
    };
    out.push_str(&format!("{pad}{head}:\n"));

    for (index, child) in node.children.iter().enumerate() {
        if Some(index) == caption {
            continue;
        }
        match &child.kind {
            Kind::Choice { text, condition } => {
                choice(
                    child,
                    text,
                    condition.as_deref(),
                    depth,
                    file,
                    report,
                    out,
                    renames,
                );
            }
            // A `set` and friends inside a menu body are Vela statements in their own right.
            _ => statement(child, depth + 1, file, report, out, renames),
        }
    }
}

/// One `"text":` arm of a menu, and its body.
#[allow(clippy::too_many_arguments)]
fn choice(
    node: &Node,
    text: &str,
    condition: Option<&str>,
    depth: usize,
    file: &str,
    report: &mut Report,
    out: &mut String,
    renames: &BTreeMap<String, String>,
) {
    let guard = condition.map_or(String::new(), |cond| match expr::expression(cond) {
        Ok(cond) => format!(" if {cond}"),
        Err(why) => {
            report.push(file, node.line, &node.text, why);
            String::new()
        }
    });
    out.push_str(&format!("{}{text}{guard}:\n", indent(depth + 1)));
    block(&node.children, depth + 2, file, report, out, renames);
}

/// `if` / `elif`.
///
/// A condition that does not translate still keeps its block: a condition is one line to fix, and
/// dropping the body would lose the story inside it.
#[allow(clippy::too_many_arguments)]
fn branch(
    keyword: &str,
    condition: &str,
    node: &Node,
    depth: usize,
    file: &str,
    report: &mut Report,
    out: &mut String,
    renames: &BTreeMap<String, String>,
) {
    let pad = indent(depth);
    match expr::expression(condition) {
        Ok(condition) => out.push_str(&format!("{pad}{keyword} {condition}:\n")),
        Err(why) => {
            report.push(file, node.line, &node.text, why);
            out.push_str(&format!("{pad}{keyword} true:\n"));
        }
    }
    block(&node.children, depth + 1, file, report, out, renames);
}

/// `jump` / `call`, through the rename map.
fn transfer(
    node: &Node,
    pad: &str,
    report: &mut Report,
    file: &str,
    out: &mut String,
    renames: &BTreeMap<String, String>,
) {
    let (keyword, target) = match &node.kind {
        Kind::Jump(target) => ("jump", target),
        Kind::Call(target) => ("call", target),
        _ => return,
    };

    // A local label is scoped to its enclosing label in Ren'Py, and Vela has no such scope.
    if let Some(local) = target.strip_prefix('.') {
        report.push(
            file,
            node.line,
            &node.text,
            format!(
                "a local label (`.{local}`) is scoped to its enclosing label in Ren'Py and has no \
                 Vela equivalent yet; write `{keyword} {local}` and give it a label of its own"
            ),
        );
        out.push_str(&format!("{pad}{keyword} {local}\n"));
        return;
    }

    out.push_str(&format!(
        "{pad}{keyword} {}\n",
        renamed(target.trim(), renames)
    ));
}

/// `scene` / `show` / `hide`.
///
/// Passed through: Ren'Py's `scene bg lecturehall` is an image reference and an attribute list,
/// which is exactly Vela's `scene` statement — the shape was copied from it deliberately.
fn stage(node: &Node, pad: &str, out: &mut String) {
    let (keyword, rest) = match &node.kind {
        Kind::Scene(rest) => ("scene", rest),
        Kind::Show(rest) => ("show", rest),
        Kind::Hide(rest) => ("hide", rest),
        _ => return,
    };
    out.push_str(&format!("{pad}{keyword} {}\n", rest.trim()));
}

/// `play` / `stop` / `queue`, checked against Vela's shape rather than passed through.
///
/// Ren'Py's `fadein`/`fadeout`/`loop` clauses are refused rather than dropped: a track that stops
/// looping when it should not is a behaviour change nobody would look for in a migration.
fn audio_line(node: &Node, pad: &str, file: &str, report: &mut Report, out: &mut String) {
    let rest = match &node.kind {
        Kind::Play(rest) | Kind::Stop(rest) | Kind::Queue(rest) => rest,
        _ => return,
    };
    match audio(rest) {
        Ok(line) => out.push_str(&format!("{pad}{line}\n")),
        Err(why) => report.push(file, node.line, &node.text, why),
    }
}

/// `pause`, with or without a duration.
fn pause(seconds: &str, pad: &str, node: &Node, file: &str, report: &mut Report, out: &mut String) {
    if seconds.trim().is_empty() {
        out.push_str(&format!("{pad}pause\n"));
        return;
    }
    match expr::expression(seconds) {
        Ok(seconds) => out.push_str(&format!("{pad}pause {seconds}\n")),
        Err(why) => report.push(file, node.line, &node.text, why),
    }
}

/// A `$` statement: an assignment translates, anything else is Python and is reported.
fn python(code: &str, pad: &str, node: &Node, file: &str, report: &mut Report, out: &mut String) {
    let Some((name, value)) = expr::split_assignment(code) else {
        report.push(
            file,
            node.line,
            &node.text,
            "a Python statement: only a plain assignment translates, and this is not one",
        );
        return;
    };
    match expr::expression(&value) {
        Ok(value) => out.push_str(&format!("{pad}{name} = {value}\n")),
        Err(why) => report.push(file, node.line, &node.text, why),
    }
}

/// Reports a line that has no translation, emitting nothing for it.
fn report_line(node: &Node, file: &str, report: &mut Report, why: &str) {
    report.push(file, node.line, &node.text, why);
}

/// The indentation for a depth.
fn indent(depth: usize) -> String {
    " ".repeat(depth * STEP)
}

/// A `play`/`stop`/`queue` line, checked against Vela's shape.
///
/// Vela's statement is `play <channel> <source>`, which is Ren'Py's, so the translation is a
/// passthrough — but only for the shape both languages have.
fn audio(rest: &str) -> Result<String, String> {
    let mut parts = rest.split_whitespace();
    let Some(channel) = parts.next() else {
        return Err("an audio statement with no channel".to_string());
    };
    let Some(source) = parts.next() else {
        return Err("an audio statement with nothing to play".to_string());
    };
    if let Some(extra) = parts.next() {
        return Err(format!(
            "`{extra}` on an audio statement is Ren'Py-only; Vela's form is `play {channel} {source}`"
        ));
    }
    Ok(format!("play {channel} {source}"))
}

/// A label's Vela name: the one it had, unless a collision forced a rename.
fn renamed<'a>(name: &'a str, renames: &'a BTreeMap<String, String>) -> &'a str {
    renames.get(name).map_or(name, String::as_str)
}
