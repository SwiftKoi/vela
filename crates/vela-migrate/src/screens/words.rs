//! The words a line is made of: values, tokens, names, indentation.
//!
//! Nothing here knows what a widget is. These are the functions that read one Ren'Py line and say
//! what its pieces are — which matters because Ren'Py's screen lines are not tidily separated: a
//! prop is a *name* and a value, a leaf's content is a value with no name, and a call is one token
//! however it is spaced.

use super::tables::*;
use super::*;

/// A value as Vela writes it.
///
/// A `gui.<name>` reference is *not* resolved here: it needs the theme, which a value on its own does
/// not have, so the callers that can write one — a prop, a style setting, a `use`'s argument — go
/// through [`Ctx::resolved`], and a reference that reaches this function is one they decided to keep
/// as written.
pub(super) fn value(written: &str) -> String {
    let trimmed = written.trim().trim_end_matches(':').trim();
    // A word at a time, not the whole value: `who is not None` is a value with a keyword in the
    // middle, and the whole-value match this replaces left the capital `None` in a condition, where
    // it named nothing and read as false — which *happened* to draw the right arm.
    let trimmed = literals(trimmed);
    if trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() > 1 {
        // Ren'Py accepts `'notify'`; Vela's strings are double-quoted.
        return format!("\"{}\"", &trimmed[1..trimmed.len() - 1]);
    }
    match face(&trimmed) {
        Some(face) => format!("\"{face}\""),
        // `renpy.variant("pc")` is the *question* the host answers (`SCREENS.md §2.6`), and Vela
        // spells it without the namespace: `variant("pc")`. Left as written it is a call nothing
        // answers, so the arm it guards draws the wrong way in silence.
        None => trimmed.replace("renpy.variant(", "variant("),
    }
}

