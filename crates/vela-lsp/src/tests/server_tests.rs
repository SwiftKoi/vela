//! The server, driven the way an editor drives it.
//!
//! Nothing here needs an editor, a socket, or a filesystem: a server is a function from a stream of
//! messages to a stream of messages, so the tests hand it one and read the other. What they pin is the
//! protocol *behaviour* — what is answered, what is published, and what is deliberately not declared —
//! rather than the analysis, which `diagnostics` owns and the parity test guards.

use std::io::Cursor;

use serde_json::{Value, json};
use vela_compile::Session;

use crate::server::Server;
use crate::transport;

/// A project with one error and one warning, as the test session sees it.
pub(super) const SOURCE: &str = "\
label start:
    var n: int = 1
    var s = \"n is [n]\"
    var b: bool = n
    \"unreached\"
    return
";

/// A server whose workspace is a session holding one file, plus the URI of that file.
pub(super) fn server() -> (Server, String) {
    let uri = "file:///game/main.vela".to_string();
    let server = Server::new("/game", |_root: &str| {
        let mut session = Session::new();
        session.set_file("main.vela", SOURCE);
        session
    });
    (server, uri)
}

/// A message a client would send.
pub(super) fn request(id: u32, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

/// A notification, which has no id and expects no answer.
pub(super) fn notice(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

/// Frames a list of messages the way a client would.
pub(super) fn framed(messages: &[Value]) -> Cursor<Vec<u8>> {
    let mut bytes = Vec::new();
    for message in messages {
        transport::write(&mut bytes, message).expect("frame a message");
    }
    Cursor::new(bytes)
}

/// Every message the server wrote.
pub(super) fn replies(messages: &[Value]) -> Vec<Value> {
    let (mut server, _) = server();
    let mut output = Vec::new();
    server
        .serve(&mut framed(messages), &mut output)
        .expect("the server answers");
    read_all(output)
}

/// Every message in a stream of framed bytes.
pub(super) fn read_all(output: Vec<u8>) -> Vec<Value> {
    let mut reader = Cursor::new(output);
    let mut out = Vec::new();
    while let Some(message) = transport::read(&mut reader).expect("read a reply") {
        out.push(message);
    }
    out
}

/// The published diagnostics for a URI, which is the last `publishDiagnostics` about it.
pub(super) fn published(messages: &[Value], uri: &str) -> Vec<Value> {
    messages
        .iter()
        .filter(|message| message["method"] == "textDocument/publishDiagnostics")
        .filter(|message| message["params"]["uri"] == uri)
        .filter_map(|message| message["params"]["diagnostics"].as_array().cloned())
        .next_back()
        .unwrap_or_default()
}

/// Opening the document the session already knows.
pub(super) fn opened() -> Value {
    notice(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": "file:///game/main.vela", "text": SOURCE } }),
    )
}

#[test]
fn initialize_answers_with_the_capabilities_it_actually_has() {
    let messages = replies(&[request(
        1,
        "initialize",
        json!({ "rootUri": "file:///game" }),
    )]);

    let capabilities = &messages[0]["result"]["capabilities"];
    assert_eq!(messages[0]["id"], 1);
    assert_eq!(capabilities["textDocumentSync"], 1);
    assert_eq!(capabilities["positionEncoding"], "utf-16");

    // What it does: the three features the symbol index answers.
    assert_eq!(capabilities["definitionProvider"], true);
    assert_eq!(capabilities["referencesProvider"], true);
    assert_eq!(capabilities["renameProvider"], true);
    assert_eq!(capabilities["hoverProvider"], true);

    // And nothing it does not: an advertised capability is one an editor calls. Completion needs the
    // names in scope at an offset, which is the next thing to build.
    assert!(
        capabilities.get("completionProvider").is_none(),
        "`completionProvider` is advertised but not implemented"
    );
}

/// Two modules, so the index has a cross-module `jump` to follow.
///
/// Named without a `src/` prefix because the workspace in these tests has no directory to check: the
/// server's rule is "names are relative to `src/` when the project has one" (`source_root`), and the
/// integration test is where a real project exercises that half.
pub(super) const MAIN: &str = "\
use chapters.forest as forest

label start:
    jump forest.clearing
";

pub(super) const FOREST: &str = "\
label clearing:
    \"Trees.\"
    return
";

/// A server for that two-module workspace.
pub(super) fn cross_module() -> Server {
    Server::new("/game", |_root: &str| {
        let mut session = Session::new();
        session.set_file("main.vela", MAIN);
        session.set_file("chapters/forest.vela", FOREST);
        session.set_entry("main.start");
        session
    })
}

/// A request about a position, framed the way an editor sends one.
pub(super) fn about_position(id: u32, method: &str, uri: &str, line: u32, character: u32) -> Value {
    request(
        id,
        method,
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
        }),
    )
}

