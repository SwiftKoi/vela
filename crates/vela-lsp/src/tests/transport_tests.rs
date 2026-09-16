//! The framing, including the ways it can be wrong.
//!
//! A transport's happy path is two lines of code; its failure modes are what decides whether a
//! misbehaving client corrupts the session or is refused. So each way a header can lie has a test, and
//! they all end the same way: an error, because a stream whose framing is broken cannot be
//! resynchronised.

use std::io::Cursor;

use serde_json::json;

use crate::transport;

#[test]
fn a_message_survives_a_round_trip() {
    let message = json!({ "jsonrpc": "2.0", "method": "initialize", "params": { "a": [1, 2] } });

    let mut bytes = Vec::new();
    transport::write(&mut bytes, &message).expect("write");

    // The header is what makes it a frame rather than a document.
    let text = String::from_utf8(bytes.clone()).expect("utf-8");
    assert!(text.starts_with("Content-Length: "), "{text}");
    assert!(text.contains("\r\n\r\n"), "{text}");

    let read = transport::read(&mut Cursor::new(bytes)).expect("read");
    assert_eq!(read, Some(message));
}

#[test]
fn two_messages_in_one_stream_are_two_messages() {
    let mut bytes = Vec::new();
    transport::write(&mut bytes, &json!({ "n": 1 })).expect("write");
    transport::write(&mut bytes, &json!({ "n": 2 })).expect("write");

    let mut reader = Cursor::new(bytes);
    assert_eq!(
        transport::read(&mut reader).expect("read"),
        Some(json!({ "n": 1 }))
    );
    assert_eq!(
        transport::read(&mut reader).expect("read"),
        Some(json!({ "n": 2 }))
    );
    assert_eq!(transport::read(&mut reader).expect("read"), None);
}

/// The end of the stream is not an error: a client that closes a pipe has said it is done, and a server
/// that reported that as a failure would look broken every time an editor was closed.
#[test]
fn an_empty_stream_is_the_end_not_an_error() {
    assert_eq!(
        transport::read(&mut Cursor::new(Vec::new())).expect("read"),
        None
    );
}

#[test]
fn a_header_with_no_length_is_refused() {
    let mut bytes = Cursor::new(b"Content-Type: application/vscode-jsonrpc\r\n\r\n{}".to_vec());
    let error = transport::read(&mut bytes).expect_err("no length");

    assert!(error.to_string().contains("Content-Length"), "{error}");
}

#[test]
fn a_length_that_is_not_a_number_is_refused() {
    let mut bytes = Cursor::new(b"Content-Length: two\r\n\r\n{}".to_vec());
    let error = transport::read(&mut bytes).expect_err("not a number");

    assert!(error.to_string().contains("not a number"), "{error}");
}

/// A length is bytes from a stream, and a stream can say anything. Without a ceiling, one malformed
/// header is an allocation of that size — a way to kill an editor by typing at it.
#[test]
fn an_absurd_length_is_refused_before_anything_is_allocated() {
    let mut bytes = Cursor::new(b"Content-Length: 999999999999\r\n\r\n".to_vec());
    let error = transport::read(&mut bytes).expect_err("absurd");

    assert!(error.to_string().contains("ceiling"), "{error}");
}

#[test]
fn a_body_that_ends_early_is_refused() {
    let mut bytes = Cursor::new(b"Content-Length: 40\r\n\r\n{\"jsonrpc\":\"2.0\"}".to_vec());
    transport::read(&mut bytes).expect_err("a short body is not a message");
}

#[test]
fn a_body_that_is_not_json_is_refused() {
    let mut bytes = Cursor::new(b"Content-Length: 5\r\n\r\n{oops".to_vec());
    let error = transport::read(&mut bytes).expect_err("not JSON");

    assert!(error.to_string().contains("JSON"), "{error}");
}

/// Header names are case-insensitive in the protocol, and a reader that accepts one spelling is a
/// reader that breaks on one client.
#[test]
fn the_header_name_is_case_insensitive() {
    let body = b"{\"n\":1}";
    let mut bytes = format!("content-length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(body);

    assert_eq!(
        transport::read(&mut Cursor::new(bytes)).expect("read"),
        Some(json!({ "n": 1 }))
    );
}
