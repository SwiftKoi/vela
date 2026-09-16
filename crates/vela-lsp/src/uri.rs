//! File URIs, and the names a session knows files by.
//!
//! The protocol names a document by URI; a session names a file by a *path*, because a module's name
//! comes from its path (`LANGUAGE.md §6`) — so the two have to be translated, in both directions, and
//! a document that arrives as a URI has to become the same name the command line would have given it.
//! Otherwise the editor and `vela check` would be looking at two different files that happen to hold
//! the same text.
//!
//! Only `file:` URIs are served: this server reads the workspace it was started in, and a URI for
//! something else is refused rather than guessed at.

/// The path a `file:` URI names, percent-decoded, or `None` when it is not one we can read.
#[must_use]
pub fn path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    // `file:///path` is local; `file://host/path` names a host's filesystem, and this server has no
    // way to answer whether it means one. Refused rather than half-supported.
    if !rest.starts_with('/') {
        return None;
    }
    Some(decode(rest))
}

/// The name a session knows a file by: its path relative to the workspace root when it is under it.
///
/// The same rule the command line uses — a name is what a module's dotted name is derived from — so a
/// document opened by an editor and a file read by `vela check` are the *same* file in the session.
#[must_use]
pub fn name(root: &str, path: &str) -> String {
    let root = root.trim_end_matches('/');
    match path.strip_prefix(root) {
        Some(rest) => rest.trim_start_matches('/').to_string(),
        None => path.to_string(),
    }
}

/// Percent-decodes a URI path, leaving anything that is not a valid escape as written.
///
/// Lenient on purpose: a stray `%` in a filename is a file that exists, and refusing the whole document
/// over it would be a server that cannot open a file the filesystem is happy with.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        let decoded = (bytes[index] == b'%')
            .then(|| hex_pair(bytes.get(index + 1)?, bytes.get(index + 2)?))
            .flatten();

        match decoded {
            Some(byte) => {
                out.push(byte);
                index += 3;
            }
            None => {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The byte two hex digits name.
///
/// `try_from` because four bits from each digit can exceed a byte — `%FF` is 255 and `%100` is not a
/// byte at all — and a pair that does not fit is not a valid escape rather than a wrapped one.
fn hex_pair(high: &u8, low: &u8) -> Option<u8> {
    let digit = |byte: &u8| (*byte as char).to_digit(16);
    u8::try_from(digit(high)? * 16 + digit(low)?).ok()
}
