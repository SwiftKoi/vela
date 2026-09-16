//! The MIR pretty-printer.
//!
//! Deterministic by construction: every collection it walks is a `Vec` or a `BTreeMap`, so
//! two runs on the same input produce byte-identical text. That is what makes this usable
//! as a golden — a pretty-printer that iterated a hash map would produce a corpus that
//! fails at random.
//!
//! Slots print by their declared name where they have one (`x`) and as `_3` where they do
//! not. The goldens are read by people, and `warmth` says more than `_2`.

use std::fmt::Write as _;

use vela_syntax::{BinOp, UnOp};
use vela_world::format_float;

use crate::ir::{
    Body, Callee, Const, ConstId, ConstPool, FuncRef, Module, Operand, Place, Stmt, StmtKind,
    Terminator, Value,
};

/// Renders a whole module.
#[must_use]
pub fn print_module(module: &Module) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "module {}", module.name.as_str());

    for definition in &module.structs {
        let _ = writeln!(out, "\nstruct {}:", definition.name);
        for field in &definition.fields {
            let _ = writeln!(out, "    {}: {}", field.name, field.ty);
        }
    }

    for definition in &module.enums {
        let _ = writeln!(out, "\nenum {}:", definition.name);
        for variant in &definition.variants {
            if variant.fields.is_empty() {
                let _ = writeln!(out, "    {}", variant.name);
            } else {
                let fields: Vec<String> = variant.fields.iter().map(ToString::to_string).collect();
                let _ = writeln!(out, "    {}({})", variant.name, fields.join(", "));
            }
        }
    }

    if !module.consts.is_empty() || !module.defaults.is_empty() {
        out.push('\n');
    }
    for definition in &module.consts {
        let _ = writeln!(
            out,
            "const {}: {} = {}",
            definition.name,
            definition.ty,
            render_const(&module.pool, &definition.value)
        );
    }
    for definition in &module.defaults {
        let _ = writeln!(
            out,
            "default {}: {} = {}",
            definition.name,
            definition.ty,
            render_const(&module.pool, &definition.init)
        );
    }

    for body in &module.fns {
        out.push('\n');
        render_body(module, body, "fn", &mut out);
    }
    for body in &module.labels {
        out.push('\n');
        render_body(module, body, "label", &mut out);
    }

    out
}

/// Renders one body, as a `fn` or a `label`.
#[must_use]
pub fn print_body(module: &Module, body: &Body) -> String {
    let mut out = String::new();
    render_body(module, body, "body", &mut out);
    out
}

/// Renders a body's header, blocks, and terminators.
fn render_body(module: &Module, body: &Body, keyword: &str, out: &mut String) {
    let params: Vec<String> = body
        .params
        .iter()
        .map(|slot| format!("{}: {}", slot_name(body, *slot), slot_ty(body, *slot)))
        .collect();

    let returns = if body.ret == vela_types::Ty::Unit {
        String::new()
    } else {
        format!(" -> {}", body.ret)
    };
    let _ = writeln!(
        out,
        "{keyword} {}({}){returns}:",
        body.name,
        params.join(", ")
    );

    let unreachable = body.unreachable_blocks();
    for block in &body.blocks {
        let marker = if unreachable.contains(&block.id) {
            " (unreachable)"
        } else {
            ""
        };
        let _ = writeln!(out, "    b{}{marker}:", block.id.0);
        for stmt in &block.stmts {
            let _ = writeln!(out, "        {}", render_stmt(module, body, stmt));
        }
        let _ = writeln!(out, "        {}", render_term(module, body, &block.term));
    }
}

