//! Declarations: `define`, `default`, and the `Character(...)` call.
//!
//! A Ren'Py declaration is where a project's *shape* comes from — a character's name and colour, the
//! type a `default` implies — so these are the arms that have to be careful rather than clever. Two
//! are refused outright rather than approximated: a `define` of a Ren'Py *store* (`config.name`),
//! because Vela has no such store, and a `default` whose value is not a literal, because
//! `RUNTIME.md §5` derives the save schema from the source and a schema cannot be inferred from an
//! expression.

use super::indent;
use crate::expr;
use crate::report::Report;
use crate::rpy::Node;

/// `define`, which is either a character or a constant.
pub(super) fn define(
    node: &Node,
    name: &str,
    value: &str,
    depth: usize,
    file: &str,
    report: &mut Report,
    out: &mut String,
) {
    let pad = indent(depth);

    // Ren'Py's `config`/`gui`/`persistent` stores have no Vela counterpart, and a dotted name is
    // how a script reaches them. Translating one would emit `const config.name = …`, which is not a
    // declaration Vela accepts — a mistranslation that looks like a translation.
    if name.contains('.') {
        report.push(
            file,
            node.line,
            &node.text,
            "a `define` of a Ren'Py store (`config.…`, `gui.…`, `persistent.…`): Vela has no such \
             store — a project's settings are its screens and its `vela.toml`",
        );
        return;
    }

    if let Some(args) = value
        .trim()
        .strip_prefix("Character(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        character(node, name, args, depth, file, report, out);
        return;
    }

    match expr::expression(value) {
        Ok(value) => out.push_str(&format!("{pad}const {name} = {value}\n")),
        Err(why) => report.push(file, node.line, &node.text, why),
    }
}

/// `Character(...)` as a Vela `character` block.
///
/// The first positional argument is the name a player sees; `color` and `image` have counterparts.
/// Anything else — `what_prefix`, `ctc`, `voice_tag` — is reported rather than dropped, because a
/// character that loses its click-to-continue marker is a difference a player notices.
fn character(
    node: &Node,
    name: &str,
    args: &str,
    depth: usize,
    file: &str,
    report: &mut Report,
    out: &mut String,
) {
    let pad = indent(depth + 1);
    out.push_str(&format!("{}character {name}:\n", indent(depth)));

    for (keyword, argument) in arguments(args) {
        match keyword.as_str() {
            "" => match expr::without_translation_call(&argument) {
                Some(text) => {
                    report.push(
                        file,
                        node.line,
                        &node.text,
                        "`_()` is Ren'Py's translation marker and Vela has no catalogue yet: the \
                         string is kept untranslated",
                    );
                    out.push_str(&format!("{pad}name = {text}\n"));
                }
                None => match expr::expression(&argument) {
                    Ok(text) => out.push_str(&format!("{pad}name = {text}\n")),
                    Err(why) => report.push(file, node.line, &node.text, why),
                },
            },
            "color" => out.push_str(&format!("{pad}color = {}\n", colour(&argument))),
            "image" => out.push_str(&format!("{pad}image = {argument}\n")),
            other => report.push(
                file,
                node.line,
                &node.text,
                format!("`{other}=` on a `Character` has no Vela counterpart"),
            ),
        }
    }
}

/// A `default`, which needs the type its literal implies.
pub(super) fn default(
    node: &Node,
    name: &str,
    value: &str,
    pad: &str,
    file: &str,
    report: &mut Report,
    out: &mut String,
) {
    let Some(ty) = expr::literal_type(value) else {
        report.push(
            file,
            node.line,
            &node.text,
            "a `default` whose value is not a literal: Vela derives its save schema from the source \
             (`RUNTIME.md §5`), so the declared type has to be written down",
        );
        return;
    };
    match expr::expression(value) {
        Ok(value) => out.push_str(&format!("{pad}default {name}: {ty} = {value}\n")),
        Err(why) => report.push(file, node.line, &node.text, why),
    }
}

/// Ren'Py's `"#rrggbb"` as Vela's `0xrrggbb`.
fn colour(argument: &str) -> String {
    let argument = argument.trim().trim_matches('"');
    match argument.strip_prefix('#') {
        Some(hex) => format!("0x{hex}"),
        None => argument.to_string(),
    }
}

/// The top-level `keyword=value` arguments of a call, in order.
fn arguments(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    let push = |piece: &str, out: &mut Vec<(String, String)>| {
        let piece = piece.trim();
        if piece.is_empty() {
            return;
        }
        match expr::split_assignment(piece) {
            Some((keyword, value)) => out.push((keyword, value)),
            None => out.push((String::new(), piece.to_string())),
        }
    };

    for (index, character) in text.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                push(&text[start..index], &mut out);
                start = index + 1;
            }
            _ => {}
        }
    }
    push(&text[start..], &mut out);
    out
}
