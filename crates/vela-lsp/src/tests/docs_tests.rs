//! The generated reference in a hover: `crate::docs`.
//!
//! The vocabulary a screen uses is not declared anywhere a name is, so hover answers it from the same
//! schema `vela doc` renders — and links into that page when the workspace has generated one. These
//! tests cover both halves: the sentence, the context that decides whether the sentence is even true,
//! and the link appearing exactly when the file it points at exists.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

use crate::docs::{self, Reference};
use crate::server::Server;

use super::server_tests::{SCREEN, about_position, framed, notice, position_of, read_all, request};

/// A server over the screen fixture, rooted at `root`.
fn with_screen(root: &str) -> Server {
    let root = root.to_string();
    Server::new(root, |_root: &str| {
        let mut session = vela_compile::Session::new();
        session.set_file("main.vela", SCREEN);
        session
    })
}

/// A workspace that has generated one reference page, in a directory of its own.
///
/// One directory per call because the tests run in parallel: sharing a name would let one test's
/// cleanup delete the page another is still hovering over, which reads as a broken link.
fn with_page(page: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "vela-lsp-docs-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(root.join("docs/reference")).expect("create the reference directory");
    std::fs::write(
        root.join("docs/reference").join(page),
        "# Widget reference\n",
    )
    .expect("write the page");
    root
}

/// What hover answers at a position, as the protocol sends it.
fn hover(server: &mut Server, line: u32, character: u32) -> Value {
    let mut output = Vec::new();
    server
        .serve(
            &mut framed(&[
                request(1, "initialize", json!({})),
                notice(
                    "textDocument/didOpen",
                    json!({ "textDocument": { "uri": "file:///game/main.vela", "text": SCREEN } }),
                ),
                about_position(
                    2,
                    "textDocument/hover",
                    "file:///game/main.vela",
                    line,
                    character,
                ),
            ]),
            &mut output,
        )
        .expect("the server answers");

    read_all(output)
        .iter()
        .find(|message| message["id"] == 2)
        .map(|message| message["result"].clone())
        .unwrap_or(Value::Null)
}

/// The markdown hover produced, or empty when it said nothing.
fn markdown(server: &mut Server, line: u32, character: u32) -> String {
    hover(server, line, character)["contents"]["value"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

/// The vocabulary knows widgets and actions, and nothing else.
#[test]
fn the_vocabulary_answers_widgets_and_actions() {
    let widget = docs::lookup("text").expect("`text` is a widget");
    assert_eq!(widget.kind, "widget");
    assert_eq!(widget.summary, "It draws, and holds nothing.");
    assert_eq!(
        (widget.page, widget.title),
        ("widgets.md", "Widget reference")
    );

    let action = docs::lookup("open_screen").expect("`open_screen` is an action");
    assert_eq!(action.kind, "action");
    assert_eq!(action.signature, "open_screen(screen)");
    assert_eq!(
        (action.page, action.title),
        ("actions.md", "Action reference")
    );

    assert!(docs::lookup("not_a_widget").is_none());
    assert!(docs::lookup("").is_none());
}

/// A word is what the caret is inside, and punctuation is not a word.
#[test]
fn a_word_is_what_the_caret_is_inside() {
    let text = "action close_screen()";
    assert_eq!(docs::word_at(text, 3), Some(("action", 0, 6)));
    assert_eq!(docs::word_at(text, 13), Some(("close_screen", 7, 19)));
    assert_eq!(
        docs::word_at(text, 6),
        Some(("action", 0, 6)),
        "a caret at the space after a word is still on the word it follows"
    );
    assert_eq!(
        docs::word_at(text, 19),
        Some(("close_screen", 7, 19)),
        "and a caret between a word and the punctuation after it is on the word"
    );
    assert_eq!(
        docs::word_at(text, 20),
        None,
        "a caret on `)` is not on a word"
    );
    assert_eq!(docs::word_at("", 0), None);
}

/// A link aims at the heading the page wrote, and only exists when the page does.
#[test]
fn a_link_points_at_the_heading_only_when_the_page_is_there() {
    let root = with_page("actions.md");
    let reference = Reference::new(root.display().to_string());
    let link = reference
        .link("actions.md", "close_screen()")
        .expect("the page is in the workspace");
    assert!(link.starts_with("file://"), "{link}");
    assert!(
        link.ends_with("/docs/reference/actions.md#close_screen"),
        "{link}"
    );
    let _ = std::fs::remove_dir_all(&root);

    let absent = Reference::new("/game");
    assert!(
        absent.link("widgets.md", "text").is_none(),
        "a link to a file that is not there is worse than no link"
    );
}

/// A widget at the start of a screen's line is answered from the schema.
#[test]
fn a_widget_is_answered_from_the_schema() {
    let mut server = with_screen("/game");
    let (line, column) = position_of(SCREEN, "column", 0);

    assert_eq!(
        markdown(&mut server, line, column),
        "**widget** `column` — It is a container, and takes children."
    );
}

/// And a word that is *not* the widget is not called one: the same name as a prop's value is a value.
#[test]
fn a_widget_name_that_is_not_at_the_start_of_a_line_is_not_a_widget() {
    let mut server = with_screen("/game");
    let (line, column) = position_of(SCREEN, "text", 1);

    assert_eq!(
        hover(&mut server, line, column),
        Value::Null,
        "`style = text` is a prop's value, not a widget"
    );
}

/// An action after the keyword is answered from the schema, with its arguments in the signature.
#[test]
fn an_action_is_answered_from_the_schema() {
    let mut server = with_screen("/game");
    let (line, column) = position_of(SCREEN, "close_screen", 0);

    assert_eq!(
        markdown(&mut server, line, column),
        "**action** `close_screen()` — Dismiss the screen this action is in."
    );
}

/// When the workspace has the page, the hover links to the section it came from.
#[test]
fn a_generated_page_is_linked_from_the_hover() {
    let root = with_page("widgets.md");
    let mut server = with_screen(&root.display().to_string());
    let (line, column) = position_of(SCREEN, "column", 0);

    let markdown = markdown(&mut server, line, column);
    assert!(
        markdown.contains("/docs/reference/widgets.md#column"),
        "the link should aim at this widget's heading: {markdown}"
    );
    assert!(markdown.contains("[Widget reference]"), "{markdown}");
    let _ = std::fs::remove_dir_all(&root);
}

/// And when it does not, the sentence stands alone — no link to a file that is not there.
#[test]
fn a_workspace_without_the_page_gets_no_link() {
    let mut server = with_screen("/game");
    let (line, column) = position_of(SCREEN, "column", 0);

    let markdown = markdown(&mut server, line, column);
    assert!(!markdown.contains("file://"), "{markdown}");
    assert!(markdown.contains("**widget**"), "{markdown}");
}
