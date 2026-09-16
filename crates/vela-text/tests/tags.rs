//! Text tags: what a line of dialogue carries inside it, and what the presenter reads.
//!
//! `BYTECODE.md §3.3` — a line of dialogue is one command, and a tag changes how it is drawn
//! rather than what the story does. So the vocabulary is checked while parsing (`E0010`) and read
//! while drawing, and these tests are about the second half.

use vela_text::tags;

/// The vocabulary is the parser's and the renderer's, not two lists.
#[test]
fn the_vocabulary_is_what_the_language_defines() {
    for tag in ["b", "i", "/b", "/i", "/"] {
        assert!(tags::is_known(tag), "`{tag}` should be known");
    }
    for tag in ["color=red", "size", "", "B"] {
        assert!(!tags::is_known(tag), "`{tag}` should not be known");
    }
}

/// A tag is removed, and the words around it keep their emphasis.
#[test]
fn a_line_splits_into_runs() {
    let split = tags::runs("a {b}bold{/b} plain");
    assert_eq!(split.len(), 3);
    assert_eq!(split[0].text, "a ");
    assert!(!split[0].bold);
    assert_eq!(split[1].text, "bold");
    assert!(split[1].bold);
    assert_eq!(split[2].text, " plain");
    assert!(!split[2].bold);
}

/// Italic is a tag of its own, and `{/}` closes whatever is open.
#[test]
fn emphasis_is_tracked_per_run() {
    let split = tags::runs("{i}slanted{/i}{b}heavy{/}");
    assert_eq!(split.len(), 2);
    assert!(split[0].italic && !split[0].bold);
    assert!(split[1].bold && !split[1].italic);
}

/// A line with nothing in it is one run, so the ordinary path stays the ordinary path.
#[test]
fn a_plain_line_is_one_run() {
    let split = tags::runs("no markup here");
    assert_eq!(split.len(), 1);
    assert_eq!(split[0].text, "no markup here");
    assert!(!split[0].bold && !split[0].italic);
}

/// `{{` is one literal `{` — the escape the language documents.
///
/// A `}` on its own is *not* special: only the opening sigil needs escaping, because a brace that
/// was never opened cannot close anything, and a line that had to escape both would make `50%}`
/// unprintable.
#[test]
fn an_escaped_brace_is_a_brace() {
    assert_eq!(tags::plain("a {{b c"), "a {b c");
    assert_eq!(tags::plain("50%}} off"), "50%}} off");
    assert_eq!(tags::runs("a {{b c").len(), 1);
}

/// An unknown tag is dropped rather than drawn: the compiler refuses one, so reaching here with
/// one means the module came from somewhere else, and visible markup is worse than a missing
/// colour.
#[test]
fn an_unknown_tag_is_dropped() {
    assert_eq!(tags::plain("a {color=red} b"), "a  b");
}
