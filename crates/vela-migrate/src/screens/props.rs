//! Props, actions and the values they carry (`SCREENS.md §4.2`, §7).
//!
//! This is where the milestone's two central rules are applied: a prop Vela has no counterpart for is
//! *reported* rather than approximated — placement above all, because a file that kept the pixels
//! would look migrated and draw in the wrong place — and an action is a name in Vela's registry
//! rather than a Python object, so the migrator lowers the name and refuses the ones that have none.

use super::words::*;
use super::*;

/// One prop, as a line Vela writes — or nothing, with the entry saying why not.
///
/// This is the whole decision about a prop, asked once: a widget's own line and a prop written under
/// it reach the same answer, because the two spellings are one question (`SCREENS.md §4.2`).
pub(super) fn prop_line(name: &str, written: &str, ctx: &mut Ctx) -> Option<String> {
    if let Some((_, renamed)) = PROPS.iter().find(|(prop, _)| *prop == name) {
        return ctx
            .resolved(written)
            .map(|value| format!("{renamed} {value}"));
    }
    if name == "action" {
        return action(written, &mut ctx.gaps).map(|call| format!("action {call}"));
    }
    if name == "at" {
        // `at <anchor>` is read as an anchor until transforms are a language (`§4.2`); a *name* of
        // a transform is animation, which is M13's and is reported.
        return match anchor_named(written) {
            Some(anchor) => Some(format!("at {anchor}")),
            None => {
                ctx.gaps
                    .unknown
                    .push(format!("`at {written}`, a transform (animation)"));
                None
            }
        };
    }
    if name == "style" {
        // A style Vela cannot express — Ren'Py's are mostly a `properties` splat and placement —
        // is not declared in the migrated module, and naming it would be `E5007`. The look it
        // carried is the report's business, not a dangling reference's.
        return match ctx.style(written) {
            Some(style) => Some(format!("style {style}")),
            None => {
                ctx.gaps
                    .unknown
                    .push(format!("`style {}`", unquote(written.trim())));
                None
            }
        };
    }
    if PLACEMENT.contains(&name) {
        ctx.gaps.placement.push(name.to_string());
        return None;
    }
    if PAINTED.contains(&strip_state(name)) {
        return ctx.resolved(written).map(|value| format!("{name} {value}"));
    }
    // A prop Vela takes, whose value is a call: `value Preference("text speed")` is a value a bar
    // holds, and the call inside it is an action under Vela's name.
    if WIDGET_PROPS.contains(&name) {
        if let Some(call) = called_value(written, &mut ctx.gaps) {
            return Some(format!("{name} {call}"));
        }
        return ctx.resolved(written).map(|value| format!("{name} {value}"));
    }
    ctx.gaps.unknown.push(name.to_string());
    None
}

/// One prop line inside a screen.
pub(super) fn lower_prop(head: &str, rest: &str, depth: usize, out: &mut String, ctx: &mut Ctx) {
    if let Some(line) = prop_line(head, rest, ctx) {
        emit(out, depth, &line);
    }
}

/// The props written on a widget's own line, each on a line of its own.
///
/// A matched `xalign`/`yalign` pair is one of Vela's nine anchors, and is written as one line; either
/// half alone is pixel placement, and is reported.
pub(super) fn props_of(props: &[(String, String)], depth: usize, ctx: &mut Ctx) -> String {
    let mut lines = String::new();
    let anchor = anchor_of(props);
    if let Some(anchor) = &anchor {
        push_prop(&mut lines, depth, anchor);
    }
    let paired = anchor.is_some();
    for (name, written) in props {
        if paired && matches!(name.as_str(), "xalign" | "yalign") {
            continue;
        }
        if let Some(line) = prop_line(name, written, ctx) {
            push_prop(&mut lines, depth, &line);
        }
    }
    lines
}

/// One prop line, indented and newline-terminated.
///
/// The newline is not cosmetic: a prop's text is followed by the widget's own children, so a line
/// without one glues the next line onto its end — `anchor bottom_leftstyle main_menu_frame:` — and
/// the parse error that produces names the *next* construct rather than the missing newline.
pub(super) fn push_prop(lines: &mut String, depth: usize, line: &str) {
    indent(depth, lines);
    lines.push_str(line);
    lines.push('\n');
}

