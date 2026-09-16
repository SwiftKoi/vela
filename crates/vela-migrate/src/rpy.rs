//! Reading Ren'Py's script surface.
//!
//! Ren'Py is indentation-significant in the same way Vela is, so the reader is a line reader: it
//! groups lines into a tree by indentation and classifies each line by its first word. That is
//! deliberately a *surface* reader and not a parser of Python — the goal is to recognize the
//! script constructs `TOOLING.md §8` lists and to be able to say "this line is something else"
//! with the line in hand, which is what the report needs and what a half-parsed Python
//! expression tree would not improve.
//!
//! # Why nothing here guesses
//!
//! A construct the reader does not recognize becomes [`Kind::Unsupported`], which carries the
//! source text and becomes a report entry. It is never "skipped quietly" and it is never
//! approximated into something that looks like a translation: a wrong translation of a menu is a
//! story that takes the wrong branch, and it fails long after the person who ran the tool is
//! gone.

/// One node of a Ren'Py file.
#[derive(Clone, Debug)]
pub struct Node {
    /// The 1-based line it came from.
    pub line: u32,
    /// The source text, trimmed.
    pub text: String,
    /// What the line is.
    pub kind: Kind,
    /// The block under it, when it opens one.
    pub children: Vec<Node>,
}

/// What a line is.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Kind {
    /// `# ...`
    Comment(String),
    /// Nothing at all.
    Blank,
    /// `label name:`
    Label(String),
    /// `menu:` — the caption, if any, is the first [`Kind::Say`] child.
    Menu,
    /// `"text":` or `"text" if cond:` inside a menu.
    Choice {
        /// The choice's text, with its quotes.
        text: String,
        /// The `if` condition, as written.
        condition: Option<String>,
    },
    /// `"text"` or `speaker "text"`.
    Say {
        /// The character, and any attributes before the string.
        speaker: String,
        /// The string, with its quotes.
        text: String,
        /// A `with` clause on the same line, if there is one.
        transition: Option<String>,
    },
    /// `if cond:`
    If(String),
    /// `elif cond:`
    Elif(String),
    /// `else:`
    Else,
    /// `jump target`
    Jump(String),
    /// `call target`
    Call(String),
    /// `return`
    Return,
    /// `scene ...`
    Scene(String),
    /// `show ...`
    Show(String),
    /// `hide ...`
    Hide(String),
    /// `with transition`
    With(String),
    /// `play ...`
    Play(String),
    /// `stop ...`
    Stop(String),
    /// `queue ...`
    Queue(String),
    /// `pause ...`
    Pause(String),
    /// `window ...`
    Window(String),
    /// `$ python`
    Python(String),
    /// `define name = value`
    Define {
        /// The name being defined.
        name: String,
        /// The right-hand side, as written.
        value: String,
    },
    /// `default name = value`
    Default {
        /// The name being defaulted.
        name: String,
        /// The right-hand side, as written.
        value: String,
    },
    /// Anything else, kept whole so the report can quote it.
    Unsupported,
}

/// Reads a file into a tree.
#[must_use]
pub fn read(source: &str) -> Vec<Node> {
    // A byte-order mark is not text: Ren'Py's own scripts are saved with one, and a reader that
    // treated it as content would classify the first line as unsupported.
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);

    let lines: Vec<Line> = source
        .lines()
        .enumerate()
        .map(|(index, text)| Line {
            number: u32::try_from(index).unwrap_or(u32::MAX) + 1,
            indent: indent_of(text),
            text: text.trim().to_string(),
        })
        .collect();

    let mut index = 0;
    block(&lines, &mut index, None)
}

/// One source line, measured.
struct Line {
    number: u32,
    indent: usize,
    text: String,
}

