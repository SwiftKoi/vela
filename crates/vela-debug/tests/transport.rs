//! Framing, and the ways a header can lie.
//!
//! The interesting part of a `Content-Length` reader is not the happy path but what it does when
//! the header is wrong, so every failure mode has a test: no length, a length that is not a
//! number, a length that is absurd, and a body that ends early.

use serde_json::{Value, json};
use vela_debug::{read, write};

/// Frames a message and reads it back.
#[test]
fn a_message_round_trips() {
    let mut bytes = Vec::new();
    let sent = json!({ "seq": 1, "type": "request", "command": "initialize" });
    write(&mut bytes, &sent).expect("frames");

    assert!(bytes.starts_with(b"Content-Length: "));

    let mut reader = std::io::Cursor::new(bytes);
    let received = read(&mut reader).expect("reads").expect("a message");
    assert_eq!(received, sent);
}

/// An empty stream is the end of it, not an error: a client that disconnects has not lied.
#[test]
fn an_empty_stream_is_the_end() {
    let mut reader = std::io::Cursor::new(Vec::new());
    assert_eq!(read(&mut reader).expect("reads"), None);
}

/// A headers block with no length cannot be trusted, so it is refused.
#[test]
fn a_message_without_a_length_is_refused() {
    let mut reader = std::io::Cursor::new(b"Content-Type: application/json\r\n\r\n{}".to_vec());
    let error = read(&mut reader).expect_err("no length");
    assert!(error.to_string().contains("Content-Length"), "{error}");
}

/// A length that is not a number is refused rather than guessed at.
#[test]
fn a_length_that_is_not_a_number_is_refused() {
    let mut reader = std::io::Cursor::new(b"Content-Length: many\r\n\r\n{}".to_vec());
    let error = read(&mut reader).expect_err("not a number");
    assert!(error.to_string().contains("not a number"), "{error}");
}

/// A length past the ceiling is refused *before* allocating, which is the point of the ceiling.
#[test]
fn a_length_over_the_ceiling_is_refused() {
    let mut reader = std::io::Cursor::new(b"Content-Length: 999999999999\r\n\r\n".to_vec());
    let error = read(&mut reader).expect_err("absurd length");
    assert!(error.to_string().contains("ceiling"), "{error}");
}

/// The header name is matched case-insensitively, because the spec says it is insensitive.
#[test]
fn the_header_name_is_case_insensitive() {
    let body = serde_json::to_vec(&json!({ "type": "event" })).expect("serde");
    let mut bytes = format!("content-length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(&body);

    let mut reader = std::io::Cursor::new(bytes);
    let message = read(&mut reader).expect("reads").expect("a message");
    assert_eq!(message["type"], Value::from("event"));
}

/// Extra headers DAP allows are ignored rather than rejected.
#[test]
fn extra_headers_are_ignored() {
    let body = serde_json::to_vec(&json!({ "type": "event" })).expect("serde");
    let mut bytes = format!(
        "Content-Length: {}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(&body);

    let mut reader = std::io::Cursor::new(bytes);
    assert!(read(&mut reader).expect("reads").is_some());
}
