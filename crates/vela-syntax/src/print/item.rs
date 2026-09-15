//! Printing a whole file, and the entry point (`TOOLING.md §3`).

use std::fmt;

use vela_diag::{Diagnostic, Severity};
use vela_span::FileId;

use crate::print::decl;
use crate::print::expr;
use crate::print::writer::Writer;
use crate::tree::{ConstDecl, DefaultDecl, EffectDecl, Item, Program, Type, UseDecl};

/// Why a file was not reformatted.
#[derive(Debug)]
pub struct NotFormatted {
    /// The syntax errors that stopped it.
    ///
    /// Carried rather than replaced with a message: a caller that asked for a format is about to
    /// tell its user why it did not happen, and the reason is a diagnostic with a span in it.
    pub diagnostics: Vec<Diagnostic>,
}

impl fmt::Display for NotFormatted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the file has {} syntax error(s), so it was not reformatted",
            self.diagnostics.len()
        )
    }
}

impl std::error::Error for NotFormatted {}

/// Formats a whole file, or refuses to and says what stopped it.
///
/// Refusing is the whole safety argument. The tree keeps an `Error` node wherever something was
/// expected, so a file that does not parse has *gaps* in it — and a formatter that printed the gaps
/// would not be preserving the file, it would be replacing the unparsable part with nothing. A file
/// with a syntax error is reformatted after the error is fixed, not around it.
///
/// Deterministic, total, and defined by the tree alone: nothing here reads the original layout, so
/// two files with the same tree format to the same text, and formatting twice is formatting once.
///
/// # Errors
///
/// Every lexical and syntactic diagnostic is a refusal; warnings from later phases are not, because
/// this crate never sees them.
pub fn format(file: FileId, src: &str) -> Result<String, NotFormatted> {
    let parsed = crate::parse(file, src);
    let errors: Vec<Diagnostic> = parsed
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .collect();
    if !errors.is_empty() {
        return Err(NotFormatted {
            diagnostics: errors,
        });
    }

    let mut writer = Writer::new(src, &parsed.program.comments);
    items(&mut writer, &parsed.program);
    Ok(writer.finish())
}

/// Writes every top-level item, and any comment that follows the last one.
fn items(writer: &mut Writer<'_>, program: &Program) {
    for declaration in &program.items {
        writer.comments_until(declaration.span().start());
        writer.blank_before(declaration.span().start());
        item(writer, declaration);
    }
    // A file can end with a comment, and a formatter that dropped it would be deleting the last
    // thing its author wrote.
    writer.comments_until(u32::MAX);
}

/// Writes one top-level item.
fn item(writer: &mut Writer<'_>, item: &Item) {
    let span = item.span();
    if writer.is_off(span.start()) {
        writer.verbatim(span);
        return;
    }

    match item {
        Item::Use(decl) => use_(writer, decl),
        Item::Effect(decl) => effect(writer, decl),
        Item::Const(decl) => constant(writer, decl),
        Item::Default(decl) => default(writer, decl),
        Item::Image(decl) => writer.line(&format!(
            "image {} = {}",
            decl.name.join("."),
            expr::text(&decl.value)
        )),

        // The body is an animation, which the parser consumes without parsing (M13). Reproduced as
        // written, header and all: this is the one construct the formatter does not lay out, because
        // it is the one construct nothing here understands.
        Item::Transform(decl) => writer.verbatim(decl.span),

        Item::Struct(decl) => decl::struct_(writer, decl),
        Item::Enum(decl) => decl::enum_(writer, decl),
        Item::Character(decl) => decl::character(writer, decl),
        Item::Style(decl) => decl::style(writer, decl),
        Item::Theme(decl) => decl::theme(writer, decl),
        Item::Screen(decl) => decl::screen(writer, decl),
        Item::Function(decl) => decl::function(writer, decl),
        Item::Label(decl) => decl::label(writer, decl),

        // Unreachable in a file that parses; `format` refuses one that does not.
        Item::Error { .. } => {}
    }

    // Where the item ended, so the next one measures its blank line from here.
    writer.note(if decl::opens_a_body(item) {
        span.start()
    } else {
        span.end()
    });
}

/// A `use`, with its alias if it has one.
fn use_(writer: &mut Writer<'_>, decl: &UseDecl) {
    let mut text = format!("use {}", decl.path.join("."));
    if let Some(alias) = &decl.alias {
        text.push_str(&format!(" as {alias}"));
    }
    writer.line(&text);
}

/// An `effect`: a name, its parameters, and what it returns.
fn effect(writer: &mut Writer<'_>, decl: &EffectDecl) {
    let mut text = format!(
        "effect {}({})",
        decl.dotted(),
        expr::params_text(&decl.params)
    );
    if let Some(ret) = &decl.ret {
        text.push_str(&format!(" -> {}", expr::type_text(ret)));
    }
    writer.line(&text);
}

/// A `const`.
fn constant(writer: &mut Writer<'_>, decl: &ConstDecl) {
    writer.line(&format!(
        "const {}{} = {}",
        decl.name,
        annotation(decl.ty.as_ref()),
        expr::text(&decl.value)
    ));
}

/// A `default`.
fn default(writer: &mut Writer<'_>, decl: &DefaultDecl) {
    writer.line(&format!(
        "default {}{} = {}",
        decl.name,
        annotation(decl.ty.as_ref()),
        expr::text(&decl.value)
    ));
}

/// A written type annotation, if there is one.
fn annotation(ty: Option<&Type>) -> String {
    ty.map_or(String::new(), |ty| format!(": {}", expr::type_text(ty)))
}
