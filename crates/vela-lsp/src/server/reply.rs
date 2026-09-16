//! The shape of a reply, and reading a value out of a request.

use std::io::Write;

use serde_json::{Value, json};

use super::state::Server;
use crate::transport;

impl Server {
    /// Answers a request.
    pub(super) fn respond(
        &self,
        output: &mut dyn Write,
        id: Option<Value>,
        result: Value,
    ) -> std::io::Result<()> {
        let Some(id) = id else {
            return Ok(());
        };
        transport::write(
            output,
            &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        )
    }

    /// Answers a request with an error.
    pub(super) fn respond_error(
        &self,
        output: &mut dyn Write,
        id: Option<Value>,
        code: i32,
        message: &str,
    ) -> std::io::Result<()> {
        let Some(id) = id else {
            return Ok(());
        };
        transport::write(
            output,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": code, "message": message },
            }),
        )
    }
}

/// A string at a path within a message, or the empty string.
pub(super) fn string_at(message: &Value, path: &[&str]) -> String {
    value_at(message, path).as_str().unwrap_or("").to_string()
}

/// A number at a path within a message, or zero.
pub(super) fn number_at(message: &Value, path: &[&str]) -> u32 {
    value_at(message, path)
        .as_u64()
        .and_then(|number| u32::try_from(number).ok())
        .unwrap_or(0)
}

/// The value at a path within a message.
fn value_at<'a>(message: &'a Value, path: &[&str]) -> &'a Value {
    let mut value = message;
    for step in path {
        value = &value[*step];
    }
    value
}
