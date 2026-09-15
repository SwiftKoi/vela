//! Printing statement lists, and the statements that open one.
//!
//! Together in one file because they are one question: where a body starts, what is in it, and
//! which of the answers above it means the body can end early.

use crate::print::expr;
use crate::print::writer::Writer;
use crate::tree::{Expr, ForStmt, IfStmt, MatchStmt, MenuChoice, MenuStmt, Pattern, WhileStmt};

/// Writes a list of statements at the current indentation.
pub(crate) fn each(writer: &mut Writer<'_>, statements: &[crate::tree::Stmt]) {
    for statement in statements {
        let start = statement.span().start();
        writer.comments_until(start);
        writer.blank_before(start);
        super::stmt::write(writer, statement);
    }
}

/// Writes an indented block.
pub(crate) fn block(writer: &mut Writer<'_>, statements: &[crate::tree::Stmt]) {
    writer.level_up();
    each(writer, statements);
    writer.level_down();
}

/// Writes an `if`, with its `elif`s and its `else`.
pub(crate) fn if_(writer: &mut Writer<'_>, stmt: &IfStmt) {
    writer.line(&format!("if {}:", expr::text(&stmt.condition)));
    writer.note(stmt.condition.span().end());
    block(writer, &stmt.then_body);

    for clause in &stmt.elifs {
        writer.comments_until(clause.span.start());
        writer.blank_before(clause.span.start());
        writer.line(&format!("elif {}:", expr::text(&clause.condition)));
        writer.note(clause.condition.span().end());
        block(writer, &clause.body);
    }

    if let Some(body) = &stmt.else_body {
        // A comment between the last branch and the `else` belongs to the `else`, so it is written
        // before the keyword rather than after it.
        if let Some(first) = body.first() {
            writer.comments_until(first.span().start());
        }
        writer.line("else:");
        block(writer, body);
    }
}

/// Writes a `while`.
pub(crate) fn while_(writer: &mut Writer<'_>, stmt: &WhileStmt) {
    writer.line(&format!("while {}:", expr::text(&stmt.condition)));
    writer.note(stmt.condition.span().end());
    block(writer, &stmt.body);
}

/// Writes a `for`.
pub(crate) fn for_(writer: &mut Writer<'_>, stmt: &ForStmt) {
    writer.line(&format!(
        "for {} in {}:",
        stmt.binding,
        expr::text(&stmt.iterable)
    ));
    writer.note(stmt.iterable.span().end());
    block(writer, &stmt.body);
}

/// Writes a `match` and its arms.
pub(crate) fn match_(writer: &mut Writer<'_>, stmt: &MatchStmt) {
    writer.line(&format!("match {}:", expr::text(&stmt.scrutinee)));
    writer.note(stmt.scrutinee.span().end());

    writer.level_up();
    for arm in &stmt.arms {
        writer.comments_until(arm.span.start());
        writer.blank_before(arm.span.start());
        writer.line(&arm_header(arm.pattern.as_ref(), arm.guard.as_ref()));
        writer.note(arm.span.start());
        block(writer, &arm.body);
    }
    writer.level_down();
}

/// Writes a `menu` and its choices.
pub(crate) fn menu(writer: &mut Writer<'_>, menu: &MenuStmt) {
    match &menu.prompt {
        Some(prompt) => writer.line(&format!("menu {}:", expr::text(prompt))),
        None => writer.line("menu:"),
    }
    writer.note(menu.span.start());

    writer.level_up();
    for choice in &menu.choices {
        writer.comments_until(choice.span.start());
        writer.blank_before(choice.span.start());
        writer.line(&choice_header(choice));
        // The header ends with the choice's text, which is where a trailing comment hangs off.
        writer.note(choice.text.span().end());
        block(writer, &choice.body);
    }
    writer.level_down();
}

/// A choice's header line, with its guard if it has one.
fn choice_header(choice: &MenuChoice) -> String {
    match &choice.condition {
        Some(condition) => format!("{} if {}:", expr::text(&choice.text), expr::text(condition)),
        None => format!("{}:", expr::text(&choice.text)),
    }
}

/// A match arm's header line. `else` is an absent pattern, and `_` an empty one.
fn arm_header(pattern: Option<&Pattern>, guard: Option<&Expr>) -> String {
    let Some(pattern) = pattern else {
        return "else:".to_string();
    };
    match guard {
        Some(guard) => format!("when {} if {}:", expr::pattern(pattern), expr::text(guard)),
        None => format!("when {}:", expr::pattern(pattern)),
    }
}
