//! URIs, names, and the translation between them.
//!
//! The interesting cases are the ones where the two sides would disagree about *which file*: a path
//! with a space in it, and a document that is not under the workspace root at all.

use crate::uri;

#[test]
fn a_file_uri_becomes_its_path() {
    assert_eq!(
        uri::path("file:///home/nikita/game/src/main.vela").as_deref(),
        Some("/home/nikita/game/src/main.vela")
    );
}

/// Editors escape what a URI cannot carry, and a filename with a space in it is a filename.
#[test]
fn a_percent_escaped_path_is_decoded() {
    assert_eq!(
        uri::path("file:///home/nikita/my%20game/src/main.vela").as_deref(),
        Some("/home/nikita/my game/src/main.vela")
    );
    // And a stray `%` is a file that exists rather than an error to refuse the document over.
    assert_eq!(
        uri::path("file:///home/100%_done/main.vela").as_deref(),
        Some("/home/100%_done/main.vela")
    );
}

/// A `file:` URI with a host names another machine's filesystem, which this server cannot read.
#[test]
fn a_uri_with_a_host_is_refused_rather_than_guessed_at() {
    assert!(uri::path("file://elsewhere/src/main.vela").is_none());
}

#[test]
fn a_uri_that_is_not_a_file_is_refused() {
    assert!(uri::path("untitled:Untitled-1").is_none());
    assert!(uri::path("https://example.com/main.vela").is_none());
}

/// The name is what the session keys a file by — so it has to be the same name `vela check` gives the
/// same file, or the editor and the command line would be reading two files that hold the same text.
#[test]
fn a_path_under_the_root_becomes_a_name_relative_to_it() {
    assert_eq!(
        uri::name("/home/nikita/game", "/home/nikita/game/src/main.vela"),
        "src/main.vela"
    );
    // A root written with a trailing slash is the same root.
    assert_eq!(
        uri::name(
            "/home/nikita/game/",
            "/home/nikita/game/src/chapters/forest.vela"
        ),
        "src/chapters/forest.vela"
    );
}

/// A name becomes a URI again, spaces and all: goto-definition lands in files the editor has never
/// opened, and it has to address them the same way the editor would.
#[test]
fn a_name_becomes_a_uri_under_the_root() {
    assert_eq!(
        uri::of("/home/nikita/game", "src/main.vela"),
        "file:///home/nikita/game/src/main.vela"
    );

    let uri = uri::of("/home/nikita/my game", "src/a b.vela");
    assert_eq!(uri, "file:///home/nikita/my%20game/src/a%20b.vela");
    assert_eq!(
        uri::name(
            "/home/nikita/my game",
            &uri::path(&uri).expect("a file uri")
        ),
        "src/a b.vela"
    );
}

/// A document from outside the workspace keeps its own path: a name is better than a refusal, because
/// the file still has to be checked.
#[test]
fn a_path_outside_the_root_keeps_its_path() {
    assert_eq!(uri::name("/game", "/tmp/scratch.vela"), "/tmp/scratch.vela");
}

/// A name that is not under any root is still a name: the session keys it by what it was given, and
/// the server has no reason to refuse a document for being somewhere unexpected.
#[test]
fn an_absolute_name_outside_the_root_is_its_own_name() {
    assert_eq!(uri::name("/game", "/game/src/main.vela"), "src/main.vela");
    assert_eq!(
        uri::name("/game", "/elsewhere/other.vela"),
        "/elsewhere/other.vela"
    );
}
