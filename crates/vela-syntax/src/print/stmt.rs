//! Printing the statements a line is enough for (`LANGUAGE.md §1.6`).
//!
//! Statements that open a body live in `block.rs`; this is the rest — dialogue, staging, control
//! transfers, audio, and the assignments whose right-hand side the formatter may wrap.

use crate::print::block;
use crate::print::expr;
use crate::print::writer::{WIDTH, Writer};
use crate::tree::{
    AssignOp, AssignStmt, AudioKind, AudioStmt, CallStmt, Expr, ReturnStmt, SayStmt, StageKind,
    StageStmt, Stmt, VarStmt, WaitEvent, WaitStmt,
};

/// Writes one statement.
pub(crate) fn write(writer: &mut Writer<'_>, statement: &Stmt) {
    let span = statement.span();
    if writer.is_off(span.start()) {
        writer.verbatim(span);
        return;
    }

    match statement {
        Stmt::Say(decl) => say(writer, decl),
        Stmt::Menu(decl) => block::menu(writer, decl),
        Stmt::Jump(decl) => writer.line(&format!("jump {}", decl.target.join("."))),
        Stmt::Call(decl) => call(writer, decl),
        Stmt::Return(decl) => returned(writer, decl),
        Stmt::Stage(decl) => stage(writer, decl),
        Stmt::With(decl) => writer.line(&format!("with {}", decl.transition)),
        Stmt::Wait(decl) => wait(writer, decl),
        Stmt::Audio(decl) => audio(writer, decl),
        Stmt::If(decl) => block::if_(writer, decl),
        Stmt::While(decl) => block::while_(writer, decl),
        Stmt::For(decl) => block::for_(writer, decl),
        Stmt::Match(decl) => block::match_(writer, decl),
        Stmt::Var(decl) => var_(writer, decl),
        Stmt::Assign(decl) => assign(writer, decl),
        Stmt::Expr(decl) => writer.line(&expr::text(&decl.expr)),

        // Unreachable in a file that parses; `format` refuses one that does not.
        Stmt::Error { .. } => {}
    }

    // Where the statement ended, so the *next* one measures the gap from here.
    //
    // A statement that opens a body ends at a synthetic `Dedent`, which sits at the start of the
    // line *after* the block — so its end can be past a blank line and the next statement would
    // never see one. Such a statement notes its own start instead, and its last child is what has
    // really been written; a leaf statement ends at its own last token, which is exact.
    writer.note(if opens_a_body(statement) {
        span.start()
    } else {
        span.end()
    });
}

/// Whether a statement's span ends at a `Dedent` rather than at a token of its own.
fn opens_a_body(statement: &Stmt) -> bool {
    matches!(
        statement,
        Stmt::If(_) | Stmt::While(_) | Stmt::For(_) | Stmt::Match(_) | Stmt::Menu(_)
    )
}

/// Dialogue: a speaker, its attributes, the line, its options, and a transition.
fn say(writer: &mut Writer<'_>, say: &SayStmt) {
    let mut text = String::new();
    if let Some(speaker) = &say.speaker {
        text.push_str(speaker);
        text.push(' ');
    }
    for attribute in &say.attributes {
        text.push_str(attribute);
        text.push(' ');
    }
    text.push_str(&expr::text(&say.line));

    if !say.options.is_empty() {
        let options: Vec<String> = say
            .options
            .iter()
            .map(|(key, value)| format!("{key}={}", expr::text(value)))
            .collect();
        text.push_str(&format!(" ({})", options.join(", ")));
    }
    if let Some(transition) = &say.transition {
        text.push_str(&format!(" with {transition}"));
    }
    writer.line(&text);
}

/// A `call`, with the transition it may carry.
fn call(writer: &mut Writer<'_>, call: &CallStmt) {
    let mut text = format!("call {}", call.target.join("."));
    if let Some(transition) = &call.transition {
        text.push_str(&format!(" with {transition}"));
    }
    writer.line(&text);
}

/// A `return`, with its value if it has one.
fn returned(writer: &mut Writer<'_>, ret: &ReturnStmt) {
    match &ret.value {
        Some(value) => writer.line(&format!("return {}", expr::text(value))),
        None => writer.line("return"),
    }
}

/// `scene`, `show`, or `hide`, with attributes, transforms, and a transition.
fn stage(writer: &mut Writer<'_>, stage: &StageStmt) {
    let mut text = format!("{} {}", stage_kind(stage.kind), stage.image.join("."));
    for attribute in &stage.attributes {
        text.push(' ');
        text.push_str(attribute);
    }
    if !stage.transforms.is_empty() {
        text.push_str(&format!(" at {}", stage.transforms.join(", ")));
    }
    if let Some(transition) = &stage.transition {
        text.push_str(&format!(" with {transition}"));
    }
    writer.line(&text);
}

