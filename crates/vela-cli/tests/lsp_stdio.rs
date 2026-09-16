//! The language server as a process, driven the way an editor drives it.
//!
//! The unit tests in `vela-lsp` cover the protocol against in-memory streams. What they cannot cover is
//! the half that only exists in a real run: that `vela lsp` starts, that the workspace is loaded by the
//! *command line's* loader, that the protocol really goes over stdin and stdout, and — the one that
//! would be embarrassing — that nothing else writes to stdout and corrupts the stream.
//!
//! So this spawns the binary, sends framed messages, and reads framed messages back.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use vela_lsp::transport;

/// A project with one error the editor must be told about, and one warning.
fn project() -> PathBuf {
    let base = std::env::temp_dir().join(format!("vela-lsp-stdio-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("src")).expect("create the fixture");

    std::fs::write(
        base.join("vela.toml"),
        "schema = 1\n\n[project]\nname = \"stdio\"\nversion = \"0.1.0\"\nentry = \"main.start\"\n",
    )
    .expect("write the manifest");
    std::fs::write(
        base.join("src").join("main.vela"),
        "label start:\n    var n: int = 1\n    var b: bool = n\n    \"unreached\"\n    return\n",
    )
    .expect("write the source");

    base
}

/// Frames a message for the child's stdin.
fn frame(bytes: &mut Vec<u8>, message: &Value) {
    transport::write(bytes, message).expect("frame");
}

/// The messages the child wrote.
fn read_all(bytes: Vec<u8>) -> Vec<Value> {
    let mut reader = std::io::Cursor::new(bytes);
    let mut messages = Vec::new();
    while let Some(message) = transport::read(&mut reader).expect("read a message") {
        messages.push(message);
    }
    messages
}

#[test]
fn the_server_answers_the_handshake_and_publishes_what_check_reports() {
    let project = project();
    let uri = format!("file://{}", project.join("src/main.vela").display());
    let text = std::fs::read_to_string(project.join("src/main.vela")).expect("read the source");

    let mut child = Command::new(env!("CARGO_BIN_EXE_vela"))
        .arg("lsp")
        .arg(&project)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("start the server");

    let mut input = Vec::new();
    frame(
        &mut input,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                 "params": { "rootUri": format!("file://{}", project.display()) } }),
    );
    frame(
        &mut input,
        &json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
    );
    frame(
        &mut input,
        &json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen",
                 "params": { "textDocument": { "uri": uri, "languageId": "vela",
                                                "version": 1, "text": text } } }),
    );
    frame(
        &mut input,
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }),
    );
    frame(&mut input, &json!({ "jsonrpc": "2.0", "method": "exit" }));

    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(&input)
        .expect("write the messages");

    let output = child.wait_with_output().expect("the server finishes");
    assert!(output.status.success(), "the server exited badly");

    let messages = read_all(output.stdout);
    let methods: Vec<&str> = messages
        .iter()
        .filter_map(|message| message["method"].as_str())
        .collect();
    assert!(
        methods.contains(&"textDocument/publishDiagnostics"),
        "nothing was published: {messages:?}"
    );

    let published = messages
        .iter()
        .find(|message| message["method"] == "textDocument/publishDiagnostics")
        .expect("a publish");
    assert_eq!(published["params"]["uri"], uri);

    let diagnostics = published["params"]["diagnostics"]
        .as_array()
        .expect("an array");
    let codes: Vec<&str> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().unwrap_or(""))
        .collect();
    assert!(codes.contains(&"E3007"), "{codes:?}");

    // And the handshake was answered, which is what tells an editor the server is there.
    let initialized = messages
        .iter()
        .find(|message| message["id"] == 1)
        .expect("a reply to initialize");
    assert_eq!(initialized["result"]["capabilities"]["textDocumentSync"], 1);

    let _ = std::fs::remove_dir_all(Path::new(&project));
}