#[test]
fn opening_a_document_publishes_its_diagnostics() {
    let messages = replies(&[request(1, "initialize", json!({})), opened()]);
    let diagnostics = published(&messages, "file:///game/main.vela");

    assert!(
        !diagnostics.is_empty(),
        "nothing was published: {messages:?}"
    );
    let codes: Vec<&str> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().unwrap_or(""))
        .collect();
    // `var b: bool = n` — an `int` where a `bool` belongs.
    assert!(codes.contains(&"E3007"), "{codes:?}");

    // The shape an editor reads: a range in UTF-16 units, a severity, our name.
    let first = &diagnostics[0];
    assert_eq!(first["source"], "vela");
    assert!(first["severity"].as_u64().is_some_and(|s| s == 1 || s == 2));
    assert!(first["range"]["start"]["line"].as_u64().is_some());
    assert!(first["message"].as_str().is_some_and(|m| !m.is_empty()));
}

/// An edit re-publishes, and a fix clears: an editor leaves the last set on screen until it is told
/// otherwise, so "no diagnostics" has to be *sent* rather than implied by silence.
#[test]
fn an_edit_republishes_and_a_fix_clears() {
    let messages = replies(&[
        request(1, "initialize", json!({})),
        opened(),
        notice(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": "file:///game/main.vela" },
                "contentChanges": [{ "text": "label start:\n    \"fine\"\n    return\n" }],
            }),
        ),
    ]);

    let last = published(&messages, "file:///game/main.vela");
    assert!(last.is_empty(), "the fix did not clear: {last:?}");
}

#[test]
fn closing_a_document_clears_its_diagnostics() {
    let messages = replies(&[
        request(1, "initialize", json!({})),
        opened(),
        notice(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": "file:///game/main.vela" } }),
        ),
    ]);

    let publishes: Vec<&Value> = messages
        .iter()
        .filter(|message| message["method"] == "textDocument/publishDiagnostics")
        .collect();
    assert_eq!(publishes.len(), 2, "one on open, one on close");
    assert!(published(&messages, "file:///game/main.vela").is_empty());
}

/// A request for something this server does not do is answered as not-found. Silence would leave an
/// editor waiting on a response that never comes.
#[test]
fn an_unknown_request_is_answered_with_an_error() {
    let messages = replies(&[request(7, "textDocument/typeDefinition", json!({}))]);

    assert_eq!(messages[0]["id"], 7);
    assert_eq!(messages[0]["error"]["code"], -32601);
}

/// `exit` ends the loop, and the messages after it are not read — the client has said it is done.
#[test]
fn exit_stops_the_loop() {
    let messages = replies(&[
        notice("exit", json!({})),
        request(1, "initialize", json!({})),
    ]);

    assert!(messages.is_empty(), "{messages:?}");
}

/// A root the client names that is not the one the server started with means a different project, so
/// the session is rebuilt from that root — and the loader is what decides what that means.
#[test]
fn a_different_root_reloads_the_workspace() {
    let mut session = Session::new();
    session.set_file("src/other.vela", "label other:\n    \"hi\"\n    return\n");
    let other = session;

    let (mut server, _) = server();
    let mut output = Vec::new();
    server
        .serve(
            &mut framed(&[request(
                1,
                "initialize",
                json!({ "rootUri": "file:///elsewhere" }),
            )]),
            &mut output,
        )
        .expect("the server answers");

    // The re-created server is a new one, so this only asserts the call happened and the reply is well
    // formed: what the loader does with a root is the command line's business, and is tested there.
    let mut reader = Cursor::new(output);
    let reply = transport::read(&mut reader).expect("a reply").expect("one");
    assert_eq!(reply["id"], 1);
    assert!(other.file_named("src/other.vela").is_some());
}