/// Ren'Py's `True`/`False`/`None` as Vela writes them, wherever they appear.
fn literals(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(['T', 'F', 'N']) {
        let (before, from) = rest.split_at(at);
        out.push_str(before);
        let word = ["True", "False", "None"]
            .iter()
            .find(|word| from.starts_with(*word))
            .filter(|word| {
                // A word, not the head of a longer name: `Nothing` is a name.
                !from[word.len()..]
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
            });
        match word.copied() {
            Some("True") => {
                out.push_str("true");
                rest = &from[4..];
            }
            Some("False") => {
                out.push_str("false");
                rest = &from[5..];
            }
            Some(_) => {
                out.push_str("none");
                rest = &from[4..];
            }
            None => {
                out.push(from.chars().next().unwrap_or_default());
                rest = &from[from.chars().next().map_or(1, char::len_utf8)..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A font file as the face name a project registers it under: `"DejaVuSans.ttf"` is `"DejaVuSans"`.
///
/// `SCREENS.md §5`'s font is a *name* the engine has, not an asset path — the rule `gui.rs` already
/// follows for the theme's faces — and a path in a `font` setting names a font nothing carries.
pub(super) fn face(written: &str) -> Option<String> {
    let quoted = written
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))?;
    let lower = quoted.to_ascii_lowercase();
    [".ttf", ".otf", ".ttc", ".woff", ".woff2"]
        .iter()
        .find(|extension| lower.ends_with(*extension))?;
    let file = quoted.rsplit('/').next().unwrap_or(quoted);
    Some(match file.rsplit_once('.') {
        Some((stem, _)) => stem.to_string(),
        None => file.to_string(),
    })
}

/// Ren'Py's translation marker, unwrapped wherever it appears: `_("About")` is `"About"`.
pub(super) fn unmarked(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("_(") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        match after.find(')') {
            Some(end) => {
                out.push_str(&after[..end]);
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[at..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The line without Ren'Py's *translation ids*: `{#auto_page}A` is the string `"A"` and an
/// identifier a catalogue keys on, and nothing draws the identifier.
pub(super) fn markers(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("{#") {
        out.push_str(&rest[..at]);
        match rest[at..].find('}') {
            Some(end) => rest = &rest[at + end + 1..],
            None => {
                out.push_str(&rest[at..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A call in a condition or an iterable that nothing answers, if there is one.
///
/// A screen decides from what it has — a parameter, a variable, a loop element, a literal, or a
/// question the host answers (`SCREENS.md §2.2`, §2.6) — so `GamepadExists()` is the input map's
/// (`§11`) and `range(n)` is Python's. A question is decidable and passes; everything else is named.
pub(super) fn ungrounded(condition: &str) -> Option<String> {
    let mut rest = condition;
    while let Some(at) = rest.find('(') {
        // A call is a name *touching* its `(`: `a or (b)` has a word before the paren that is an
        // operator, and reading that as a call name reports `or(…)` — which is not a call at all.
        let touching = rest[..at]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        let head = rest[..at].split_whitespace().last().unwrap_or("");
        rest = &rest[at + 1..];
        if touching && is_name(head) && head != "variant" {
            return Some(head.to_string());
        }
    }
    None
}

/// The leading positional a widget line carries, and the props after it.
///
/// Which widgets take one is the widget's schema, and it is the same question the parser leaves to
/// the checker: `text what id "what"` and `text name style = body` are indistinguishable by shape,
/// because `what` and `name` are both bare names — but only a widget that *has* content can be
/// holding one. Reading `what` as a prop's name instead would make the text draw nothing, and
/// reading `textbutton "Yes"`'s caption as a prop name reports a button that had one.
pub(super) fn positional(
    takes_content: bool,
    text: &str,
) -> (Option<String>, Vec<(String, String)>) {
    let trimmed = text.trim();
    if !takes_content {
        return (None, pairs(trimmed));
    }
    let Some((head, rest)) = take_value(trimmed) else {
        return (None, Vec::new());
    };
    (Some(head), pairs(rest))
}

/// Whether a token is a bare name, which is what a prop's own name has to be.
pub(super) fn is_name(token: &str) -> bool {
    !token.is_empty()
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
}

/// The name/value pairs on a line, in order.
pub(super) fn pairs(text: &str) -> Vec<(String, String)> {
    let mut tokens = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        match take_value(rest) {
            Some((token, after)) => {
                tokens.push(token);
                rest = after.trim_start();
            }
            None => break,
        }
    }
    tokens
        .chunks(2)
        .filter_map(|pair| match pair {
            [name, value] => Some((name.clone(), value.clone())),
            [name] => Some((name.clone(), String::new())),
            _ => None,
        })
        .collect()
}

/// One token and the text after it: to the first top-level space, quotes and brackets balanced.
pub(super) fn take_value(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    if text.is_empty() {
        return None;
    }
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    for (at, ch) in text.char_indices() {
        match (quote, ch) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), _) => {}
            (None, '"') | (None, '\'') => quote = Some(ch),
            (None, '(') | (None, '[') | (None, '{') => depth += 1,
            (None, ')') | (None, ']') | (None, '}') => depth = depth.saturating_sub(1),
            (None, c) if c.is_whitespace() && depth == 0 => {
                return Some((text[..at].to_string(), &text[at..]));
            }
            _ => {}
        }
    }
    Some((text.to_string(), ""))
}

/// Whether a value is a Python call with keyword arguments, which Vela's expression grammar has no
/// place for: `FilePageNameInputValue(pattern=…)` is a host displayable (`SCREENS.md §7`).
pub(super) fn python_call(written: &str) -> bool {
    let value = written.split_once('=').map_or(written, |(_, value)| value);
    match (value.find('('), value.find('=')) {
        (Some(paren), Some(equals)) => equals > paren,
        _ => false,
    }
}

/// A name as a bare one: Vela's `style_prefix` takes a name, not a string.
pub(super) fn unquote(text: &str) -> String {
    text.trim().trim_matches('"').to_string()
}

/// A widget's Vela name, if the word is one.
pub(super) fn widget_name(word: String) -> Option<String> {
    if let Some((_, renamed)) = WIDGETS.iter().find(|(name, _)| *name == word) {
        return Some((*renamed).to_string());
    }
    SAME.contains(&word.as_str()).then_some(word)
}

/// The first word, and everything after it.
pub(super) fn split_head(text: &str) -> (String, &str) {
    let text = text.trim();
    match text.find(char::is_whitespace) {
        Some(at) => (text[..at].to_string(), text[at..].trim_start()),
        None => (text.to_string(), ""),
    }
}

/// The first word without a trailing colon, and everything after it.
///
/// A header with no arguments is one word: `vbox:` splits into `vbox:` and nothing, so a head read by
/// whitespace alone never matches the widget table — and the whole block under it is then reported as
/// a construct with no counterpart, which is how a screen with a `vbox:` in it came out empty.
pub(super) fn head_of(text: &str) -> (String, &str) {
    let (head, rest) = split_head(text);
    (head.trim_end_matches(':').to_string(), rest)
}

/// The first whitespace-separated word.
pub(super) fn first_word(text: &str) -> String {
    split_head(text).0
}

/// A string short enough to quote in a report entry.
pub(super) fn short(text: &str) -> String {
    if text.chars().count() > 40 {
        let cut: String = text.chars().take(37).collect();
        format!("{cut}...")
    } else {
        text.to_string()
    }
}

/// The distinct names in a list, in order, so a report entry reads the same twice.
pub(super) fn distinct(names: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for name in names {
        if !seen.contains(name) {
            seen.push(name.clone());
        }
    }
    seen
}

/// Writes one line at a depth.
pub(super) fn emit(out: &mut String, depth: usize, line: &str) {
    indent(depth, out);
    out.push_str(line);
    out.push('\n');
}

/// Writes the indentation for a depth.
pub(super) fn indent(depth: usize, out: &mut String) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

/// Whether a `add`'s caption is a picture Vela can draw: a path, or a dotted name from data.
///
/// `gui.main_menu_background` is a path in the skin — the same class of thing the gui pass reports —
/// and a call is a displayable the host owns, so neither is an image `vela check` could resolve.
pub(super) fn picture(caption: &str) -> bool {
    !caption.starts_with("gui.") && (is_name(caption) || caption.starts_with('"'))
}

/// A `text`'s content as Vela writes it, or `None` when Vela has no value for it.
///
/// A screen draws what it was given and what it can compute; a *call* is a host function
/// (`FileTime(slot)`, `SideImage()`), and an interpolated string is Ren'Py's — `[config.name!t]` is
/// Vela's `[name]` with a namespace and a conversion flag Vela does not have, and `{a=…}` is a text
/// tag its presenter does not know (`LANGUAGE.md §5.5`). Both are reported rather than written, and
/// the widget goes with them: `text` with nothing to draw draws nothing.
pub(super) fn text_content(caption: &str) -> Option<String> {
    let trimmed = caption.trim();
    if call_head(trimmed).1.starts_with('(') {
        return None;
    }
    if !tags_are_velas(caption) || !interpolations_are_velas(caption) {
        return None;
    }
    Some(value(trimmed))
}

/// Whether every text tag in a string is one Vela's presenter knows.
///
/// `{b}`/`{i}` are Vela's (`LANGUAGE.md §5.5`); `{a=…}` is a hyperlink, which is Ren'Py's own, and
/// `{#id}` is a translation marker, which the walk strips before this is asked.
fn tags_are_velas(text: &str) -> bool {
    let mut rest = text;
    while let Some(at) = rest.find('{') {
        let after = &rest[at + 1..];
        let Some(end) = after.find('}') else {
            return false;
        };
        let tag = after[..end].trim();
        if !["b", "i", "/b", "/i", "/", "u", "/u"].contains(&tag) {
            return false;
        }
        rest = &after[end + 1..];
    }
    true
}

/// Whether every `[expr]` in a string is one Vela can evaluate.
///
/// `[page]` is a name a loop bound, which is exactly what Vela's interpolation takes; `[config.name!t]`
/// is a namespace Vela has no value for and a conversion flag its grammar has no place for.
fn interpolations_are_velas(text: &str) -> bool {
    let mut rest = text;
    while let Some(at) = rest.find('[') {
        let after = &rest[at + 1..];
        let Some(end) = after.find(']') else {
            return false;
        };
        let inside = after[..end].trim();
        if !is_name(inside) || reads_engine(inside) {
            return false;
        }
        rest = &after[end + 1..];
    }
    true
}
