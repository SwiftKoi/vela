//! The disassembler.
//!
//! Driven by the same table the verifier reads, which is the point of the table being data:
//! a disassembly that could disagree with what the module means would be worse than no
//! disassembly, because it would be *believed*.
//!
//! Offsets are shown, because they are what a jump target refers to and what a debugger
//! maps back to source. A disassembly without them is a listing, not a tool.

use std::fmt::Write as _;

use crate::module::{ByteConst, ByteTy, FuncDef, Module, StringId, TypeId};
use crate::op::{NO_ENUM, Op, Operand};

/// Renders a whole module.
#[must_use]
pub fn disassemble(module: &Module) -> String {
    let mut out = String::new();

    let _ = writeln!(
        out,
        "; format {} abi {} flags {:#x} checksum {:#018x}",
        module.header.format, module.header.abi, module.header.flags, module.checksum
    );
    let _ = writeln!(
        out,
        "; {} strings, {} constants, {} types",
        module.strings.items().len(),
        module.consts.items().len(),
        module.types.items().len()
    );

    render_constants(module, &mut out);
    render_shapes(module, &mut out);
    render_defaults(module, &mut out);

    for (index, function) in module.fns.iter().enumerate() {
        let _ = writeln!(out);
        render_body(module, function, "fn", index, &mut out);
    }
    for (index, label) in module.labels.iter().enumerate() {
        let _ = writeln!(out);
        render_body(module, label, "label", index, &mut out);
    }

    out
}

/// Renders the constants, so that a `const.s` can be read without a second lookup.
fn render_constants(module: &Module, out: &mut String) {
    if module.consts.items().is_empty() {
        return;
    }
    let _ = writeln!(out, "; constants");
    for (index, constant) in module.consts.items().iter().enumerate() {
        let _ = writeln!(out, ";   c{index:<4} {}", render_constant(module, constant));
    }
}

/// Renders a constant.
fn render_constant(module: &Module, constant: &ByteConst) -> String {
    match constant {
        ByteConst::None => "none".to_string(),
        ByteConst::Bool(flag) => flag.to_string(),
        ByteConst::Int(number) => number.to_string(),
        ByteConst::Float(number) => vela_world::format_float(*number),
        ByteConst::Str(id) => format!("\"{}\"", module.strings.get(*id).unwrap_or("?")),
        ByteConst::Variant { enum_id, variant } => {
            let name = module
                .enums
                .get(*enum_id as usize)
                .and_then(|definition| definition.variants.get(*variant as usize))
                .and_then(|variant| module.strings.get(variant.name))
                .unwrap_or("?");
            format!("variant {name}")
        }
        ByteConst::Function(index) => format!("fn #{index}"),
    }
}

/// Renders the struct and enum shapes, which a `struct.new` and a `dispatch` refer to.
fn render_shapes(module: &Module, out: &mut String) {
    if module.structs.is_empty() && module.enums.is_empty() {
        return;
    }
    let _ = writeln!(out, "; shapes");

    for (index, definition) in module.structs.iter().enumerate() {
        let name = module.strings.get(definition.name).unwrap_or("?");
        let fields: Vec<String> = definition
            .fields
            .iter()
            .map(|field| {
                let field_name = module.strings.get(field.name).unwrap_or("?");
                format!("{field_name}: {}", render_type(module, field.ty))
            })
            .collect();
        let _ = writeln!(out, ";   struct {index} {name} {{ {} }}", fields.join(", "));
    }
    for (index, definition) in module.enums.iter().enumerate() {
        let name = module.strings.get(definition.name).unwrap_or("?");
        let variants: Vec<String> = definition
            .variants
            .iter()
            .enumerate()
            .map(|(position, variant)| {
                let variant_name = module.strings.get(variant.name).unwrap_or("?");
                let fields: Vec<String> = variant
                    .fields
                    .iter()
                    .map(|ty| render_type(module, *ty))
                    .collect();
                format!("{position}:{variant_name}({})", fields.join(", "))
            })
            .collect();
        let _ = writeln!(out, ";   enum {index} {name} {{ {} }}", variants.join(", "));
    }
}

/// Renders the defaults, which are the save schema.
fn render_defaults(module: &Module, out: &mut String) {
    if module.defaults.is_empty() {
        return;
    }
    let _ = writeln!(out, "; defaults");
    for definition in &module.defaults {
        let name = module.strings.get(definition.name).unwrap_or("?");
        let ty = render_type(module, definition.ty);
        let value = module
            .consts
            .get(definition.init)
            .map_or_else(|| "?".to_string(), |value| render_constant(module, value));
        let _ = writeln!(out, ";   {name}: {ty} = {value}");
    }
}

