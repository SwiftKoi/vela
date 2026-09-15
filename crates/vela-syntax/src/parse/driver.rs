//! The parse driver: lex a file, parse it, and order the diagnostics.

use vela_diag::Diagnostic;
use vela_span::{FileId, Span};

use crate::lex::lexer::lex;
use crate::lex::token::TokenKind;
use crate::parse::parser::Parser;
use crate::tree::Program;

/// A path literal found in a file, `@"art/forest.png"`.
///
/// Recorded as the file is parsed rather than found later by walking the tree, because the
/// lexer already had to recognize the token and the parser already had to build the node — a
/// second traversal would be a second definition of "where a path literal can appear", and the
/// two would drift the first time a statement gained one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PathRef {
    /// The path, with the `@` and the quotes removed.
    pub value: String,
    /// The literal's span, including the `@` and the quotes.
    pub span: Span,
}

/// Everything parsing produced.
#[derive(Debug)]
pub struct ParseResult {
    /// The syntax tree.
    pub program: Program,
    /// Lexical and syntactic diagnostics, in source order.
    pub diagnostics: Vec<Diagnostic>,
    /// Every path literal in the file, in source order.
    ///
    /// A *reference*, not a declaration: what it names is a build question, answered against
    /// the asset manifest (`BUILD_AND_ASSETS.md §9`).
    pub paths: Vec<PathRef>,
}

/// Lexes and parses a whole file.
///
/// Lexing runs to completion first. That is only possible because the lexer resolves
/// indentation into tokens, and it means the parser never has to look at whitespace.
#[must_use]
pub fn parse(file: FileId, src: &str) -> ParseResult {
    let lexed = lex(file, src);
    let tokens = lexed.tokens;
    let mut diagnostics = lexed.diagnostics;

    let mut parser = Parser::new(file, src, &tokens);
    let mut program = parser.parse_program();
    diagnostics.extend(parser.take_diagnostics());

    // The comments the lexer collected ride along with the tree. Attached here rather than built
    // by the parser: no rule of the grammar is written in terms of a comment, and a parser that
    // produced them would be one that had to step over them (`crate::Comment`).
    program.comments = lexed.comments;

    // Source order, so the first thing reported is the first thing wrong. Lexical and
    // syntactic diagnostics are produced by separate passes and would otherwise
    // interleave by pass rather than by position.
    diagnostics.sort_by_key(|diagnostic| diagnostic.primary.span.start());

    let paths = path_literals(&tokens, src);

    ParseResult {
        program,
        diagnostics,
        paths,
    }
}

/// Every path literal in a token stream, in source order.
///
/// The tokens are already in order and a path is a token of its own, so this is a filter
/// rather than a walk. A `@"..."` inside a comment or a string is not a path token and is not
/// collected, which a character scan for `@"` could not promise.
fn path_literals(tokens: &[crate::lex::token::Token], src: &str) -> Vec<PathRef> {
    tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::Path))
        .map(|token| {
            let text = src
                .get(token.span.start() as usize..token.span.end() as usize)
                .unwrap_or("");
            PathRef {
                value: crate::parse::expr::parse_path(text),
                span: token.span,
            }
        })
        .collect()
}