/// Reads a block: every line indented deeper than the line that opened it.
fn block(lines: &[Line], index: &mut usize, parent: Option<usize>) -> Vec<Node> {
    let mut nodes = Vec::new();

    while *index < lines.len() {
        let line = &lines[*index];

        // A blank line belongs to whoever is reading it, and ends nothing: a block that a
        // blank line sits inside is still one block.
        if line.text.is_empty() {
            nodes.push(Node {
                line: line.number,
                text: String::new(),
                kind: Kind::Blank,
                children: Vec::new(),
            });
            *index += 1;
            continue;
        }

        if parent.is_some_and(|parent| line.indent <= parent) {
            break;
        }

        let indent = line.indent;
        let kind = classify(&line.text);
        let node = Node {
            line: line.number,
            text: line.text.clone(),
            kind,
            children: Vec::new(),
        };
        *index += 1;

        // A line's block belongs to it whatever it *is*: indentation is the structure in Ren'Py,
        // and a reader that only collected children for the kinds it recognized would leak every
        // indented line of a `style` block into the enclosing file.
        //
        // A comment is the exception, because an indented comment after a comment belongs to
        // whatever block the comments sit in rather than to the comment above it.
        let opens = !matches!(node.kind, Kind::Comment(_));

        // The next line that is *not blank* decides whether this one opens a block. A blank line
        // inside a body is ordinary — Ren'Py's own screens put one after `if` — and treating it as
        // the end of the block sent the rest of the body to the enclosing file, which is how one
        // 1,500-line `screens.rpy` reported a line per statement instead of one entry for the file.
        let deeper = lines[*index..]
            .iter()
            .find(|line| !line.text.is_empty())
            .is_some_and(|line| line.indent > indent);

        let children = if opens && deeper {
            block(lines, index, Some(indent))
        } else {
            Vec::new()
        };

        nodes.push(Node { children, ..node });
    }

    nodes
}

/// How far a line is indented, counting a tab as four.
fn indent_of(text: &str) -> usize {
    text.chars()
        .take_while(|character| character.is_whitespace() && *character != '\n')
        .map(|character| if character == '\t' { 4 } else { 1 })
        .sum()
}

/// What a trimmed line is.
fn classify(text: &str) -> Kind {
    if let Some(rest) = text.strip_prefix('#') {
        return Kind::Comment(rest.to_string());
    }

    // Block openers, before anything else: a `label` line can contain a string.
    for (prefix, wrap) in [("label ", 0u8), ("if ", 1), ("elif ", 1)] {
        if let Some(rest) = text.strip_prefix(prefix)
            && let Some(inner) = rest.strip_suffix(':')
        {
            return match wrap {
                0 => Kind::Label(inner.trim().to_string()),
                1 if prefix == "if " => Kind::If(inner.trim().to_string()),
                _ => Kind::Elif(inner.trim().to_string()),
            };
        }
    }
    match text {
        "menu:" => return Kind::Menu,
        "else:" => return Kind::Else,
        "return" => return Kind::Return,
        _ => {}
    }

    for (prefix, make) in [
        ("jump ", Kind::Jump as fn(String) -> Kind),
        ("call ", Kind::Call),
        ("scene ", Kind::Scene),
        ("show ", Kind::Show),
        ("hide ", Kind::Hide),
        ("with ", Kind::With),
        ("play ", Kind::Play),
        ("stop ", Kind::Stop),
        ("queue ", Kind::Queue),
        ("pause ", Kind::Pause),
        ("pause", Kind::Pause),
        ("window ", Kind::Window),
        ("$ ", Kind::Python),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return make(rest.trim().to_string());
        }
    }

    for (prefix, definition) in [("define ", 0u8), ("default ", 1)] {
        if let Some(rest) = text.strip_prefix(prefix)
            && let Some((name, value)) = crate::expr::split_assignment(rest)
        {
            return if definition == 0 {
                Kind::Define { name, value }
            } else {
                Kind::Default { name, value }
            };
        }
    }

    // Last, a string: a say statement, or a choice when it ends in `:`.
    if let Some((before, literal, after)) = first_literal(text) {
        if let Some(rest) = after.strip_suffix(':') {
            let condition = rest.trim().strip_prefix("if ").map(str::to_string);
            return Kind::Choice {
                text: literal.to_string(),
                condition,
            };
        }

        return Kind::Say {
            speaker: before.trim().to_string(),
            text: literal.to_string(),
            transition: after
                .trim()
                .strip_prefix("with ")
                .map(|name| name.trim().to_string()),
        };
    }

    Kind::Unsupported
}

/// The first string literal on a line, and what surrounds it.
///
/// Scans rather than splitting on `"`, because a string may hold an escaped quote — which the
/// sample story does, in `s "Sure, but what's a \"visual novel?\""`.
fn first_literal(text: &str) -> Option<(&str, &str, &str)> {
    let start = text.find('"')?;
    let bytes = text.as_bytes();
    let mut index = start + 1;

    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => {
                return Some((&text[..start], &text[start..=index], &text[index + 1..]));
            }
            _ => index += 1,
        }
    }
    None
}
