//! Text tags: the markup a line of dialogue carries inside it.
//!
//! `BYTECODE.md §3.3` settles the shape: *a line of dialogue is one command*, and the controls a
//! script puts inside that text are interpreted by the presenter rather than emitted as more
//! commands. A `{b}` is one of those controls — it changes how words are drawn and nothing about
//! what the story does — so it travels inside the `Say` text and is read here.
//!
//! # One vocabulary, below the parser
//!
//! `vela-syntax` validates a tag against [`is_known`] as it parses, so an unknown tag is a
//! diagnostic on the line it is written on rather than a pair of braces a player sees. That is
//! why this module sits *here*, at rank 1, rather than beside the renderer: the language's idea
//! of what a tag is and the renderer's have to be one list, and a second list is how the two come
//! to disagree about `{/}`.
//!
//! # What a tag does not do
//!
//! It does not wrap, nest beyond an on/off pair, or carry arguments. Ren'Py's tag vocabulary is
//! larger — `{color=…}`, `{size=…}`, `{image=…}` — and each of those is a *value* rather than a
//! switch, which wants the parsing and the styling model that rich inline runs will bring.
//! Refusing them by name beats accepting them and drawing nothing.

/// The tags the language defines, without their braces.
const KNOWN: &[&str] = &["b", "i", "/b", "/i", "/"];

/// Whether a tag — the text between its braces — is one the language defines.
#[must_use]
pub fn is_known(tag: &str) -> bool {
    KNOWN.contains(&tag.trim())
}

/// One stretch of text drawn with one emphasis.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Run {
    /// The words, with no markup in them.
    pub text: String,
    /// Whether the run is emphasised as bold.
    pub bold: bool,
    /// Whether the run is emphasised as italic.
    pub italic: bool,
}

/// Splits a line into runs, dropping the tags.
///
/// An unrecognised tag is dropped rather than drawn: the compiler refuses one, so reaching here
/// with an unknown tag means the module was built by something else, and a visible `{color=red}`
/// in the middle of a line is worse than a missing colour.
#[must_use]
pub fn runs(text: &str) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    let (mut bold, mut italic) = (false, false);
    let mut literal = String::new();
    let mut characters = text.chars().peekable();

    while let Some(character) = characters.next() {
        if character != '{' {
            literal.push(character);
            continue;
        }
        // `{{` is one literal brace, which is how a line says "not a tag".
        if characters.peek() == Some(&'{') {
            characters.next();
            literal.push('{');
            continue;
        }

        let mut tag = String::new();
        for inner in characters.by_ref() {
            if inner == '}' {
                break;
            }
            tag.push(inner);
        }

        let switched = match tag.trim() {
            "b" => (true, italic),
            "i" => (bold, true),
            "/b" => (false, italic),
            "/i" => (bold, false),
            // `{/}` closes whatever is open, which is what a writer means by it.
            "/" => (false, false),
            _ => (bold, italic),
        };

        if switched != (bold, italic) {
            if !literal.is_empty() {
                out.push(Run {
                    text: std::mem::take(&mut literal),
                    bold,
                    italic,
                });
            }
            (bold, italic) = switched;
        }
    }

    if !literal.is_empty() || out.is_empty() {
        out.push(Run {
            text: literal,
            bold,
            italic,
        });
    }
    out
}

/// The line with every tag removed.
#[must_use]
pub fn plain(text: &str) -> String {
    runs(text).into_iter().map(|run| run.text).collect()
}