/// Renders the command schemas the module carries.
fn render_commands(module: &Module, out: &mut String) {
    for (index, command) in module.cmds.iter().enumerate() {
        let name = module.strings.get(command.name).unwrap_or("?");
        let fields: Vec<String> = command
            .fields
            .iter()
            .map(|field| {
                let field_name = module.strings.get(field.name).unwrap_or("?");
                format!("{field_name}: {}", render_type(module, field.ty))
            })
            .collect();
        let _ = writeln!(out, ";   cmd {index} {name}({})", fields.join(", "));
    }
}

/// Renders a type by its table index.
///
/// The table is structural, so only the *root* of a type is an index; the rest is nested.
/// Looking one up and then delegating is what keeps a name like `list<map<str, int>>` from
/// needing an entry in the table for every part of it.
///
/// Public because a debugger shows a variable's type too, and two renderers would eventually
/// disagree about how a type is spelled — which is the one thing a debugger's view of a value
/// must not do.
#[must_use]
pub fn render_type(module: &Module, id: TypeId) -> String {
    match module.types.get(id) {
        Some(ty) => render_ty(module, ty),
        None => "?".to_string(),
    }
}

/// Renders a type.
fn render_ty(module: &Module, ty: &ByteTy) -> String {
    match ty {
        ByteTy::Int => "int".to_string(),
        ByteTy::Float => "float".to_string(),
        ByteTy::Bool => "bool".to_string(),
        ByteTy::Str => "str".to_string(),
        ByteTy::None => "none".to_string(),
        ByteTy::Unit => "unit".to_string(),
        ByteTy::Unknown => "?".to_string(),
        ByteTy::Optional(inner) => format!("{}?", render_ty(module, inner)),
        ByteTy::List(element) => format!("list<{}>", render_ty(module, element)),
        ByteTy::Map(key, value) => {
            format!(
                "map<{}, {}>",
                render_ty(module, key),
                render_ty(module, value)
            )
        }
        ByteTy::Struct(index) => module
            .structs
            .get(*index as usize)
            .and_then(|definition| module.strings.get(definition.name))
            .unwrap_or("struct")
            .to_string(),
        ByteTy::Enum(index) => module
            .enums
            .get(*index as usize)
            .and_then(|definition| module.strings.get(definition.name))
            .unwrap_or("enum")
            .to_string(),
        ByteTy::Func(index) => format!("fn#{index}"),
    }
}

/// Renders one function or label.
fn render_body(module: &Module, body: &FuncDef, kind: &str, index: usize, out: &mut String) {
    let name = module.strings.get(body.name).unwrap_or("?");
    let params: Vec<String> = body
        .params
        .iter()
        .map(|slot| {
            let local = body.locals.get(*slot as usize);
            let local_name = local
                .and_then(|local| module.strings.get(local.name))
                .unwrap_or("?");
            let ty = local.map_or_else(|| "?".to_string(), |local| render_type(module, local.ty));
            format!("{local_name}: {ty}")
        })
        .collect();

    let _ = writeln!(
        out,
        "{kind} {index} `{name}`({}) -> {}:",
        params.join(", "),
        render_type(module, body.ret)
    );

    for (position, instr) in body.code.iter().enumerate() {
        let _ = writeln!(
            out,
            "  {:04x}  {:<12}{}",
            body.offset_of(position),
            instr.op.name(),
            render_operand(module, instr.op, &instr.operand)
        );
    }
}

/// Renders an instruction's operands.
fn render_operand(module: &Module, op: Op, operand: &Operand) -> String {
    match operand {
        Operand::None => String::new(),
        Operand::U32(value) => match op {
            // A jump's operand is an offset, and saying so is the difference between a
            // listing and something a person can follow.
            Op::Jump | Op::JumpIfFalse | Op::JumpIfTrue => format!("-> {value:04x}"),
            Op::CallFn => format!("#{value}"),
            Op::CallLabel => format!(
                "{} `{}`",
                value,
                module
                    .labels
                    .get(*value as usize)
                    .and_then(|label| module.strings.get(label.name))
                    .unwrap_or("?")
            ),
            _ => value.to_string(),
        },
        Operand::I64(value) => value.to_string(),
        Operand::F64(value) => vela_world::format_float(*value),
        Operand::Str(id) => format!("\"{}\"", module.strings.get(StringId(*id)).unwrap_or("?")),
        Operand::Pair(first, second) => format!("{first}, {second}"),
        Operand::Tables(enum_id, targets) => {
            let entries: Vec<String> = targets
                .iter()
                .map(|target| format!("{target:04x}"))
                .collect();
            if *enum_id == NO_ENUM {
                format!("[{}]", entries.join(", "))
            } else {
                let name = module
                    .enums
                    .get(*enum_id as usize)
                    .and_then(|definition| module.strings.get(definition.name))
                    .unwrap_or("?");
                format!("{name} [{}]", entries.join(", "))
            }
        }
    }
}

/// Renders the command schemas, which the header does not carry but a report should.
#[must_use]
pub fn disassemble_commands(module: &Module) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "; commands");
    render_commands(module, &mut out);
    out
}
