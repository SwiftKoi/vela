//! Printing a screen body: a widget tree by indentation (`SCREENS.md §2`).

use crate::print::expr;
use crate::print::writer::Writer;
use crate::tree::{ScreenArg, ScreenLine, ScreenNode};

/// Writes a screen body, indented one level.
pub(crate) fn body(writer: &mut Writer<'_>, lines: &[ScreenLine]) {
    writer.level_up();
    if lines.is_empty() {
        // `pass` is the language's own empty statement, and the parser *drops* it — so a body that
        // contained one arrives here empty. Writing nothing would produce a screen with no block,
        // which does not parse: `E1002`.
        writer.line("pass");
    }
    each(writer, lines);
    writer.level_down();
}

/// Writes a list of body lines at the current indentation.
fn each(writer: &mut Writer<'_>, lines: &[ScreenLine]) {
    for line in lines {
        let start = start_of(line);
        writer.comments_until(start);
        writer.blank_before(start);
        write(writer, line);
    }
}

/// Writes one body line.
fn write(writer: &mut Writer<'_>, line: &ScreenLine) {
    match line {
        ScreenLine::Layer { name, .. } => writer.line(&format!("layer {name}")),
        ScreenLine::StylePrefix { name, .. } => writer.line(&format!("style_prefix {name}")),
        ScreenLine::If { .. } | ScreenLine::For { .. } => nested(writer, line),
        ScreenLine::Use {
            span,
            name,
            args,
            body: inner,
        } => {
            let arguments = if args.is_empty() {
                String::new()
            } else {
                let rendered: Vec<String> = args.iter().map(arg_text).collect();
                format!("({})", rendered.join(", "))
            };
            let text = format!("use {name}{arguments}");
            if inner.is_empty() {
                writer.line(&text);
            } else {
                writer.line(&format!("{text}:"));
                writer.note(span.start());
                body(writer, inner);
            }
        }
        // A bare line, like `pass`: nothing follows it, and nothing can.
        ScreenLine::Transclude { .. } => writer.line("transclude"),
        ScreenLine::Default { name, value, .. } => {
            writer.line(&format!("default {name} = {}", expr::text(value)));
        }
        ScreenLine::Key { name, action, .. } => {
            writer.line(&format!("key {name} action {}", expr::text(action)));
        }
        ScreenLine::Timer {
            seconds,
            action,
            repeat,
            ..
        } => {
            // `repeat` before `action`, which is the canonical order the parser reads — a binding
            // written the other way round would come back in this one.
            let again = if *repeat { " repeat" } else { "" };
            writer.line(&format!(
                "timer {}{again} action {}",
                expr::text(seconds),
                expr::text(action)
            ));
        }
        ScreenLine::Node(declared) => node(writer, declared),
    }
}

/// A line that opens a block: a conditional's arms, or a loop's body.
///
/// Apart from the lines that open a block, because they are the only ones that print *more than one*
/// body — an `if` writes its arms one after another, each at the same indentation.
fn nested(writer: &mut Writer<'_>, line: &ScreenLine) {
    match line {
        // Children from data: the same shape the statement form prints (`LANGUAGE.md §3`).
        ScreenLine::For {
            binding,
            iterable,
            body: inner,
            ..
        } => {
            writer.line(&format!("for {binding} in {}:", expr::text(iterable)));
            body(writer, inner);
        }
        ScreenLine::If {
            span,
            condition,
            body: inner,
            elifs,
            else_body,
        } => {
            writer.line(&format!("if {}:", expr::text(condition)));
            writer.note(span.start());
            body(writer, inner);

            for clause in elifs {
                writer.comments_until(clause.span.start());
                writer.blank_before(clause.span.start());
                writer.line(&format!("elif {}:", expr::text(&clause.condition)));
                writer.note(clause.span.start());
                body(writer, &clause.body);
            }

            if let Some(else_body) = else_body {
                // A comment between the last arm and the `else` belongs to the `else`, so it is
                // written before the keyword rather than after it — the same rule the statement
                // form's printer follows.
                if let Some(first) = else_body.first() {
                    writer.comments_until(start_of(first));
                }
                writer.line("else:");
                body(writer, else_body);
            }
        }
        // Neither: `write` sends only the two above here.
        _ => {}
    }
}

/// Writes one widget or prop line, and its children if it has any.
fn node(writer: &mut Writer<'_>, node: &ScreenNode) {
    let mut text = node.name.clone();
    if !node.args.is_empty() {
        let args: Vec<String> = node.args.iter().map(arg_text).collect();
        // Separated by commas, always. A comma cannot begin an expression, which is what stops a
        // bare flag from swallowing the arg after it: `box stretch_x stretch_y` reads as
        // `stretch_x = stretch_y`, while `box stretch_x, stretch_y` reads as two flags. Both
        // spellings parse, so this is the formatter deciding which one is canonical — and the
        // round-trip test is what noticed it had to.
        text.push(' ');
        text.push_str(&args.join(", "));
    }

    if node.children.is_empty() {
        writer.line(&text);
        return;
    }
    writer.line(&format!("{text}:"));
    writer.note(node.span.start());
    body(writer, &node.children);
}

/// One word or value after a widget's name.
fn arg_text(arg: &ScreenArg) -> String {
    match arg {
        ScreenArg::Value(value) => expr::text(value),
        // A bare name is a flag, and stays one.
        ScreenArg::Named {
            name, value: None, ..
        } => name.clone(),
        // A named value is written with its `=`. Without it, `text line` and `gap 8` are the same
        // shape to the parser, and a value that is itself a bare name would split into two args on
        // the way back in.
        ScreenArg::Named {
            name,
            value: Some(value),
            ..
        } => format!("{name} = {}", expr::text(value)),
    }
}

/// Where a body line starts.
fn start_of(line: &ScreenLine) -> u32 {
    match line {
        ScreenLine::Layer { span, .. }
        | ScreenLine::StylePrefix { span, .. }
        | ScreenLine::If { span, .. }
        | ScreenLine::For { span, .. }
        | ScreenLine::Use { span, .. }
        | ScreenLine::Key { span, .. }
        | ScreenLine::Timer { span, .. }
        | ScreenLine::Default { span, .. }
        | ScreenLine::Transclude { span } => span.start(),
        ScreenLine::Node(node) => node.span.start(),
    }
}
