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

use std::collections::{BTreeMap, BTreeSet};

use crate::expr;
use crate::report::Report;
use crate::rpy::{Kind, Node};

use decl::{default, define};

/// What a file needs to know about the *project* it is part of.
///
/// Two things. Labels that had to be renamed to escape a collision (`project::collisions`), and the
/// images Ren'Py defined automatically from file names (`project::image_name`) — which the story's
/// `scene`/`show` names have to be rewritten to match, or the picture never appears.
pub struct Names<'a> {
    /// A label's Ren'Py name, to the name it took in Vela.
    pub renames: &'a BTreeMap<String, String>,
    /// The dotted image names in the migrated project.
    pub images: &'a BTreeSet<String>,
}

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
pub fn file(nodes: &[Node], name: &str, report: &mut Report, names: &Names<'_>) -> String {
    let mut out = String::from(HEADER);
    block(nodes, 0, name, report, &mut out, names);
    out
}

/// Emits a block of statements at a depth.
pub(super) fn block(
    nodes: &[Node],
    depth: usize,
    file: &str,
    report: &mut Report,
    out: &mut String,
    names: &Names<'_>,
) {
    for node in nodes {
        statement(node, depth, file, report, out, names);
    }
}

/// Emits one statement, and its block if it opens one.
fn statement(
    node: &Node,
    depth: usize,
    file: &str,
    report: &mut Report,
    out: &mut String,
    names: &Names<'_>,
) {
    let pad = indent(depth);

    match &node.kind {
        Kind::Blank => out.push('\n'),
        Kind::Comment(text) => out.push_str(&format!("{pad}#{text}\n")),
        Kind::Label(name) => {
            out.push_str(&format!("{pad}label {}:\n", renamed(name, names.renames)));
            block(&node.children, depth + 1, file, report, out, names);
        }
        Kind::Say {
            speaker,
            text,
            transition,
        } => say(speaker, text, transition.as_deref(), &pad, out),
        Kind::Menu => menu(node, depth, file, report, out, names),
        Kind::If(condition) | Kind::Elif(condition) => {
            let keyword = if matches!(node.kind, Kind::If(_)) {
                "if"
            } else {
                "elif"
            };
            branch(keyword, condition, node, depth, file, report, out, names);
        }
        Kind::Else => {
            out.push_str(&format!("{pad}else:\n"));
            block(&node.children, depth + 1, file, report, out, names);
        }
        Kind::Jump(_) | Kind::Call(_) => transfer(node, &pad, report, file, out, names),
        Kind::Return => out.push_str(&format!("{pad}return\n")),
        Kind::Scene(_) | Kind::Show(_) | Kind::Hide(_) => {
            stage(node, depth, file, report, out, names);
        }
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
    names: &Names<'_>,
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
                    names,
                );
            }
            // A `set` and friends inside a menu body are Vela statements in their own right.
            _ => statement(child, depth + 1, file, report, out, names),
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
    names: &Names<'_>,
) {
    let guard = condition.map_or(String::new(), |cond| match expr::expression(cond) {
        Ok(cond) => format!(" if {cond}"),
        Err(why) => {
            report.push(file, node.line, &node.text, why);
            String::new()
        }
    });
    out.push_str(&format!("{}{text}{guard}:\n", indent(depth + 1)));
    block(&node.children, depth + 2, file, report, out, names);
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
    names: &Names<'_>,
) {
    let pad = indent(depth);
    match expr::expression(condition) {
        Ok(condition) => out.push_str(&format!("{pad}{keyword} {condition}:\n")),
        Err(why) => {
            report.push(file, node.line, &node.text, why);
            out.push_str(&format!("{pad}{keyword} true:\n"));
        }
    }
    block(&node.children, depth + 1, file, report, out, names);
}

/// `jump` / `call`, through the rename map.
fn transfer(
    node: &Node,
    pad: &str,
    report: &mut Report,
    file: &str,
    out: &mut String,
    names: &Names<'_>,
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
        renamed(target.trim(), names.renames)
    ));
}

/// `scene` / `show` / `hide`.
///
/// Ren'Py names an image with a *tag and attributes* — `show sylvie green normal` — and Vela names
/// it with a dotted path: `sylvie.green.normal`. The migration reproduces Ren'Py's automatic image
/// definitions and rewrites the reference to match, which is what makes the picture appear at all —
/// an attribute Vela does not know about stages nothing.
///
/// A reference that names no image in the project is left as it is and reported. The honest reason
/// differs by case, and both are worth a person's eye: Ren'Py has built-in `black` and `white`
/// images that Vela does not, and a misspelled name is a misspelled name.
fn stage(
    node: &Node,
    depth: usize,
    file: &str,
    report: &mut Report,
    out: &mut String,
    names: &Names<'_>,
) {
    let (keyword, rest) = match &node.kind {
        Kind::Scene(rest) => ("scene", rest),
        Kind::Show(rest) => ("show", rest),
        Kind::Hide(rest) => ("hide", rest),
        _ => return,
    };
    let pad = indent(depth);

    // `scene bg uni with fade` puts the transition on the line it stages on.
    let (reference, transition) = match rest.split_once(" with ") {
        Some((reference, transition)) => (reference.trim(), Some(transition.trim())),
        None => (rest.trim(), None),
    };
    let with = transition.map_or(String::new(), |name| format!(" with {name}"));
    let dotted = reference.split_whitespace().collect::<Vec<_>>().join(".");

    if names.images.contains(&dotted) {
        out.push_str(&format!("{pad}{keyword} {dotted}{with}\n"));
        return;
    }

    report.push(
        file,
        node.line,
        &node.text,
        format!(
            "`{reference}` names no image in this project. Ren'Py defines an image from every image \
             file's name, so this is either a name with no file — Ren'Py's built-in `black` and \
             `white` have no Vela equivalent — or a typo. Nothing is staged by it"
        ),
    );
    out.push_str(&format!("{pad}{keyword} {reference}{with}\n"));
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
