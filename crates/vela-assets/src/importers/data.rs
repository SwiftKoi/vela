//! Data assets: JSON, TOML, and CSV, normalized to canonical JSON.
//!
//! `BUILD_AND_ASSETS.md §3.1` calls the output a "typed value blob, validated against a
//! declared schema". The blob this build writes is canonical JSON — compact, keys sorted by
//! `serde_json`'s ordered map — for three reasons: the inputs are a tree of the same values, the
//! runtime then decodes one format instead of four, and a normalizing importer is what makes the
//! digest an identity. Two files that differ only in formatting import to the same bytes, so
//! they are one artifact rather than two.
//!
//! CSV is the odd one out: a table, not a tree, so it becomes an array of objects with the
//! header row naming the keys.

use serde_json::Value;

use crate::error::AssetError;
use crate::importers::registry::{ImportRequest, Importer, Output};

/// The data importer.
pub struct Data;

impl Importer for Data {
    fn kind(&self) -> &'static str {
        "data"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["json", "toml", "csv"]
    }

    /// Always `false`, and that is the honest answer.
    ///
    /// None of these formats has magic bytes — JSON may open with any value, TOML with a key, a
    /// comment, or nothing at all, and CSV is text — so nothing here can be *recognized* and
    /// only the extension can decide. Selection is magic-bytes-first, so this importer is chosen
    /// second, which is where it belongs.
    fn probe(&self, _bytes: &[u8]) -> bool {
        false
    }

    fn import(&self, request: &ImportRequest<'_>) -> Result<Vec<Output>, AssetError> {
        let path = std::path::PathBuf::from(request.source);
        let refuse = |message: String| AssetError::Import {
            path: path.clone(),
            kind: "data",
            message,
        };

        let text = std::str::from_utf8(request.bytes)
            .map_err(|error| refuse(format!("not UTF-8: {error}")))?;

        let value = match request.extension().as_str() {
            "json" => {
                serde_json::from_str(text).map_err(|error| refuse(format!("not JSON: {error}")))?
            }
            "toml" => {
                let parsed: toml::Value =
                    toml::from_str(text).map_err(|error| refuse(format!("not TOML: {error}")))?;
                from_toml(parsed).map_err(&refuse)?
            }
            "csv" => from_csv(text).map_err(&refuse)?,
            other => return Err(refuse(format!("`{other}` is not a data format"))),
        };

        let bytes = serde_json::to_vec(&value).map_err(|error| refuse(error.to_string()))?;
        Ok(vec![Output {
            path: format!("{}{}.json", request.directory(), request.stem()),
            kind: "data",
            bytes,
        }])
    }
}

/// A TOML value as JSON.
///
/// Written out rather than going through `serde_json::to_value`, because TOML has a datetime
/// and JSON does not: the derived path encodes it as a private struct that only `toml` can
/// read back, which is worse than a string and much worse than saying so. A datetime becomes
/// its RFC 3339 text.
///
/// # Errors
///
/// Fails on a float JSON cannot hold. `nan` and `inf` are expressible in TOML and not in JSON,
/// and writing `null` instead would turn a loud failure into a wrong value.
fn from_toml(value: toml::Value) -> Result<Value, String> {
    Ok(match value {
        toml::Value::String(text) => Value::String(text),
        toml::Value::Integer(number) => Value::Number(number.into()),
        toml::Value::Float(number) => serde_json::Number::from_f64(number)
            .map(Value::Number)
            .ok_or_else(|| format!("`{number}` is a float JSON cannot hold"))?,
        toml::Value::Boolean(flag) => Value::Bool(flag),
        toml::Value::Datetime(when) => Value::String(when.to_string()),
        toml::Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(from_toml(item)?);
            }
            Value::Array(out)
        }
        toml::Value::Table(table) => {
            let mut out = serde_json::Map::new();
            for (key, item) in table {
                out.insert(key, from_toml(item)?);
            }
            Value::Object(out)
        }
    })
}

