//! Printing the declarations that own a body: types, screens, characters, and bodies of code.
//!
//! Apart from the declarations a line is enough for, because that is the division that decides how
//! a line ends: each of these writes a header, records where the header ended — so a comment on it
//! stays on it — and hands the rest to a block printer.

use crate::print::block;
use crate::print::expr;
use crate::print::screen;
use crate::print::writer::Writer;
use crate::tree::{
    CharacterDecl, EnumDecl, FnDecl, Item, LabelDecl, ScreenDecl, Setting, StructDecl, StyleDecl,
    ThemeDecl,
};

/// Writes a `struct` and its fields.
pub(crate) fn struct_(writer: &mut Writer<'_>, decl: &StructDecl) {
    writer.line(&format!("struct {}:", decl.name));
    writer.note(decl.span.start());

    writer.level_up();
    for field in &decl.fields {
        writer.comments_until(field.span.start());
        writer.blank_before(field.span.start());
        let default = field
            .default
            .as_ref()
            .map_or(String::new(), |value| format!(" = {}", expr::text(value)));
        writer.line(&format!(
            "{}: {}{default}",
            field.name,
            expr::type_text(&field.ty)
        ));
        writer.note(field.span.end());
    }
    writer.level_down();
}

/// Writes an `enum` and its variants.
pub(crate) fn enum_(writer: &mut Writer<'_>, decl: &EnumDecl) {
    writer.line(&format!("enum {}:", decl.name));
    writer.note(decl.span.start());

    writer.level_up();
    for variant in &decl.variants {
        writer.comments_until(variant.span.start());
        writer.blank_before(variant.span.start());
        let fields = if variant.fields.is_empty() {
            String::new()
        } else {
            format!("({})", expr::params_text(&variant.fields))
        };
        writer.line(&format!("{}{fields}", variant.name));
        writer.note(variant.span.end());
    }
    writer.level_down();
}

/// Writes a `character` and its settings.
pub(crate) fn character(writer: &mut Writer<'_>, decl: &CharacterDecl) {
    writer.line(&format!("character {}:", decl.name));
    writer.note(decl.span.start());
    settings(writer, &decl.settings);
}

/// Writes a `style`, including what it inherits from.
pub(crate) fn style(writer: &mut Writer<'_>, decl: &StyleDecl) {
    let from = decl
        .from
        .as_ref()
        .map_or(String::new(), |base| format!(" from {base}"));
    writer.line(&format!("style {}{from}:", decl.name));
    writer.note(decl.span.start());
    settings(writer, &decl.settings);
}

/// Writes a `theme` and its settings.
pub(crate) fn theme(writer: &mut Writer<'_>, decl: &ThemeDecl) {
    writer.line(&format!("theme {}:", decl.name));
    writer.note(decl.span.start());
    settings(writer, &decl.settings);
}

/// Writes a `screen` and its widget tree.
pub(crate) fn screen(writer: &mut Writer<'_>, decl: &ScreenDecl) {
    // Parentheses only when there is something in them: a screen with no parameters reads better
    // without an empty pair, and both spellings parse (`TOOLING.md §3`).
    let params = if decl.params.is_empty() {
        String::new()
    } else {
        format!("({})", expr::params_text(&decl.params))
    };
    writer.line(&format!("screen {}{params}:", decl.name));
    writer.note(decl.span.start());
    screen::body(writer, &decl.body);
}

/// Writes a `fn` and its body.
pub(crate) fn function(writer: &mut Writer<'_>, decl: &FnDecl) {
    let ret = decl
        .ret
        .as_ref()
        .map_or(String::new(), |ret| format!(" -> {}", expr::type_text(ret)));
    writer.line(&format!(
        "fn {}({}){ret}:",
        decl.name,
        expr::params_text(&decl.params)
    ));
    writer.note(decl.span.start());
    block::block(writer, &decl.body);
}

/// Writes a `label` and its body.
pub(crate) fn label(writer: &mut Writer<'_>, decl: &LabelDecl) {
    writer.line(&format!("label {}:", decl.name));
    writer.note(decl.span.start());
    block::block(writer, &decl.body);
}

/// Writes a `key = value` block, as characters, styles, and themes use.
///
/// The type word is written back if the declaration had one — `color bg = 0x10121a` is a colour and
/// `space sm = 4` is a length, and the two are the same shape without it (`SCREENS.md §5`).
fn settings(writer: &mut Writer<'_>, settings: &[Setting]) {
    writer.level_up();
    for setting in settings {
        writer.comments_until(setting.span.start());
        writer.blank_before(setting.span.start());
        let ty = setting
            .ty
            .as_ref()
            .map_or(String::new(), |ty| format!("{ty} "));
        writer.line(&format!(
            "{ty}{} = {}",
            setting.key,
            expr::text(&setting.value)
        ));
        writer.note(setting.span.end());
    }
    writer.level_down();
}

/// Whether an item's span ends at a `Dedent` rather than at a token of its own.
///
/// The answer decides which end is worth recording: see `stmt::opens_a_body` for why the end of a
/// block cannot be used to measure the gap after it.
pub(crate) fn opens_a_body(item: &Item) -> bool {
    matches!(
        item,
        Item::Struct(_)
            | Item::Enum(_)
            | Item::Character(_)
            | Item::Style(_)
            | Item::Theme(_)
            | Item::Screen(_)
            | Item::Function(_)
            | Item::Label(_)
    )
}