/// A `pause` or a `wait`.
///
/// Two keywords, two events, one spelling each (`TOOLING.md §3`). A duration is `pause`, which is
/// what a Ren'Py reader already writes and the far more common case; a click is `wait click`,
/// because a bare `pause` does not say what it is waiting for. `wait 2.0` and a bare `pause` both
/// parse, and are written the other way.
fn wait(writer: &mut Writer<'_>, stmt: &WaitStmt) {
    match &stmt.event {
        WaitEvent::Click => writer.line("wait click"),
        WaitEvent::Duration(duration) => writer.line(&format!("pause {}", expr::text(duration))),
    }
}

/// `play`, `stop`, or `queue`, with a source, a loop, and a fade.
fn audio(writer: &mut Writer<'_>, audio: &AudioStmt) {
    let mut text = format!("{} {}", audio_kind(audio.kind), audio.channel);
    if let Some(source) = &audio.source {
        text.push(' ');
        text.push_str(&expr::text(source));
    }
    if audio.looping {
        text.push_str(" loop");
    }
    if let Some(fade) = &audio.fade {
        text.push_str(&format!(" fade {}", expr::text(fade)));
    }
    writer.line(&text);
}

/// A local declaration, which is an assignment with `var` in front of it.
fn var_(writer: &mut Writer<'_>, stmt: &VarStmt) {
    let annotation = stmt
        .ty
        .as_ref()
        .map_or(String::new(), |ty| format!(": {}", expr::type_text(ty)));
    assignment(
        writer,
        &format!("var {}{annotation} = ", stmt.name),
        &stmt.value,
    );
}

/// An assignment to an existing place.
fn assign(writer: &mut Writer<'_>, stmt: &AssignStmt) {
    let prefix = format!("{} {} ", expr::text(&stmt.target), assign_op(stmt.op));
    assignment(writer, &prefix, &stmt.value);
}

/// Writes `prefix` and a value, wrapped at [`WIDTH`] if it does not fit.
///
/// `TOOLING.md §3` asks for the split to be at the *outermost* binary operator and never inside a
/// token, which for a left-leaning chain means one line per operand with the operator leading the
/// continuation. The continuation is a `\` because that is what the language has
/// (`LANGUAGE.md §1`) — a wrapped line would otherwise be a new statement.
///
/// Only the right-hand side of an `=` is wrapped, only at binary operators, and only once per
/// operand: an operand that is itself too long stays too long. Saying so is better than wrapping it
/// somewhere the rule does not cover, because the wrap has to re-parse to the same expression.
fn assignment(writer: &mut Writer<'_>, prefix: &str, value: &Expr) {
    let rendered = expr::text(value);
    let chain = spine(value);

    let Some((head, rest)) = chain else {
        // Nothing to split at, so the line is as long as it is.
        writer.line(&format!("{prefix}{rendered}"));
        return;
    };
    if writer.cursor() + prefix.len() + rendered.len() <= WIDTH {
        writer.line(&format!("{prefix}{rendered}"));
        return;
    }

    writer.line(&format!("{prefix}{} \\", expr::text(head)));
    writer.level_up();
    let last = rest.len() - 1;
    for (index, (op, operand)) in rest.iter().enumerate() {
        let continuation = if index == last { "" } else { " \\" };
        writer.line(&format!(
            "{} {}{continuation}",
            expr::bin_op(*op),
            expr::text(operand)
        ));
    }
    writer.level_down();
}

/// A binary chain's first operand and the operator-operand pairs that follow it.
///
/// The chain is followed down its *left* spine, which is how associativity puts it in the tree: for
/// `a and b and c` the head is `a` and the pairs are `(and b)` and `(and c)`.
fn spine(expr: &Expr) -> Option<(&Expr, Vec<(crate::tree::BinOp, &Expr)>)> {
    let Expr::Binary { op, lhs, rhs, .. } = expr else {
        return None;
    };

    let mut rest = vec![(*op, rhs.as_ref())];
    let mut head = lhs.as_ref();
    while let Expr::Binary { op, lhs, rhs, .. } = head {
        rest.push((*op, rhs.as_ref()));
        head = lhs.as_ref();
    }
    rest.reverse();
    Some((head, rest))
}

/// A staging keyword.
fn stage_kind(kind: StageKind) -> &'static str {
    match kind {
        StageKind::Scene => "scene",
        StageKind::Show => "show",
        StageKind::Hide => "hide",
    }
}

/// An audio keyword.
fn audio_kind(kind: AudioKind) -> &'static str {
    match kind {
        AudioKind::Play => "play",
        AudioKind::Stop => "stop",
        AudioKind::Queue => "queue",
    }
}

/// An assignment operator.
fn assign_op(op: AssignOp) -> &'static str {
    match op {
        AssignOp::Assign => "=",
        AssignOp::Add => "+=",
        AssignOp::Sub => "-=",
        AssignOp::Mul => "*=",
        AssignOp::Div => "/=",
    }
}