/// A CSV table as an array of objects, the first record naming the columns.
///
/// **Every cell is a string, and that is deliberate.** A CSV file does not say what its columns
/// mean, and a column of digits is as likely to be a postcode or an account number as a
/// quantity: reading `007` as `7` is a silent corruption no later stage can detect. Types come
/// from a declared schema, which is the other half of §3.1's "typed value blob" and does not
/// exist yet.
///
/// # Errors
///
/// Fails on a row that does not match the header's width, or a quoted field that is never
/// closed. Both are cases where carrying on would produce a table the author did not write.
fn from_csv(text: &str) -> Result<Value, String> {
    let records = parse_csv(text)?;
    let Some(header) = records.first() else {
        return Ok(Value::Array(Vec::new()));
    };
    if header.is_empty() {
        return Err("the header row is empty".to_string());
    }

    let mut rows = Vec::with_capacity(records.len().saturating_sub(1));
    for (index, record) in records.iter().enumerate().skip(1) {
        if record.len() != header.len() {
            return Err(format!(
                "row {} has {} field(s) and the header has {}",
                index + 1,
                record.len(),
                header.len()
            ));
        }
        let mut object = serde_json::Map::new();
        for (name, cell) in header.iter().zip(record) {
            object.insert(name.clone(), Value::String(cell.clone()));
        }
        rows.push(Value::Object(object));
    }
    Ok(Value::Array(rows))
}

/// Splits CSV text into records of fields (RFC 4180).
///
/// A field is quoted or not; inside quotes a comma and a newline are ordinary characters and
/// `""` is one quote. Both `\r\n` and `\n` end a record, because a file that was opened and
/// saved once on Windows is still a CSV file.
fn parse_csv(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();

    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            '"' if quoted => quoted = false,
            '"' if field.is_empty() => quoted = true,
            ',' if !quoted => record.push(std::mem::take(&mut field)),
            '\r' | '\n' if !quoted => {
                if character == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            other => field.push(other),
        }
    }

    if quoted {
        return Err("a quoted field is never closed".to_string());
    }
    // A file ending without a newline still has a last record; one ending *with* a newline has
    // already had it pushed, and this must not add an empty row after it.
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::{from_csv, parse_csv};
    use serde_json::json;

    /// A table becomes an array of objects, with every cell a string.
    #[test]
    fn a_table_becomes_objects() {
        let parsed = from_csv("name,hp\nEileen,3\n\"Forest, deep\",7\n").expect("parses");
        assert_eq!(
            parsed,
            json!([
                {"name": "Eileen", "hp": "3"},
                {"name": "Forest, deep", "hp": "7"},
            ])
        );
    }

    /// A quoted field holds a comma, a newline, and a doubled quote.
    #[test]
    fn quotes_are_literal() {
        let rows = parse_csv("a,b\n\"x,y\",\"line\nbreak\"\n\"say \"\"hi\"\"\",z").expect("parses");
        assert_eq!(
            rows,
            vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["x,y".to_string(), "line\nbreak".to_string()],
                vec!["say \"hi\"".to_string(), "z".to_string()],
            ]
        );
    }

    /// A row that does not match the header is refused rather than padded.
    #[test]
    fn a_ragged_row_is_refused() {
        let error = from_csv("a,b\n1\n").expect_err("ragged");
        assert!(error.contains("row 2"), "{error}");
    }

    /// A quote that is never closed is refused rather than swallowing the rest of the file.
    #[test]
    fn an_unclosed_quote_is_refused() {
        let error = from_csv("a\n\"unfinished\n").expect_err("unclosed");
        assert!(error.contains("never closed"), "{error}");
    }

    /// A header with no rows is an empty table, and an empty file is too.
    #[test]
    fn an_empty_table_is_empty() {
        assert_eq!(from_csv("a,b\n").expect("parses"), json!([]));
        assert_eq!(from_csv("").expect("parses"), json!([]));
    }
}
