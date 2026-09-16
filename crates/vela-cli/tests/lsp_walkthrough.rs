//! The steps of `docs/guides/lsp-walkthrough.md`, executed against `examples/standard`.
//!
//! The milestone asks for a documented walkthrough, and a document is the one artefact in a repository
//! that cannot fail a test. So each step of the guide is a test here, against the real `vela lsp` binary
//! and the real example project: if the guide says a caret on `chapters.street.arrive` opens
//! `chapters/street.vela`, that is what this asserts. The guide can then only drift by being edited in
//! the same commit as the behaviour.
//!
//! The framing helpers are repeated from `lsp_stdio.rs` rather than shared, because an integration test
//! binary is its own crate: the two tests cover different things, and a `common` module for fifteen
//! lines of framing would hide the one thing worth reading here.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use vela_lsp::transport;

/// The example the guide walks through.
fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/standard")
}

/// A file of the example, and its URI.
struct Open {
    /// The absolute path, as an LSP client would send it.
    uri: String,
    /// Its text, which is where the positions below are computed from.
    text: String,
}

/// `src/main.vela`, `src/chapters/street.vela`, and `src/chapters/hearth.vela`, open and indexed.
fn workspace() -> Vec<Open> {
    [
        "src/main.vela",
        "src/chapters/street.vela",
        "src/chapters/hearth.vela",
    ]
    .into_iter()
    .map(|relative| {
        let path = project().join(relative);
        Open {
            uri: format!("file://{}", path.display()),
            text: std::fs::read_to_string(&path).expect("read the example"),
        }
    })
    .collect()
}

/// The position of `anchor` in `text`, plus `inside` characters.
///
/// The example is ASCII, so a byte offset and a UTF-16 code unit are the same number; the conversion
/// module that does this properly for arbitrary text is `vela_lsp::position`.
fn position(text: &str, anchor: &str, inside: usize) -> Value {
    let offset = text.find(anchor).expect("the example contains it") + inside;
    let line = text[..offset].matches('\n').count();
    let character = offset - (text[..offset].rfind('\n').map_or(0, |index| index + 1));

    json!({ "line": line, "character": character })
}

/// Sends `requests` to a real server over the example, and returns every message it wrote.
fn answers(requests: Vec<(u32, String, usize, Value)>) -> Vec<Value> {
    let files = workspace();
    let mut input = Vec::new();

    let mut frame = |message: &Value| {
        transport::write(&mut input, message).expect("frame");
    };
    frame(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }));
    for file in &files {
        frame(&json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen",
                       "params": { "textDocument": { "uri": file.uri, "text": file.text } } }));
    }
    for (id, method, file, params) in &requests {
        let mut params = params.clone();
        params["textDocument"] = json!({ "uri": files[*file].uri });
        frame(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
    }
    frame(&json!({ "jsonrpc": "2.0", "id": 99, "method": "shutdown" }));
    frame(&json!({ "jsonrpc": "2.0", "method": "exit" }));

    let mut child = Command::new(env!("CARGO_BIN_EXE_vela"))
        .arg("lsp")
        .arg(project())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("start the server");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(&input)
        .expect("write the messages");

    let output = child.wait_with_output().expect("the server finishes");
    assert!(output.status.success(), "the server exited badly");

    let mut reader = std::io::Cursor::new(output.stdout);
    let mut messages = Vec::new();
    while let Some(message) = transport::read(&mut reader).expect("read a message") {
        messages.push(message);
    }
    messages
}

/// The reply to one request.
fn reply(messages: &[Value], id: u32) -> &Value {
    messages
        .iter()
        .find(|message| message["id"] == id)
        .unwrap_or_else(|| panic!("a reply to {id}: {messages:?}"))
}

/// One request: its id, its method, which file it is about, and its parameters.
type Ask = (u32, String, usize, Value);

/// A request about `file`.
fn ask(id: u32, method: &str, file: usize, params: Value) -> Ask {
    (id, method.to_string(), file, params)
}

/// Wraps a position into a request, with the anchor named so the call site reads as an editor action.
fn at(id: u32, method: &str, file: usize, text: &str, anchor: &str, inside: usize) -> Ask {
    ask(
        id,
        method,
        file,
        json!({ "position": position(text, anchor, inside) }),
    )
}

/// Step 1: the definition of a cross-module transfer opens the other module's file.
#[test]
fn step_1_a_jump_resolves_into_another_module() {
    let files = workspace();
    let main = &files[0].text;
    let messages = answers(vec![at(
        2,
        "textDocument/definition",
        0,
        main,
        "jump chapters.street.arrive",
        5,
    )]);

    let locations = reply(&messages, 2)["result"]
        .as_array()
        .expect("a list of locations");
    assert_eq!(locations.len(), 1, "{locations:?}");
    assert!(
        locations[0]["uri"]
            .as_str()
            .unwrap_or("")
            .ends_with("src/chapters/street.vela"),
        "the definition is in the file that declares it: {locations:?}"
    );

    // And on the line of `label arrive:`, which is what the guide claims.
    let street = &files[1].text;
    let arrive = position(street, "label arrive:", 6);
    assert_eq!(locations[0]["range"]["start"], arrive, "{locations:?}");
}

