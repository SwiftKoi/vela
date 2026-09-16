//! The capabilities that read the index: definition, references, rename, and hover.
//!
//! Split from `server_tests` when it outgrew the file budget, and the seam is real: those tests cover
//! the protocol's *lifecycle* — what is answered, what is published, what stops the loop — and these
//! cover what the server says about a position, which is where the two sources of an answer meet.

use serde_json::{Value, json};

use super::server_tests::{
    MAIN, about_position, cross_module, framed, notice, opened, read_all, replies, request,
};

#[test]
fn goto_definition_lands_in_another_module() {
    let mut server = cross_module();
    let mut output = Vec::new();
    server
        .serve(
            &mut framed(&[
                request(1, "initialize", json!({})),
                notice(
                    "textDocument/didOpen",
                    json!({ "textDocument": { "uri": "file:///game/main.vela", "text": MAIN } }),
                ),
                // Line 3 is the `jump`, and column 12 is inside `forest.clearing`.
                about_position(
                    2,
                    "textDocument/definition",
                    "file:///game/main.vela",
                    3,
                    12,
                ),
            ]),
            &mut output,
        )
        .expect("the server answers");

    let messages = read_all(output);
    let reply = messages
        .iter()
        .find(|message| message["id"] == 2)
        .expect("a reply to the request");

    let locations = reply["result"].as_array().expect("a list of locations");
    assert_eq!(locations.len(), 1, "{reply}");
    assert_eq!(
        locations[0]["uri"], "file:///game/chapters/forest.vela",
        "the label lives in the other module"
    );
    assert_eq!(locations[0]["range"]["start"]["line"], 0);
}

/// References are the same walk, and the declaration is among them: an editor that renamed a label
/// without its declaration would leave the project saying two different things.
#[test]
fn references_name_every_place_including_the_declaration() {
    let mut server = cross_module();
    let mut output = Vec::new();
    server
        .serve(
            &mut framed(&[
                request(1, "initialize", json!({})),
                notice(
                    "textDocument/didOpen",
                    json!({ "textDocument": { "uri": "file:///game/main.vela", "text": MAIN } }),
                ),
                about_position(
                    2,
                    "textDocument/references",
                    "file:///game/main.vela",
                    3,
                    12,
                ),
            ]),
            &mut output,
        )
        .expect("the server answers");

    let messages = read_all(output);
    let reply = messages
        .iter()
        .find(|message| message["id"] == 2)
        .expect("a reply to the request");

    let locations = reply["result"].as_array().expect("a list of locations");
    let uris: Vec<&str> = locations
        .iter()
        .map(|location| location["uri"].as_str().unwrap_or(""))
        .collect();

    assert!(uris.contains(&"file:///game/main.vela"), "{uris:?}");
    assert!(
        uris.contains(&"file:///game/chapters/forest.vela"),
        "{uris:?}"
    );
}

/// A position that names nothing answers `null` rather than the enclosing label: an editor draws no
/// goto arrow, which is honest, instead of jumping somewhere plausible.
#[test]
fn a_position_that_names_nothing_answers_null() {
    let mut server = cross_module();
    let mut output = Vec::new();
    server
        .serve(
            &mut framed(&[
                request(1, "initialize", json!({})),
                notice(
                    "textDocument/didOpen",
                    json!({ "textDocument": { "uri": "file:///game/main.vela", "text": MAIN } }),
                ),
                // Line 0 is `use chapters.forest as forest`; column 1 is inside the keyword, which is
                // not a name this index knows.
                about_position(2, "textDocument/definition", "file:///game/main.vela", 1, 0),
            ]),
            &mut output,
        )
        .expect("the server answers");

    let messages = read_all(output);
    let reply = messages
        .iter()
        .find(|message| message["id"] == 2)
        .expect("a reply to the request");
    assert!(reply["result"].is_null(), "{reply}");
}

