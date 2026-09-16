//! The Python subset that maps to a Vela expression.
//!
//! `TOOLING.md §8` asks for "simple Python expressions that map to Vela expressions", and the
//! honest reading of *simple* is a whitelist rather than a Python parser: this accepts the
//! operators and literals both languages spell the same way, translates the three keywords that
//! differ (`True`, `False`, `None`), and refuses everything else with a reason.
//!
//! The refusal is the important half. A Python expression that runs through untouched is not a
//! translation — it is a guess that happens to parse sometimes, and the ways it fails (a
//! comprehension, a format string, `len()`) are exactly the ways that produce a story which
//! checks clean and behaves differently.

/// Translates a Python expression, or says why it cannot be done.
///
/// # Errors
///
/// The message names what was found, so a report entry reads as a reason rather than a shrug.
pub fn expression(python: &str) -> Result<String, String> {
    let text = python.trim();
    if text.is_empty() {
        return Err("an empty expression".to_string());
    }

    let out = collapse(&scan(text)?);
    if out.is_empty() {
        return Err("an empty expression".to_string());
    }
    Ok(out)
}

/// Walks the expression, translating it one token at a time.
///
/// Every character is either whitespace, an operator both languages share, a literal, or a name;
/// anything else stops the walk with the character in the message.
fn scan(text: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut index = 0;
    let bytes = text.as_bytes();

    while index < bytes.len() {
        let rest = &text[index..];

        if rest.starts_with(char::is_whitespace) {
            out.push(' ');
            index += rest.chars().next().map_or(1, char::len_utf8);
            continue;
        }

        // Two-character operators first, so `<=` is not read as `<` then `=`.
        let pair = &text[index..text.len().min(index + 2)];
        if matches!(pair, "==" | "!=" | "<=" | ">=") {
            out.push_str(pair);
            index += 2;
            continue;
        }

        let character = rest.chars().next().unwrap_or(' ');
        if "+-*/%<>()".contains(character) {
            out.push(character);
            index += character.len_utf8();
            continue;
        }
        if character == ',' {
            out.push_str(", ");
            index += 1;
            continue;
        }
        if character == '"' {
            let (literal, length) = string_literal(rest)?;
            out.push_str(literal);
            index += length;
            continue;
        }
        if character == '\'' {
            return Err("a single-quoted string has no Vela equivalent".to_string());
        }

        if character.is_ascii_digit() {
            let token: String = rest
                .chars()
                .take_while(|character| character.is_ascii_alphanumeric() || *character == '.')
                .collect();
            out.push_str(&token);
            index += token.len();
            continue;
        }

        if character.is_alphabetic() || character == '_' {
            let token: String = rest
                .chars()
                .take_while(|character| character.is_alphanumeric() || *character == '_')
                .collect();
            out.push_str(keyword(&token)?);
            index += token.len();
            continue;
        }

        if character == '.' {
            out.push('.');
            index += 1;
            continue;
        }

        return Err(format!("`{character}` is not part of a Vela expression"));
    }

    Ok(out)
}

/// One identifier, translated or refused.
fn keyword(token: &str) -> Result<&str, String> {
    Ok(match token {
        "True" => "true",
        "False" => "false",
        "None" => "none",
        "and" | "or" | "not" => token,
        // Python-only vocabulary, named so the report says what it saw.
        "is" | "in" | "lambda" | "yield" | "await" | "assert" | "del" | "global" | "print" => {
            return Err(format!("`{token}` is Python, not Vela"));
        }
        _ => token,
    })
}

/// A double-quoted string, returned whole with its length in bytes.
fn string_literal(text: &str) -> Result<(&str, usize), String> {
    let bytes = text.as_bytes();
    let mut index = 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return Ok((&text[..=index], index + 1)),
            _ => index += 1,
        }
    }
    Err("a string that is not closed".to_string())
}

/// Collapses runs of spaces, so a translation does not inherit Python's spacing.
fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = false;
    for character in text.chars() {
        if character == ' ' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(character);
            last_space = false;
        }
    }
    out.trim().to_string()
}

/// Splits `name = value` on the first `=` that is not part of a comparison.
#[must_use]
pub fn split_assignment(text: &str) -> Option<(String, String)> {
    let bytes = text.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'=' {
            continue;
        }
        let previous = index.checked_sub(1).map(|at| bytes[at]);
        let next = bytes.get(index + 1);
        // `==`, `!=`, `<=`, `>=` are comparisons, not assignments.
        if next == Some(&b'=') || matches!(previous, Some(b'!') | Some(b'<') | Some(b'>')) {
            continue;
        }
        return Some((
            text[..index].trim().to_string(),
            text[index + 1..].trim().to_string(),
        ));
    }
    None
}

/// The string inside `_("...")`, which is Ren'Py's translation marker.
///
/// Vela has no translation catalogue yet (`M10` records `renpy translate` as the reference), so
/// the call is dropped and the string kept — which is the right direction to lose: the line
/// still reads correctly, and the report says a translatable string was found.
#[must_use]
pub fn without_translation_call(text: &str) -> Option<String> {
    let inner = text.trim().strip_prefix("_(")?.strip_suffix(')')?.trim();
    let (literal, length) = string_literal(inner).ok()?;
    (length == inner.len()).then(|| literal.to_string())
}

/// The Vela type of a literal `default`, so the declaration can be typed.
///
/// `None` when the value is not a literal, because `default` is where the save schema comes from
/// (`RUNTIME.md §5`) and a schema cannot be inferred from an expression.
#[must_use]
pub fn literal_type(value: &str) -> Option<&'static str> {
    let value = value.trim();
    match value {
        "True" | "False" => Some("bool"),
        _ if value.starts_with('"') => Some("str"),
        _ if value.parse::<i64>().is_ok() => Some("int"),
        _ if value.parse::<f64>().is_ok() => Some("float"),
        _ => None,
    }
}