/// The `xalign`/`yalign` lines in one block, when together they name one of Vela's nine anchors.
///
/// The pair is what makes a position a *point* rather than an axis: `xalign 0.5` alone says nothing
/// about where vertically, so Vela has no spelling for it and it is reported (`SCREENS.md §4.2`).
pub(super) fn anchor_lines(nodes: &[Node]) -> Option<(usize, usize, String)> {
    let mut horizontal: Option<(usize, String)> = None;
    let mut vertical: Option<(usize, String)> = None;
    for (index, node) in nodes.iter().enumerate() {
        let text = unmarked(node.text.trim());
        let (head, rest) = head_of(&text);
        let rest = rest.trim().to_string();
        match head.as_str() {
            "xalign" if horizontal.is_none() => horizontal = Some((index, rest)),
            "yalign" if vertical.is_none() => vertical = Some((index, rest)),
            _ => {}
        }
    }
    let (horizontal_at, horizontal) = horizontal?;
    let (vertical_at, vertical) = vertical?;
    let anchor = anchor_name(side(&horizontal, 0)?, side(&vertical, 1)?);
    Some((horizontal_at, vertical_at, format!("anchor {anchor}")))
}

/// The named position a matched `xalign`/`yalign` pair denotes, if the line has one.
pub(super) fn anchor_of(props: &[(String, String)]) -> Option<String> {
    let horizontal = props
        .iter()
        .find(|(name, _)| name == "xalign")
        .and_then(|(_, value)| side(value, 0));
    let vertical = props
        .iter()
        .find(|(name, _)| name == "yalign")
        .and_then(|(_, value)| side(value, 1));
    match (horizontal, vertical) {
        (Some(horizontal), Some(vertical)) => {
            Some(format!("anchor {}", anchor_name(horizontal, vertical)))
        }
        _ => None,
    }
}

/// A written `at` value as one of Vela's nine anchors, when it is one (`SCREENS.md §4.2`).
pub(super) fn anchor_named(written: &str) -> Option<String> {
    const ANCHORS: &[&str] = &[
        "top_left",
        "top",
        "top_right",
        "left",
        "center",
        "right",
        "bottom_left",
        "bottom",
        "bottom_right",
    ];
    let name = unquote(written.trim().trim_end_matches(':'));
    ANCHORS.contains(&name.as_str()).then_some(name)
}

/// A fraction as one of three sides, on an axis.
pub(super) fn side(value: &str, axis: usize) -> Option<&'static str> {
    const SIDES: [[&str; 3]; 2] = [["left", "center", "right"], ["top", "center", "bottom"]];
    match value.trim() {
        "0.0" | "0" => Some(SIDES[axis][0]),
        "0.5" => Some(SIDES[axis][1]),
        "1.0" | "1" => Some(SIDES[axis][2]),
        _ => None,
    }
}

/// The anchor name for a horizontal and a vertical side.
pub(super) fn anchor_name(horizontal: &str, vertical: &str) -> String {
    match (horizontal, vertical) {
        ("center", "center") => "center".to_string(),
        ("center", vertical) => vertical.to_string(),
        (horizontal, "center") => horizontal.to_string(),
        (horizontal, vertical) => match (horizontal, vertical) {
            ("left", "top") => "top_left".to_string(),
            ("right", "top") => "top_right".to_string(),
            ("left", "bottom") => "bottom_left".to_string(),
            ("right", "bottom") => "bottom_right".to_string(),
            _ => format!("{horizontal}_{vertical}"),
        },
    }
}

/// `timer 3.25 action Hide('notify')`, in Vela's shape.
pub(super) fn timer(rest: &str, ctx: &mut Ctx) -> String {
    match rest.split_once(" action ") {
        Some((seconds, call)) => match action(call, &mut ctx.gaps) {
            Some(call) => format!("timer {} action {}", value(seconds), call),
            None => format!("timer {}", value(seconds)),
        },
        None => format!("timer {}", value(rest)),
    }
}

/// A `use`'s screen name and its arguments, as one line's worth.
///
/// The name is what stands before the `(` — not the first word, which for `game_menu(_("About"),
/// scroll="viewport")` is `game_menu(_("About"),`: a call's arguments are part of the *line*, and
/// reading the name off a whitespace split loses the first argument to the name and repeats it.
pub(super) fn use_line(rest: &str, ctx: &mut Ctx) -> String {
    let rest = rest.trim().trim_end_matches(':').trim_end();
    let (name, tail) = match rest.find('(') {
        Some(at) => (rest[..at].trim(), &rest[at..]),
        None => (rest, ""),
    };
    format!("{name}{}", arguments(tail, ctx))
}

/// A `use`'s arguments, with the ones Vela cannot read dropped and reported.
pub(super) fn arguments(tail: &str, ctx: &mut Ctx) -> String {
    let Some(inside) = tail.trim().strip_prefix('(') else {
        return String::new();
    };
    let inside = inside.trim_end();
    let inside = inside.strip_suffix(')').unwrap_or(inside);
    let mut kept = Vec::new();
    for argument in split_top_level(inside) {
        let argument = argument.trim();
        if argument.is_empty() {
            continue;
        }
        if argument.contains(" if ") || argument.contains(" else ") {
            // A Python conditional expression, which Vela's grammar has no place for.
            ctx.gaps
                .unknown
                .push(format!("the argument `{}`", short(argument)));
            continue;
        }
        let (name, written) = match argument.split_once('=') {
            Some((name, written)) => (Some(name.trim()), written.trim()),
            None => (None, argument),
        };
        let Some(value) = ctx.resolved(&unmarked(written)) else {
            continue;
        };
        match name {
            Some(name) => kept.push(format!("{name}={value}")),
            None => kept.push(value),
        }
    }
    if kept.is_empty() {
        String::new()
    } else {
        format!("({})", kept.join(", "))
    }
}