/// Hover answers for a name the index knows, and for the things inside a body that it does not.
///
/// The two sources are worth testing through the protocol because the *shape* is the same for both and
/// only the text differs — an editor does not care which of them answered.
#[test]
fn hover_answers_for_names_and_for_locals() {
    let messages = replies(&[
        request(1, "initialize", json!({})),
        opened(),
        // Line 0 is `label start:` and column 6 is inside `start`.
        about_position(2, "textDocument/hover", "file:///game/main.vela", 0, 6),
        // Line 1 is `    var n: int = 1`, and column 8 is the `n`.
        about_position(3, "textDocument/hover", "file:///game/main.vela", 1, 8),
        // Line 5 is `    return`, which names nothing.
        about_position(4, "textDocument/hover", "file:///game/main.vela", 5, 5),
    ]);

    let hover = |id: u32| -> String {
        messages
            .iter()
            .find(|message| message["id"] == id)
            .map(|message| {
                message["result"]["contents"]["value"]
                    .as_str()
                    .unwrap_or("")
            })
            .unwrap_or_default()
            .to_string()
    };

    // `label start:` — the index knows the kind, the qualified name, and the file.
    assert_eq!(hover(2), "**label** `main.start` — declared in `main.vela`");
    // `var n: int = 1` — a local, whose type only the checker knows.
    assert_eq!(hover(3), "**local** `n: int`");
    // Nothing nameable there.
    assert_eq!(
        messages
            .iter()
            .find(|message| message["id"] == 4)
            .map(|message| message["result"].clone()),
        Some(Value::Null),
        "a position that names nothing has nothing to say"
    );
}

/// A rename comes back as a workspace edit, one document at a time, with the *name* in each range.
///
/// The ranges are what an editor applies verbatim, so a range that covered `jump forest.clearing`
/// instead of `clearing` would delete the keyword.
#[test]
fn a_rename_edits_every_file_the_name_appears_in() {
    let mut server = cross_module();
    let mut output = Vec::new();
    server
        .serve(
            &mut framed(&[
                request(1, "initialize", json!({})),
                notice(
                    "textDocument/didOpen",
                    json!({ "textDocument": { "uri": "file:///game/main.vela", "text": MAIN } }),
                ),
                request(
                    2,
                    "textDocument/rename",
                    json!({
                        "textDocument": { "uri": "file:///game/main.vela" },
                        "position": { "line": 3, "character": 12 },
                        "newName": "glade",
                    }),
                ),
            ]),
            &mut output,
        )
        .expect("the server answers");

    let messages = read_all(output);
    let reply = messages
        .iter()
        .find(|message| message["id"] == 2)
        .expect("a reply to the request");

    let changes = &reply["result"]["changes"];
    let main = changes["file:///game/main.vela"]
        .as_array()
        .unwrap_or_else(|| panic!("the reference is not edited: {reply}"));
    assert_eq!(main.len(), 1, "{reply}");
    assert_eq!(main[0]["newText"], "glade");

    // `    jump forest.clearing` — the target starts at column 9, and only `clearing` is replaced.
    assert_eq!(main[0]["range"]["start"]["character"], 16);
    assert_eq!(main[0]["range"]["end"]["character"], 24);

    let forest = changes["file:///game/chapters/forest.vela"]
        .as_array()
        .unwrap_or_else(|| panic!("the declaration is not edited: {reply}"));
    assert_eq!(forest.len(), 1, "{reply}");
}

/// A rename that would not be a name is refused *before* anything is edited: an editor that applied it
/// would leave a file the parser cannot read, and the author looking at an error nobody wrote.
#[test]
fn a_rename_to_something_that_is_not_a_name_is_refused() {
    let mut server = cross_module();
    let mut output = Vec::new();
    server
        .serve(
            &mut framed(&[
                request(1, "initialize", json!({})),
                notice(
                    "textDocument/didOpen",
                    json!({ "textDocument": { "uri": "file:///game/main.vela", "text": MAIN } }),
                ),
                request(
                    2,
                    "textDocument/rename",
                    json!({
                        "textDocument": { "uri": "file:///game/main.vela" },
                        "position": { "line": 3, "character": 12 },
                        "newName": "not a name",
                    }),
                ),
            ]),
            &mut output,
        )
        .expect("the server answers");

    let messages = read_all(output);
    let reply = messages
        .iter()
        .find(|message| message["id"] == 2)
        .expect("a reply to the request");

    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    assert!(reply.get("result").is_none(), "{reply}");
}
