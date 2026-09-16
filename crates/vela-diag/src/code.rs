//! Stable diagnostic codes and their registry.

use std::fmt;
use std::sync::OnceLock;

/// The registry file, embedded so lookups never touch the filesystem at runtime.
///
/// It sits at the crate root rather than in `src/` because it is data, not code — and
/// because `check-diag-codes` validates it without linking this crate.
const CODES: &str = include_str!("../codes.txt");

/// A diagnostic's severity. Derived from the code's letter, never set independently,
/// so a code cannot be an error in one place and a warning in another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The build fails.
    Error,
    /// The build succeeds; CI with `--deny-warnings` fails.
    Warning,
    /// A style or hygiene suggestion.
    Lint,
}

impl Severity {
    /// Lowercase name, as it appears in rendered output.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Lint => "lint",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "error" => Some(Self::Error),
            "warning" => Some(Self::Warning),
            "lint" => Some(Self::Lint),
            _ => None,
        }
    }
}

/// A stable diagnostic code, such as `E5003`.
///
/// Stored as its five ASCII bytes: `Code` is `Copy` and infallible to pass around,
/// and it cannot be constructed except through the registry, so an unregistered code
/// is unrepresentable rather than merely discouraged.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Code([u8; 5]);

impl Code {
    /// Looks up a code by name, returning `None` if it is not registered.
    ///
    /// The `None` case is deliberate: a diagnostic with an invented code is a bug, and
    /// the type system should make that bug visible at the call site.
    #[must_use]
    pub fn new(name: &str) -> Option<Self> {
        let bytes: [u8; 5] = name.as_bytes().try_into().ok()?;
        if !well_formed(bytes) {
            return None;
        }
        registry().get(bytes).map(|_| Self(bytes))
    }

    /// The code as it appears in output.
    ///
    /// The registry only admits ASCII digits and one leading letter, so this is always
    /// valid UTF-8.
    #[must_use]
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("E9999")
    }

    /// The severity declared by the registry.
    #[must_use]
    pub fn severity(self) -> Severity {
        registry()
            .get(self.0)
            .map_or(Severity::Error, |entry| entry.severity)
    }

    /// The human-readable title declared by the registry.
    #[must_use]
    pub fn title(self) -> &'static str {
        registry()
            .get(self.0)
            .map_or("", |entry| entry.title.as_str())
    }
}

/// Every registered code, in the order the registry file lists them.
///
/// The order is the file's, which is grouped by phase — lexical first, then syntactic, and on up — because
/// that is the order a reference reads best in, and because a generator needs an answer that is stable
/// across runs rather than "whatever the map happened to give".
pub fn codes() -> impl Iterator<Item = Code> {
    registry().entries.iter().map(|entry| Code(entry.code))
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// One entry in the registry.
struct Entry {
    code: [u8; 5],
    severity: Severity,
    title: String,
}

/// The parsed registry.
struct Registry {
    entries: Vec<Entry>,
}

impl Registry {
    /// Parses `<code>|<severity>|<title>` lines, skipping blanks and comments.
    ///
    /// Malformed entries are skipped rather than fatal: `check-diag-codes` is the gate
    /// that rejects them, and a diagnostic library that panics is worse than one that
    /// reports an unknown code.
    fn parse(text: &str) -> Self {
        let mut entries = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split('|');
            let (Some(code), Some(severity), Some(title)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let Ok(code) = <[u8; 5]>::try_from(code.trim().as_bytes()) else {
                continue;
            };
            let Some(severity) = Severity::parse(severity.trim()) else {
                continue;
            };
            entries.push(Entry {
                code,
                severity,
                title: title.trim().to_string(),
            });
        }
        Self { entries }
    }

    fn get(&self, code: [u8; 5]) -> Option<&Entry> {
        self.entries.iter().find(|e| e.code == code)
    }
}

/// The registry, parsed once per process.
fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry::parse(CODES))
}

/// Whether `bytes` is `[EWL]` followed by four ASCII digits.
fn well_formed(bytes: [u8; 5]) -> bool {
    matches!(bytes[0], b'E' | b'W' | b'L') && bytes[1..].iter().all(u8::is_ascii_digit)
}