/// Splits on the commas that are not inside a call, a string, or a bracket.
pub(super) fn split_top_level(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut start = 0usize;
    for (at, ch) in text.char_indices() {
        match (quote, ch) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), _) => {}
            (None, '"') | (None, '\'') => quote = Some(ch),
            (None, '(') | (None, '[') | (None, '{') => depth += 1,
            (None, ')') | (None, ']') | (None, '}') => depth = depth.saturating_sub(1),
            (None, ',') if depth == 0 => {
                parts.push(text[start..at].to_string());
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].to_string());
    parts
}

/// An action, under Vela's name for it — or nothing, with the entry naming what it was.
pub(super) fn action(written: &str, gaps: &mut Gaps) -> Option<String> {
    let marked = unmarked(written.trim());
    let (name, rest) = call_head(&marked);
    if name.is_empty() {
        return None;
    }
    // A bare name is an action the screen was *given* (`SCREENS.md §7`), not a call.
    if !rest.starts_with('(') {
        return Some(name);
    }
    let lowered = match ACTIONS.iter().find(|(old, _)| *old == name) {
        Some((_, new)) => (*new).to_string(),
        None => snake_case(&name),
    };
    if !KNOWN_ACTIONS.contains(&lowered.as_str()) {
        gaps.unknown
            .push(format!("the `{name}` action, which Vela has no name for"));
        return None;
    }
    Some(format!(
        "{lowered}{}",
        literals(rest, gaps)? // a `?` here drops the action, which is what an argument Vela cannot
                              // hold means: the entry has already named it
    ))
}

/// A call's name and the `(…)` after it, however the two are spaced.
///
/// Splitting on whitespace is the mistake this exists to avoid: `Return()` has no space in it, so a
/// whitespace split returns the whole call as the name and the name is then never looked up — a
/// `Return()` that Vela calls `close_screen()` came out as `Return()`, and the checker's "no action
/// by this name" was the first thing to say so.
pub(super) fn call_head(text: &str) -> (String, &str) {
    match text.find('(') {
        Some(at) => (text[..at].trim().to_string(), text[at..].trim_start()),
        None => (text.trim().to_string(), ""),
    }
}

/// A call's arguments as Vela writes them, or `None` when one of them is not a value a migrated
/// screen could hold.
///
/// Two differences from Ren'Py, and both are reported rather than papered over. A Vela action takes
/// *positional* arguments only — `E5013` counts them — so `Quit(confirm=False)` keeps the action and
/// loses the word. And an argument that reads a namespace Vela has no value for (`config.`,
/// `renpy.`, `persistent.`) takes the whole action with it: an action that plays an asset nobody
/// named is not an action (`SCREENS.md §7`).
pub(super) fn literals(rest: &str, gaps: &mut Gaps) -> Option<String> {
    if !rest.starts_with('(') {
        return Some(rest.to_string());
    }
    let ends = rest.trim_end().ends_with(')');
    let inside = &rest[1..rest.len() - usize::from(ends)];
    let mut kept = Vec::new();
    for argument in split_top_level(inside) {
        let argument = argument.trim();
        if argument.is_empty() {
            continue;
        }
        if let Some((name, _)) = argument.split_once('=') {
            gaps.unknown
                .push(format!("the `{name}=` argument of an action"));
            continue;
        }
        if reads_engine(argument) {
            gaps.unknown
                .push(format!("the action's argument `{}`", short(argument)));
            return None;
        }
        kept.push(value(&unmarked(argument)));
    }
    Some(format!("({})", kept.join(", ")))
}

/// Whether an expression reads a namespace that belongs to the engine rather than to the project.
pub(super) fn reads_engine(argument: &str) -> bool {
    ["config.", "renpy.", "persistent.", "store."]
        .iter()
        .any(|namespace| argument.contains(namespace))
}

/// A value that is a call, lowered under Vela's name for it — `Preference("sound volume")` is
/// `preference("sound volume")`, whether it is an `action` or a `value` a bar holds.
pub(super) fn called_value(written: &str, gaps: &mut Gaps) -> Option<String> {
    let (name, rest) = call_head(written.trim());
    if !is_name(&name) || !rest.starts_with('(') {
        return None;
    }
    action(written, gaps)
}

/// `ShowMenu` → `show_menu`: Vela's registry spells an action in lower snake case.
pub(super) fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
