//! Printing expressions, types, and patterns as one line.
//!
//! No precedence handling, deliberately: the tree came from source, and every place the grammar
//! needs parentheses the parser put a [`Expr::Paren`] node there. Printing the tree in order
//! therefore re-parses to the same tree — which is the property the round-trip test pins, and the
//! reason this is a printer and not a pretty-printer.

use crate::tree::{BinOp, Expr, Param, Pattern, StrPart, Type, UnOp};

/// An expression, as text.
pub(crate) fn text(expr: &Expr) -> String {
    match expr {
        Expr::Int { value, hex, .. } => number(*value, *hex),
        Expr::Float { value, .. } => float(*value),
        Expr::Path { value, .. } => format!("@{value:?}"),
        Expr::Bool { value, .. } => value.to_string(),
        Expr::None { .. } => "none".to_string(),
        Expr::Name { name, .. } => name.clone(),
        Expr::Str { parts, .. } => string(parts),
        Expr::List { items, .. } => {
            let items: Vec<String> = items.iter().map(text).collect();
            format!("[{}]", items.join(", "))
        }
        Expr::Map { entries, .. } => {
            let entries: Vec<String> = entries
                .iter()
                .map(|(key, value)| format!("{}: {}", text(key), text(value)))
                .collect();
            format!("{{{}}}", entries.join(", "))
        }
        Expr::Field { base, name, .. } => format!("{}.{name}", text(base)),
        Expr::Call { callee, args, .. } => {
            let args: Vec<String> = args.iter().map(text).collect();
            format!("{}({})", text(callee), args.join(", "))
        }
        Expr::Index { base, index, .. } => format!("{}[{}]", text(base), text(index)),
        Expr::Unary { op, operand, .. } => format!("{}{}", un_op(*op), text(operand)),
        Expr::Binary { op, lhs, rhs, .. } => {
            format!("{} {} {}", text(lhs), bin_op(*op), text(rhs))
        }
        Expr::Paren { inner, .. } => format!("({})", text(inner)),
        Expr::If {
            cond, then_, else_, ..
        } => format!("{} if {} else {}", text(then_), text(cond), text(else_)),
        Expr::Lambda { params, body, .. } => {
            format!("fn({}) -> {}", params_text(params), text(body))
        }
        // Unreachable in a file that parses: `format` refuses one that does not.
        Expr::Error { .. } => String::new(),
    }
}

/// A declared type, as text.
pub(crate) fn type_text(declared: &Type) -> String {
    match declared {
        Type::Named { path, .. } => path.join("."),
        Type::Optional { inner, .. } => format!("{}?", type_text(inner)),
        Type::List { element, .. } => format!("list<{}>", type_text(element)),
        Type::Map { key, value, .. } => format!("map<{}, {}>", type_text(key), type_text(value)),
        Type::Tuple { elements, .. } => {
            let elements: Vec<String> = elements.iter().map(type_text).collect();
            format!("({})", elements.join(", "))
        }
        Type::Error { .. } => String::new(),
    }
}

/// A parameter list, as text.
pub(crate) fn params_text(params: &[Param]) -> String {
    let rendered: Vec<String> = params
        .iter()
        .map(|param| {
            let mut out = format!("{}: {}", param.name, type_text(&param.ty));
            if let Some(default) = &param.default {
                out.push_str(" = ");
                out.push_str(&text(default));
            }
            out
        })
        .collect();
    rendered.join(", ")
}

/// A `when` pattern, as text. The wildcard prints as `_`.
pub(crate) fn pattern(pattern: &Pattern) -> String {
    let mut out = if pattern.path.is_empty() {
        "_".to_string()
    } else {
        pattern.path.join(".")
    };
    if !pattern.bindings.is_empty() {
        out.push('(');
        out.push_str(&pattern.bindings.join(", "));
        out.push(')');
    }
    out
}

/// A binary operator, as written.
pub(crate) fn bin_op(op: BinOp) -> &'static str {
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

/// A prefix operator, as written.
fn un_op(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "-",
        UnOp::Not => "!",
    }
}

/// An integer literal, in the radix it was written in.
fn number(value: i64, hex: bool) -> String {
    if hex && value >= 0 {
        format!("0x{value:x}")
    } else {
        value.to_string()
    }
}

/// A float literal, always with a fractional part or an exponent.
///
/// Written here rather than taken from `vela-world`: this crate ranks below that one, and the
/// contract is round-tripping — a reader must get the same `f64` back, and `1.0` must stay a float
/// literal rather than becoming the integer `1`. The MIR printer answers the same question for its
/// own readers, and neither is the other's definition of correct.
fn float(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    let rendered = format!("{value}");
    if rendered.contains(['.', 'e', 'E']) {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

/// A string literal, with its escapes put back.
///
/// The tree holds *resolved* text, so the printer is the only party that can restore the escapes —
/// and it must: an unescaped `[` would come back as an interpolation, an unescaped `{` as a text
/// tag, and a literal newline would end the string. What is *not* restored is an escape that did
/// nothing: `\q` and `\}` are `q` and `}`, and the canonical form has the redundant backslash
/// removed (`TOOLING.md §3`). A doubled sigil *is* restored, because dropping it would change what
/// the string means.
///
/// `]` and `}` need no escape: only the opening character of a pair means anything outside an
/// interpolation.
fn string(parts: &[StrPart]) -> String {
    let mut out = String::from("\"");

    for part in parts {
        match part {
            StrPart::Literal { text, .. } => out.push_str(&escaped(text)),
            StrPart::Interpolation { expr, .. } => {
                out.push('[');
                out.push_str(&text(expr));
                out.push(']');
            }
        }
    }

    out.push('"');
    out
}

/// Text with the escapes a string literal needs.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            // The two sigils, doubled — the same spelling Ren'Py uses, and the one the diagnostic
            // tells a reader to write.
            '[' => out.push_str("[["),
            '{' => out.push_str("{{"),
            _ => out.push(character),
        }
    }
    out
}