/// Renders one statement.
///
/// Two families: straight-line work, and building a value. The split is by what the reader
/// is looking for — a computation, or a constructor.
fn render_stmt(module: &Module, body: &Body, stmt: &Stmt) -> String {
    match &stmt.kind {
        StmtKind::Assign { dst, op, a, b } => format!(
            "{} = {} {} {}",
            render_place(body, dst),
            render_value(module, body, *a),
            render_binop(*op),
            render_value(module, body, *b)
        ),
        StmtKind::AssignUn { dst, op, a } => format!(
            "{} = {}{}",
            render_place(body, dst),
            render_unop(*op),
            render_value(module, body, *a)
        ),
        StmtKind::Load { dst, src } => {
            format!(
                "{} = {}",
                render_place(body, dst),
                render_operand(module, body, src)
            )
        }
        StmtKind::Call { dst, callee, args } => {
            let rendered: Vec<String> = args
                .iter()
                .map(|arg| render_value(module, body, *arg))
                .collect();
            let call = format!(
                "{}({})",
                render_callee(module, body, callee),
                rendered.join(", ")
            );
            match dst {
                Some(dst) => format!("{} = {call}", render_place(body, dst)),
                None => call,
            }
        }
        StmtKind::Cmd { kind, args } => {
            let rendered: Vec<String> = args
                .iter()
                .map(|arg| render_value(module, body, *arg))
                .collect();
            format!("cmd {}({})", kind.as_str(), rendered.join(", "))
        }
        other => render_build(module, body, other),
    }
}

/// Renders a statement that builds a value.
fn render_build(module: &Module, body: &Body, kind: &StmtKind) -> String {
    match kind {
        StmtKind::ListNew { dst, items } => {
            let rendered: Vec<String> = items
                .iter()
                .map(|item| render_value(module, body, *item))
                .collect();
            format!("{} = [{}]", render_place(body, dst), rendered.join(", "))
        }
        StmtKind::MapNew { dst, entries } => {
            let rendered: Vec<String> = entries
                .iter()
                .map(|(key, value)| {
                    format!(
                        "{}: {}",
                        render_value(module, body, *key),
                        render_value(module, body, *value)
                    )
                })
                .collect();
            format!("{} = {{{}}}", render_place(body, dst), rendered.join(", "))
        }
        StmtKind::StructNew { dst, name, fields } => {
            let rendered: Vec<String> = fields
                .iter()
                .map(|(field, value)| format!("{field}: {}", render_value(module, body, *value)))
                .collect();
            format!(
                "{} = {name} {{ {} }}",
                render_place(body, dst),
                rendered.join(", ")
            )
        }
        StmtKind::EnumNew {
            dst,
            enum_name,
            variant,
            args,
        } => {
            let rendered: Vec<String> = args
                .iter()
                .map(|arg| render_value(module, body, *arg))
                .collect();
            format!(
                "{} = {enum_name}.{variant}({})",
                render_place(body, dst),
                rendered.join(", ")
            )
        }
        StmtKind::EnumField { dst, base, index } => format!(
            "{} = {}.{index}",
            render_place(body, dst),
            render_value(module, body, *base)
        ),
        StmtKind::IsNone { dst, base } => format!(
            "{} = {} is none",
            render_place(body, dst),
            render_value(module, body, *base)
        ),
        StmtKind::Unwrap { dst, base } => format!(
            "{} = {}!",
            render_place(body, dst),
            render_value(module, body, *base)
        ),
        // Handled by the caller, which is the only place they can be told apart from the
        // family they belong to.
        other => format!("{other:?}"),
    }
}

/// Renders one terminator.
fn render_term(module: &Module, body: &Body, term: &Terminator) -> String {
    match term {
        Terminator::Goto(target) => format!("goto b{}", target.0),
        Terminator::Branch { cond, then_, else_ } => format!(
            "br {} -> b{}, b{}",
            render_value(module, body, *cond),
            then_.0,
            else_.0
        ),
        Terminator::Return(None) => "return".to_string(),
        Terminator::Return(Some(value)) => {
            format!("return {}", render_value(module, body, *value))
        }
        Terminator::JumpLabel(target) => format!("jump {}", render_label(target)),
        Terminator::CallLabel { target, ret } => {
            format!("call {} -> b{}", render_label(target), ret.0)
        }
        Terminator::Dispatch {
            enum_name,
            value,
            arms,
            else_,
        } => {
            let targets: Vec<String> = arms
                .iter()
                .map(|(variant, block)| format!("{}=b{}", variant.0, block.0))
                .collect();
            format!(
                "dispatch {} as {enum_name} [{}] else b{}",
                render_value(module, body, *value),
                targets.join(", "),
                else_.0
            )
        }
        Terminator::Yield(site) => match site.result {
            Some(result) => format!(
                "yield {} -> b{} into {}",
                site.command.as_str(),
                site.resume.0,
                slot_name(body, result)
            ),
            None => format!("yield {} -> b{}", site.command.as_str(), site.resume.0),
        },
        Terminator::Unreachable => "unreachable".to_string(),
    }
}

