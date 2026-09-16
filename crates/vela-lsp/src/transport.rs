//! Frames: the bytes around a JSON-RPC message.
//!
//! The protocol frames each message with headers — `Content-Length`, and nothing else that matters —
//! so reading one is: find the blank line, take the length, read exactly that many bytes. Twenty lines
//! of code rather than a dependency, and the interesting part is the failure modes, which is why they
//! are tested: a headers block with no length, a length that is not a number, a length that is absurd,
//! and a body that ends early because the client was killed mid-message.

use std::io::{self, BufRead, Write};

use serde_json::Value;

/// The most one message may claim.
///
/// A header is bytes from a stream, and a stream can say anything: without a ceiling, one malformed
/// `Content-Length` is an allocation of that size, which is a way to kill an editor by typing at it.
const MAX_BODY: usize = 64 * 1024 * 1024;

/// Reads one message, or `None` when the stream has ended.
///
/// # Errors
///
/// Fails on a malformed header, on a length above [`MAX_BODY`], on a body that ends early, and on a
/// body that is not JSON. Every one of those is the client's fault and none of them is recoverable:
/// the stream is no longer framed, so there is nothing to resynchronise to.
pub fn read(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let Some(length) = headers(reader)? else {
        return Ok(None);
    };
    if length > MAX_BODY {
        return Err(invalid(format!(
            "a message claiming {length} bytes, over the {MAX_BODY}-byte ceiling"
        )));
    }

    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;

    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| invalid(format!("a message whose body is not JSON: {error}")))
}

/// Writes one message, framed, and flushes it.
///
/// Flushed per message rather than buffered: a client is waiting for the answer, and the next thing
/// this process does is block on its next request.
pub fn write(writer: &mut dyn Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message).map_err(|error| invalid(error.to_string()))?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}

/// The `Content-Length` of the next message's headers, or `None` at the end of the stream.
fn headers(reader: &mut impl BufRead) -> io::Result<Option<usize>> {
    let mut length: Option<usize> = None;
    let mut line = String::new();

    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }

        // Case-insensitively: the header names are ASCII and the spec says they are insensitive, and a
        // reader that only accepts one spelling is a reader that breaks on one client.
        let (name, value) = header.split_once(':').unwrap_or((header, ""));
        if name.eq_ignore_ascii_case("content-length") {
            length = Some(value.trim().parse().map_err(|_| {
                invalid(format!(
                    "`Content-Length: {}` is not a number",
                    value.trim()
                ))
            })?);
        }
    }

    match length {
        Some(length) => Ok(Some(length)),
        None => Err(invalid("a message with no `Content-Length`")),
    }
}

/// An error that means the stream is no longer readable as messages.
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
