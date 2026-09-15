//! The diagnostic value types and their builder.

use vela_span::Span;

use crate::{Code, Severity};

/// A message attached to a span.
#[derive(Debug, Clone)]
pub struct Label {
    /// Where the label points.
    pub span: Span,
    /// What is being pointed out.
    pub message: String,
}

/// A machine-applicable fix.
///
/// Kept separate from the human-facing `help` text: an editor can apply this without
/// parsing prose, and a language server can offer it as a code action.
#[derive(Debug, Clone)]
pub struct Suggestion {
    /// The text to replace.
    pub span: Span,
    /// What to replace it with.
    pub replacement: String,
}

/// A single diagnostic.
///
/// The primary span is not optional. A diagnostic that cannot point at anything is a
/// log line, not a diagnostic, and treating the two the same is how error messages
/// become useless.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The stable code. Determines severity, so severity is never set by hand.
    pub code: Code,
    /// The headline, without the code (the renderer adds it).
    pub message: String,
    /// The span the diagnostic is primarily about.
    pub primary: Label,
    /// Additional labelled spans.
    pub secondary: Vec<Label>,
    /// Footer notes, rendered as `= note: ...`.
    pub notes: Vec<String>,
    /// Human-facing advice, rendered as `= help: ...`.
    pub help: Option<String>,
    /// A machine-applicable fix.
    pub suggestion: Option<Suggestion>,
}

impl Diagnostic {
    /// Creates a diagnostic with a primary label.
    #[must_use]
    pub fn new(
        code: Code,
        message: impl Into<String>,
        span: Span,
        label: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            primary: Label {
                span,
                message: label.into(),
            },
            secondary: Vec::new(),
            notes: Vec::new(),
            help: None,
            suggestion: None,
        }
    }

    /// Adds a labelled secondary span.
    #[must_use]
    pub fn with_secondary(mut self, span: Span, message: impl Into<String>) -> Self {
        self.secondary.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    /// Adds a footer note.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Adds human-facing advice.
    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Attaches a machine-applicable fix.
    #[must_use]
    pub fn with_suggestion(mut self, span: Span, replacement: impl Into<String>) -> Self {
        self.suggestion = Some(Suggestion {
            span,
            replacement: replacement.into(),
        });
        self
    }

    /// The severity, taken from the code's registry entry.
    #[must_use]
    pub fn severity(&self) -> Severity {
        self.code.severity()
    }
}