/// Renders a label reference.
fn render_label(target: &crate::ir::LabelRef) -> String {
    match &target.module {
        Some(module) => format!("{}.{}", module, target.label),
        None => target.label.clone(),
    }
}

/// Renders a write target.
fn render_place(body: &Body, place: &Place) -> String {
    match place {
        Place::Local(slot) => slot_name(body, *slot),
        Place::Default(id) => format!("default#{}", id.0),
        Place::Field { base, field } => format!("{}.{field}", render_place(body, base)),
        Place::Index { base, index } => {
            format!(
                "{}[{}]",
                render_place(body, base),
                render_slot_only(body, *index)
            )
        }
    }
}

/// Renders a read.
fn render_operand(module: &Module, body: &Body, operand: &Operand) -> String {
    match operand {
        Operand::Value(value) => render_value(module, body, *value),
        Operand::Field { base, field } => {
            format!("{}.{field}", render_value(module, body, *base))
        }
        Operand::Index { base, index } => format!(
            "{}[{}]",
            render_value(module, body, *base),
            render_value(module, body, *index)
        ),
        Operand::Len { base } => format!("len({})", render_value(module, body, *base)),
        Operand::Default(id) => format!("default#{}", id.0),
    }
}

/// Renders a value.
fn render_value(module: &Module, body: &Body, value: Value) -> String {
    match value {
        Value::Slot(slot) => slot_name(body, slot),
        Value::Const(id) => render_const(&module.pool, &id),
    }
}

/// Renders a value that a subscript position must hold, which is always a value.
fn render_slot_only(body: &Body, value: Value) -> String {
    match value {
        Value::Slot(slot) => slot_name(body, slot),
        Value::Const(id) => format!("c{}", id.0),
    }
}

/// Renders a call target.
fn render_callee(module: &Module, body: &Body, callee: &Callee) -> String {
    match callee {
        Callee::Direct(FuncRef::Builtin(builtin)) => builtin.as_str().to_string(),
        Callee::Direct(FuncRef::Effect(index)) => module
            .effects
            .get(*index as usize)
            .map_or_else(|| format!("effect#{index}"), |effect| effect.name.clone()),
        Callee::Direct(FuncRef::Defined(index)) => module
            .fns
            .get(*index as usize)
            .map_or_else(|| format!("fn#{index}"), |body| body.name.to_string()),
        Callee::Indirect(value) => format!("{}()", render_value(module, body, *value)),
    }
}

/// Renders a constant from the pool.
fn render_const(pool: &ConstPool, id: &ConstId) -> String {
    match pool.get(*id) {
        Some(Const::None) => "none".to_string(),
        Some(Const::Bool(flag)) => flag.to_string(),
        Some(Const::Int(number)) => number.to_string(),
        Some(Const::Float(number)) => format_float(*number),
        Some(Const::Str(text)) => format!("\"{}\"", escape(text)),
        Some(Const::Variant { enum_name, variant }) => format!("{enum_name}.{variant}"),
        Some(Const::Function(name)) => format!("fn {name}"),
        None => format!("c{}", id.0),
    }
}

/// Escapes a string for the printed form.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// The name a slot prints as.
fn slot_name(body: &Body, slot: crate::ir::Slot) -> String {
    match body.local(slot) {
        Some(decl) if !decl.name.is_empty() => decl.name.clone(),
        _ => format!("_{}", slot.0),
    }
}

/// The type a slot holds, for the header.
fn slot_ty(body: &Body, slot: crate::ir::Slot) -> String {
    body.slot_ty(slot)
        .map_or_else(|| "?".to_string(), ToString::to_string)
}

/// The symbol for a binary operator.
fn render_binop(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "and",
        BinOp::Or => "or",
        BinOp::Is => "is",
        BinOp::IsNot => "is not",
        BinOp::In => "in",
        BinOp::NotIn => "not in",
        BinOp::Coalesce => "??",
    }
}

/// The symbol for a prefix operator, as the source wrote it.
///
/// `not ` keeps its space: MIR is read by people, and a goldens diff that turns `not x` into `notx`
/// would be a diff nobody can read.
fn render_unop(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "-",
        UnOp::Not => "not ",
        UnOp::Bang => "!",
    }
}