/// Step 2: hover names the kind, the qualified name, and the file.
#[test]
fn step_2_hover_names_the_kind_and_the_file() {
    let files = workspace();
    let messages = answers(vec![at(
        2,
        "textDocument/hover",
        0,
        &files[0].text,
        "jump chapters.street.arrive",
        5,
    )]);

    assert_eq!(
        reply(&messages, 2)["result"]["contents"]["value"],
        "**label** `chapters.street.arrive` — declared in `chapters/street.vela`"
    );
}

/// Step 2, second half: an expression is answered too, which is the part no index could do.
#[test]
fn step_2_hover_answers_for_an_expression_too() {
    let files = workspace();
    let messages = answers(vec![at(
        2,
        "textDocument/hover",
        0,
        &files[0].text,
        "Ending.cold",
        7,
    )]);

    assert_eq!(
        reply(&messages, 2)["result"]["contents"]["value"],
        "`Ending`"
    );
}

/// Step 3: references from a declaration reach the module that calls it, qualified.
#[test]
fn step_3_references_find_the_qualified_callers() {
    let files = workspace();
    let messages = answers(vec![at(
        2,
        "textDocument/references",
        0,
        &files[0].text,
        "label back_from_street",
        6,
    )]);

    let locations = reply(&messages, 2)["result"]
        .as_array()
        .expect("a list of locations");
    let places: Vec<String> = locations
        .iter()
        .map(|location| {
            format!(
                "{}:{}",
                location["uri"]
                    .as_str()
                    .unwrap_or("")
                    .rsplit('/')
                    .next()
                    .unwrap_or(""),
                location["range"]["start"]["line"]
            )
        })
        .collect();

    assert_eq!(
        places,
        vec!["street.vela:35", "street.vela:39", "main.vela:111"],
        "the declaration, and the two jumps in the file that imports this module"
    );
}

/// Step 4: completion offers the labels this module can reach, qualified as they must be written.
#[test]
fn step_4_completion_offers_reachable_labels() {
    let files = workspace();
    let messages = answers(vec![at(
        2,
        "textDocument/completion",
        0,
        &files[0].text,
        "jump chapters.street.arrive",
        5,
    )]);

    let items = reply(&messages, 2)["result"]["items"]
        .as_array()
        .expect("a list of items");
    let listed: Vec<&str> = items
        .iter()
        .map(|item| item["label"].as_str().unwrap_or(""))
        .collect();

    assert_eq!(
        listed,
        vec![
            "back_from_hearth",
            "back_from_street",
            "chapters.hearth",
            "chapters.hearth.settle",
            "chapters.street",
            "chapters.street.arrive",
            "ending",
            "start",
            "tally",
        ],
        "the guide prints this list, so this test is what keeps it honest"
    );
}

/// Step 6: the screen vocabulary is answered from the same schema the reference is generated from.
///
/// `column` and `close_screen` are not declared anywhere a name is, so neither the index nor the checker
/// knows them — they come from `vela-ui`'s registries, which is what makes this the third source and
/// what keeps the hover and `docs/reference/` saying the same sentence.
#[test]
fn step_6_the_screen_vocabulary_answers_from_the_schema() {
    let files = workspace();
    let main = &files[0].text;
    let messages = answers(vec![
        at(2, "textDocument/hover", 0, main, "column gap", 1),
        at(3, "textDocument/hover", 0, main, "action close_screen", 10),
    ]);

    assert_eq!(
        reply(&messages, 2)["result"]["contents"]["value"],
        "**widget** `column` — It is a container, and takes children."
    );
    assert_eq!(
        reply(&messages, 3)["result"]["contents"]["value"],
        "**action** `close_screen()` — Dismiss the screen this action is in."
    );
}

/// Step 5: a rename edits every file the name is written in, which is what makes it safe.
#[test]
fn step_5_the_rename_touches_both_files() {
    let files = workspace();
    let messages = answers(vec![ask(
        2,
        "textDocument/rename",
        0,
        json!({
            "position": position(&files[0].text, "label back_from_street", 6),
            "newName": "returned_from_street",
        }),
    )]);

    let changes = reply(&messages, 2)["result"]["changes"]
        .as_object()
        .expect("a set of changes per document");
    let mut edits: Vec<String> = changes
        .iter()
        .flat_map(|(uri, edits)| {
            edits
                .as_array()
                .expect("a list of edits")
                .iter()
                .map(|edit| {
                    format!(
                        "{}:{} -> {}",
                        uri.rsplit('/').next().unwrap_or(""),
                        edit["range"]["start"]["line"],
                        edit["newText"].as_str().unwrap_or("")
                    )
                })
        })
        .collect();
    edits.sort();

    assert_eq!(
        edits,
        vec![
            "main.vela:111 -> returned_from_street",
            "street.vela:35 -> returned_from_street",
            "street.vela:39 -> returned_from_street",
        ],
        "the declaration and both qualified references"
    );
}
