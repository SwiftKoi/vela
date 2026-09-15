//! Token kinds, reserved words, and the token value itself.

use vela_span::Span;

/// A reserved word.
///
/// The language has no contextual keywords — everything here is reserved
/// unconditionally, so the parser never has to guess what a name means. The list is
/// normative and matches `docs/spec/LANGUAGE.md §2`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Keyword {
    /// `and`
    And,
    /// `as`
    As,
    /// `at`
    At,
    /// `call`
    Call,
    /// `character`
    Character,
    /// `const`
    Const,
    /// `default`
    Default,
    /// `effect`
    Effect,
    /// `elif`
    Elif,
    /// `else`
    Else,
    /// `enum`
    Enum,
    /// `false`
    False,
    /// `fn`
    Fn,
    /// `for`
    For,
    /// `from`
    From,
    /// `hide`
    Hide,
    /// `if`
    If,
    /// `image`
    Image,
    /// `import`
    Import,
    /// `in`
    In,
    /// `is`
    Is,
    /// `jump`
    Jump,
    /// `label`
    Label,
    /// `match`
    Match,
    /// `menu`
    Menu,
    /// `none`
    None,
    /// `not`
    Not,
    /// `or`
    Or,
    /// `pause`
    Pause,
    /// `play`
    Play,
    /// `queue`
    Queue,
    /// `return`
    Return,
    /// `scene`
    Scene,
    /// `screen`
    Screen,
    /// `show`
    Show,
    /// `stop`
    Stop,
    /// `struct`
    Struct,
    /// `style`
    Style,
    /// `theme`
    Theme,
    /// `transform`
    Transform,
    /// `true`
    True,
    /// `use`
    Use,
    /// `var`
    Var,
    /// `wait`
    Wait,
    /// `when`
    When,
    /// `while`
    While,
    /// `with`
    With,
}

impl Keyword {
    /// Every keyword with its spelling, sorted so `from_str` can binary-search it.
    ///
    /// This is the single source of truth: `as_str` and `from_str` both read it, so a
    /// keyword cannot be recognised but unprintable, or vice versa. A test asserts the
    /// table stays sorted.
    pub const ALL: &'static [(Self, &'static str)] = &[
        (Self::And, "and"),
        (Self::As, "as"),
        (Self::At, "at"),
        (Self::Call, "call"),
        (Self::Character, "character"),
        (Self::Const, "const"),
        (Self::Default, "default"),
        (Self::Effect, "effect"),
        (Self::Elif, "elif"),
        (Self::Else, "else"),
        (Self::Enum, "enum"),
        (Self::False, "false"),
        (Self::Fn, "fn"),
        (Self::For, "for"),
        (Self::From, "from"),
        (Self::Hide, "hide"),
        (Self::If, "if"),
        (Self::Image, "image"),
        (Self::Import, "import"),
        (Self::In, "in"),
        (Self::Is, "is"),
        (Self::Jump, "jump"),
        (Self::Label, "label"),
        (Self::Match, "match"),
        (Self::Menu, "menu"),
        (Self::None, "none"),
        (Self::Not, "not"),
        (Self::Or, "or"),
        (Self::Pause, "pause"),
        (Self::Play, "play"),
        (Self::Queue, "queue"),
        (Self::Return, "return"),
        (Self::Scene, "scene"),
        (Self::Screen, "screen"),
        (Self::Show, "show"),
        (Self::Stop, "stop"),
        (Self::Struct, "struct"),
        (Self::Style, "style"),
        (Self::Theme, "theme"),
        (Self::Transform, "transform"),
        (Self::True, "true"),
        (Self::Use, "use"),
        (Self::Var, "var"),
        (Self::Wait, "wait"),
        (Self::When, "when"),
        (Self::While, "while"),
        (Self::With, "with"),
    ];

    /// Recognises a spelling, or returns `None` if it is an ordinary identifier.
    ///
    /// Named `lookup` rather than `from_str` because it is infallible-by-`Option` and
    /// would otherwise shadow the `FromStr` trait method with a different signature.
    #[must_use]
    pub fn lookup(text: &str) -> Option<Self> {
        Self::ALL
            .binary_search_by(|(_, spelling)| (*spelling).cmp(text))
            .ok()
            .map(|index| Self::ALL[index].0)
    }

    /// The keyword's spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(keyword, _)| *keyword == self)
            .map_or("", |(_, spelling)| spelling)
    }
}

/// What a token is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TokenKind {
    /// An identifier: a name that is not a reserved word.
    Ident,
    /// A reserved word.
    Keyword(Keyword),
    /// An integer literal.
    Int,
    /// A floating-point literal.
    Float,
    /// A string literal, including its surrounding quotes.
    Str,
    /// A path literal: `@"assets/forest.png"`.
    Path,

    /// `=`
    Eq,
    /// `==`
    EqEq,
    /// `!=`
    BangEq,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `+=`
    PlusEq,
    /// `-=`
    MinusEq,
    /// `*=`
    StarEq,
    /// `/=`
    SlashEq,
    /// `->`
    Arrow,
    /// `=>`
    FatArrow,
    /// `?`
    Question,
    /// `??`
    QuestionQuestion,
    /// `!`
    Bang,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// `:`
    Colon,

    /// A statement-terminating line break.
    Newline,
    /// The start of an indented block.
    Indent,
    /// The end of an indented block.
    Dedent,
    /// End of input.
    Eof,
}

impl TokenKind {
    /// Whether this token opens a bracket group, inside which newlines are ignored.
    #[must_use]
    pub fn opens_bracket(self) -> bool {
        matches!(self, Self::LParen | Self::LBracket | Self::LBrace)
    }

    /// Whether this token closes a bracket group.
    #[must_use]
    pub fn closes_bracket(self) -> bool {
        matches!(self, Self::RParen | Self::RBracket | Self::RBrace)
    }

    /// Whether this token can never appear inside an expression, and so marks the end
    /// of a statement for error recovery.
    #[must_use]
    pub fn ends_statement(self) -> bool {
        matches!(self, Self::Newline | Self::Dedent | Self::Eof)
    }

    /// A short description for diagnostics, e.g. ``expected an expression``.
    ///
    /// Keywords and literals report their kind rather than their text; a caller with
    /// access to the source should prefer quoting the actual text.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::Ident => "an identifier",
            Self::Keyword(_) => "a keyword",
            Self::Int => "an integer",
            Self::Float => "a number",
            Self::Str => "a string",
            Self::Path => "a path literal",
            Self::Newline => "the end of a line",
            Self::Indent => "an indent",
            Self::Dedent => "a dedent",
            Self::Eof => "the end of the file",
            _ => "an operator",
        }
    }
}

/// A token: what it is, and where it came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Token {
    /// The token's kind.
    pub kind: TokenKind,
    /// The bytes it covers, including quotes for string literals.
    pub span: Span,
}

impl Token {
    /// Creates a token.
    #[must_use]
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}
