//! Diagnostics the back end reports.
//!
//! Everything here is a **compiler bug**, not something an author can write — which is why
//! they are `E6xxx` and `E7xxx` rather than `E3xxx`. A verifier failure means the optimizer
//! or the code generator produced a module that cannot run, and the report carries enough
//! of the module for whoever reads it to see what went wrong.

use vela_diag::{Code, Diagnostic};
use vela_span::Span;

/// Builds a diagnostic for a registered code.
///
/// # Panics
///
/// Panics if the code is not registered. `check-diag-codes` rejects an unregistered code
/// before it can be committed.
fn diag(
    code: &str,
    message: impl Into<String>,
    span: Span,
    label: impl Into<String>,
) -> Diagnostic {
    let code =
        Code::new(code).unwrap_or_else(|| panic!("`{code}` is not in crates/vela-diag/codes.txt"));
    Diagnostic::new(code, message, span, label)
}

/// A verifier failure, named by the rule it broke.
///
/// The eight rules are `BYTECODE.md §4`, and one code per rule is deliberate: a report that
/// says "stack mismatch" and a report that says "unreachable code left in" are acted on
/// differently, and collapsing them would hide which invariant the compiler is violating.
#[must_use]
pub fn stack_depth(where_: &str, span: Span) -> Diagnostic {
    diag(
        "E6001",
        "the stack depth differs across paths into a block",
        span,
        format!("`{where_}` is reached with two different stack depths"),
    )
    .with_help("this is a code generator bug, not a mistake in the story")
}

/// `E6002` — an operand does not have the type the instruction expects.
#[must_use]
pub fn stack_type(where_: &str, expected: &str, found: &str, span: Span) -> Diagnostic {
    diag(
        "E6002",
        format!("`{where_}` was given `{found}` where `{expected}` is required"),
        span,
        "the operand does not match this instruction's stack effect",
    )
    .with_help("this is a code generator bug, not a mistake in the story")
}

/// `E6003` — a jump lands in the middle of an instruction.
#[must_use]
pub fn bad_target(offset: u32, span: Span) -> Diagnostic {
    diag(
        "E6003",
        format!("the jump target {offset} is not an instruction boundary"),
        span,
        "control would land inside an instruction",
    )
    .with_help("this is a code generator bug, not a mistake in the story")
}

/// `E6004` — a local is read on a path where nothing wrote it.
#[must_use]
pub fn local_not_written(slot: u32, span: Span) -> Diagnostic {
    diag(
        "E6004",
        format!("local {slot} is read before it is written"),
        span,
        "no path reaching this instruction writes the slot",
    )
    .with_help("this is a code generator bug, not a mistake in the story")
}

/// `E6005` — a reachable block runs off its end.
#[must_use]
pub fn no_terminator(offset: u32, span: Span) -> Diagnostic {
    diag(
        "E6005",
        format!("the block at {offset} does not end in a terminator"),
        span,
        "control would fall off the end of the function",
    )
    .with_help("this is a code generator bug, not a mistake in the story")
}

/// `E6006` — a dispatch table has a hole.
#[must_use]
pub fn incomplete_table(entry: u32, variant: &str, span: Span) -> Diagnostic {
    diag(
        "E6006",
        format!("the dispatch table has no entry for variant `{variant}`"),
        span,
        format!("entry {entry} is missing, so a value of that variant would fall through"),
    )
    .with_help("a non-exhaustive match slipped past `E4001`")
}

/// `E6007` — a command or effect was built with the wrong number of arguments.
#[must_use]
pub fn wrong_arity(name: &str, expected: usize, found: usize, span: Span) -> Diagnostic {
    diag(
        "E6007",
        format!("`{name}` takes {expected} argument(s) but was given {found}"),
        span,
        "the operand count does not match the registered schema",
    )
    .with_help("a command or effect is a schema registration, so this is the schema disagreeing with the compiler")
}

/// `E6008` — unreachable code is still present.
#[must_use]
pub fn orphan_code(offset: u32, span: Span) -> Diagnostic {
    diag(
        "E6008",
        format!("the code at {offset} is unreachable"),
        span,
        "no path reaches this instruction, and `dead_block` should have removed it",
    )
    .with_help("the module cannot be verified while unreachable code is present")
}

/// `E7101` — the container's format version is not one this build reads.
#[must_use]
pub fn unknown_format(found: u16, span: Span) -> Diagnostic {
    diag(
        "E7101",
        format!("bytecode format {found} is not supported"),
        span,
        format!("this build reads format {}", crate::module::FORMAT),
    )
    .with_help("rebuild the project: a newer compiler wrote this module")
}

/// `E7102` — the module uses a command variant this runtime does not know.
#[must_use]
pub fn unknown_command(name: &str, span: Span) -> Diagnostic {
    diag(
        "E7102",
        format!("`{name}` is not a command this runtime knows"),
        span,
        "the module was built against a newer set of command schemas",
    )
    .with_help("update the runtime, or rebuild the project against this one")
}

/// `E7103` — the container is malformed.
#[must_use]
pub fn corrupt(reason: &str, span: Span) -> Diagnostic {
    diag(
        "E7103",
        format!("the module container is corrupt: {reason}"),
        span,
        "the bytes do not describe a module",
    )
    .with_help("this is a build or transfer problem rather than a mistake in the story")
}
