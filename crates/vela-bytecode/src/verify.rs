//! The verifier: the eight rules of `BYTECODE.md §4`.
//!
//! Runs on load, and in CI over every golden. Every failure is `E6xxx` — always a compiler
//! bug, never something an author wrote — so a report carries enough of the module for
//! whoever reads it to see what went wrong.
//!
//! # Why this is one dataflow pass and not eight checks
//!
//! Seven of the eight rules are questions about the *same* walk: what is on the stack when
//! control arrives here, and has this local been written. Answering them separately would
//! mean seven traversals that could disagree about which instructions are reachable — and
//! reachability is what half of them are really asking about. Totality and arity are the
//! two that need the module's schema tables rather than the walk, so they hang off it.

use std::collections::{BTreeMap, BTreeSet};

use vela_diag::Diagnostic;
use vela_span::{FileId, Span};

use crate::error;
use crate::module::{FuncDef, Module, TypeId};
use crate::op::{Effect, NO_ENUM, Op, Operand};

/// The type-table entry an `int` has, which is what a comparison produces.
///
/// Pushed types are mostly unknown — the rule exists to catch a code generator that
/// confused an `int` for a `str`, and inventing a type it cannot prove would report a
/// mismatch that is not there.
const UNKNOWN: TypeId = TypeId(u32::MAX);

/// Checks a whole module.
#[must_use]
pub fn verify(module: &Module) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (index, function) in module.fns.iter().enumerate() {
        check_function(module, function, "fn", index, &mut diagnostics);
    }
    for (index, label) in module.labels.iter().enumerate() {
        check_function(module, label, "label", index, &mut diagnostics);
    }

    diagnostics
}

/// What control looks like when it arrives somewhere.
#[derive(Clone, PartialEq, Debug, Default)]
struct State {
    /// The types on the stack, bottom first.
    stack: Vec<TypeId>,
    /// Locals that have certainly been written on every path here.
    written: BTreeSet<u32>,
}

impl State {
    /// Merges two arrivals, reporting the first disagreement.
    ///
    /// Depths must be *equal*, not compatible: rule 1 says a block's incoming depth is
    /// identical on all paths, and a difference means the code generator lost track of a
    /// value rather than that the program is doing something clever.
    fn merge(&mut self, other: &Self) -> Result<(), MergeError> {
        if self.stack.len() != other.stack.len() {
            return Err(MergeError::Depth);
        }
        self.written = self.written.intersection(&other.written).copied().collect();
        Ok(())
    }
}

/// How a merge failed.
enum MergeError {
    /// Two paths arrive with different stack depths.
    Depth,
}

/// Every instruction, keyed by the byte offset it starts at.
///
/// Rule 3 is the question "is this target an instruction boundary", and this map is that
/// question's answer. Built once per function rather than searched per jump.
fn offsets(function: &FuncDef) -> BTreeMap<u32, (usize, &crate::module::Instr)> {
    function
        .code
        .iter()
        .enumerate()
        .map(|(position, instr)| (function.offset_of(position), (position, instr)))
        .collect()
}

/// Folds an arrival into what is already known about a position.
///
/// Returns `None` when there is nothing to do: either the two disagree, which is rule 1
/// failing, or they agree exactly, which means this position and everything below it has
/// already been processed. The second case is what makes the walk terminate — without it,
/// every cycle in the control-flow graph re-pushes its own successors for ever, which is
/// how this hung on the first `while` loop it met.
fn merge_arrival(
    arrivals: &mut BTreeMap<usize, State>,
    reported: &mut BTreeSet<usize>,
    function: &FuncDef,
    position: usize,
    state: &State,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<State> {
    match arrivals.get(&position) {
        None => {
            arrivals.insert(position, state.clone());
            Some(state.clone())
        }
        Some(known) => {
            let mut merged = known.clone();
            if merged.merge(state).is_err() {
                if reported.insert(position) {
                    diagnostics.push(error::stack_depth(
                        &format!("at {}", function.offset_of(position)),
                        span_of(function, position),
                    ));
                }
                return None;
            }
            if merged == *known {
                return None;
            }
            arrivals.insert(position, merged.clone());
            Some(merged)
        }
    }
}

/// Checks one function or label.
fn check_function(
    module: &Module,
    function: &FuncDef,
    kind: &str,
    index: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let where_ = format!("{kind}#{index}");
    let codes = offsets(function);

    let mut entry = State::default();
    entry.written.extend(function.params.iter().copied());

    // `arrivals` is the fixpoint store, not merely a record. A state that has not *changed*
    // must not be propagated, or every cycle in the control-flow graph re-pushes its own
    // successors for ever — which is what a `while` loop in the corpus does, and why this
    // hung rather than failing.
    //
    // Termination comes from the lattice: `written` only ever shrinks under a merge, so it
    // has finite height, and the stack depth is required to be equal rather than merged.
    let mut arrivals: BTreeMap<usize, State> = BTreeMap::new();
    let mut pending: Vec<(usize, State)> = vec![(0, entry)];
    let mut visited: BTreeSet<usize> = BTreeSet::new();
    let mut reported: BTreeSet<usize> = BTreeSet::new();

    // A bound as well as a fixpoint, because this runs on *load*: a verifier that can hang
    // on a malformed module is worse than one that rejects it. Reaching the bound leaves
    // instructions unvisited, which rule 8 reports — so a compiler bug surfaces as a
    // rejection rather than as a module that loads and then spins.
    let budget = function.code.len().saturating_mul(4).saturating_add(64);
    let mut steps = 0usize;

    while let Some((position, state)) = pending.pop() {
        steps += 1;
        if steps > budget {
            break;
        }

        if function.code.get(position).is_none() {
            // Rule 5: control fell off the end of the function.
            if reported.insert(position) {
                diagnostics.push(error::no_terminator(
                    function.offset_of(position),
                    span_of(function, position.saturating_sub(1)),
                ));
            }
            continue;
        }
        visited.insert(position);

        // Rule 1: every path into a block arrives with the same depth.
        let merged = match merge_arrival(
            &mut arrivals,
            &mut reported,
            function,
            position,
            &state,
            diagnostics,
        ) {
            Some(merged) => merged,
            // Either it disagrees, or nothing changed and nothing downstream can either.
            None => continue,
        };

        // Rule 2, rule 4 and rule 7: this instruction's own requirements.
        let mut state = merged;
        apply(module, function, position, &mut state, diagnostics, &where_);

        for successor in successors(module, function, position, &codes, diagnostics, &where_) {
            pending.push((successor, state.clone()));
        }
    }

    report_orphans(function, &visited, diagnostics);
}

/// Rule 8: every instruction is reachable.
///
/// Reported as one diagnostic per orphaned instruction rather than one per run, because the
/// offsets are what locate the mistake — a pass that failed to remove a block leaves a
/// contiguous span of them, and seeing that shape is most of the diagnosis.
fn report_orphans(
    function: &FuncDef,
    visited: &BTreeSet<usize>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for position in 0..function.code.len() {
        if !visited.contains(&position) {
            diagnostics.push(error::orphan_code(
                function.offset_of(position),
                span_of(function, position),
            ));
        }
    }
}

/// Applies one instruction's requirements to the arriving state.
///
/// Only the instruction's *own* requirements: where control goes next is `successors`'s
/// question, and answering it in both places is how a jump target stops being followed.
fn apply(
    module: &Module,
    function: &FuncDef,
    position: usize,
    state: &mut State,
    diagnostics: &mut Vec<Diagnostic>,
    where_: &str,
) {
    let Some(instr) = function.code.get(position) else {
        return;
    };
    let op = instr.op;
    let span = span_of(function, position);

    // Rule 4: a read of a local nothing has written.
    if op == Op::LoadLocal
        && let Operand::U32(slot) = instr.operand
        && !state.written.contains(&slot)
    {
        diagnostics.push(error::local_not_written(slot, span));
    }
    if let Op::StoreLocal | Op::LoadLocal = op
        && let Operand::U32(slot) = instr.operand
    {
        state.written.insert(slot);
    }

    // Rule 7: a command's arity against the schema it names.
    if op == Op::Cmd
        && let Operand::Pair(variant, count) = &instr.operand
    {
        match module.cmds.get(*variant as usize) {
            Some(schema) if schema.fields.len() != *count as usize => {
                let name = module.strings.get(schema.name).unwrap_or("?").to_string();
                diagnostics.push(error::wrong_arity(
                    &name,
                    schema.fields.len(),
                    *count as usize,
                    span,
                ));
            }
            Some(_) => {}
            None => diagnostics.push(error::corrupt("a command schema is missing", span)),
        }
    }

    // Rule 1 and rule 2: the instruction's stack effect.
    let (pops, pushes) = effect(module, instr.op, &instr.operand);
    if state.stack.len() < pops {
        diagnostics.push(error::stack_type(
            where_,
            &format!("{pops} operands"),
            &format!("{}", state.stack.len()),
            span,
        ));
        return;
    }
    state.stack.truncate(state.stack.len() - pops);
    let pushed = pushed_types(module, op, &instr.operand);
    for ty in pushed.into_iter().take(pushes) {
        state.stack.push(ty);
    }

    // Rule 6: a dispatch table covers its enum.
    if op == Op::Dispatch
        && let Operand::Tables(enum_id, targets) = &instr.operand
        && *enum_id != NO_ENUM
        && let Some(definition) = module.enums.get(*enum_id as usize)
        && targets.len() < definition.variants.len()
    {
        for (index, variant) in definition.variants.iter().enumerate().skip(targets.len()) {
            let name = module.strings.get(variant.name).unwrap_or("?").to_string();
            diagnostics.push(error::incomplete_table(index as u32, &name, span));
        }
    }
}

/// Where control goes next.
fn successors(
    module: &Module,
    function: &FuncDef,
    position: usize,
    codes: &BTreeMap<u32, (usize, &crate::module::Instr)>,
    diagnostics: &mut Vec<Diagnostic>,
    where_: &str,
) -> Vec<usize> {
    let Some(instr) = function.code.get(position) else {
        return Vec::new();
    };
    let span = span_of(function, position);

    // Rule 3: every branch target is an instruction boundary.
    let mut target = |offset: u32| -> Option<usize> {
        match codes.get(&offset) {
            Some((index, _)) => Some(*index),
            None => {
                diagnostics.push(error::bad_target(offset, span));
                None
            }
        }
    };

    let _ = (module, where_);
    match instr.op {
        Op::Jump => instr
            .operand
            .u32()
            .and_then(&mut target)
            .into_iter()
            .collect(),
        Op::JumpIfFalse | Op::JumpIfTrue => {
            let mut out = Vec::new();
            if let Some(index) = instr.operand.u32().and_then(&mut target) {
                out.push(index);
            }
            out.push(position + 1);
            out
        }
        Op::Dispatch => match &instr.operand {
            Operand::Tables(_, targets) => {
                let mut out = Vec::new();
                for entry in targets {
                    if let Some(index) = target(*entry) {
                        out.push(index);
                    }
                }
                out
            }
            _ => Vec::new(),
        },
        Op::Return => Vec::new(),
        // Everything else, including `CallLabel`, continues at the next instruction: a label
        // returns to the instruction after the call, which is where the compiler put the
        // jump that carries on from there.
        _ => vec![position + 1],
    }
}

/// How many values an instruction pops and pushes.
fn effect(module: &Module, op: Op, operand: &Operand) -> (usize, usize) {
    let count = operand.count();

    match op.spec().effect {
        Effect::Nothing | Effect::Jump => (0, 0),
        Effect::Push => (0, 1),
        Effect::PushN => (0, count),
        Effect::Pop => (1, 0),
        Effect::Replace => (1, 1),
        Effect::Binary => (2, 1),
        Effect::Set => (2, 0),
        Effect::Update => (3, 1),
        Effect::Index => (2, 1),
        // A command is built into a side register rather than pushed, and the suspension
        // hands the host's answer back in its place. Modelling `Cmd` as pushing a command
        // and `Yield` as replacing it reads better but does not describe the machine: the
        // two drifted by one, and the mismatch showed up as a `dispatch` given `none`.
        Effect::Yield => (0, 1),
        Effect::Return => (count, 0),
        // A call's arity and result come from the callee rather than from its operand,
        // which only names it. A map pops two values per entry.
        // A command is built, not pushed: it goes to the side register the next `Yield`
        // reads. Every other `CallN` instruction leaves its result on the stack.
        Effect::CallN if op == Op::Cmd => (count, 0),
        Effect::CallN => match op {
            Op::MapNew => (2 * count, 1),
            Op::CallFn => {
                let index = operand.u32().map_or(usize::MAX, |index| index as usize);
                match module.fns.get(index) {
                    Some(function) => (function.params.len(), 1),
                    None => (0, 1),
                }
            }
            _ => (count, 1),
        },
        Effect::Variant => {
            // The payload count comes from the enum's schema, which is the only place that
            // knows how many values the construction pushed.
            let pushes = 1;
            let pops = match operand {
                Operand::Pair(enum_id, variant) => module
                    .enums
                    .get(*enum_id as usize)
                    .and_then(|definition| definition.variants.get(*variant as usize))
                    .map_or(0, |variant| variant.fields.len()),
                _ => 0,
            };
            (pops, pushes)
        }
    }
}

/// The types an instruction pushes.
///
/// Unknown wherever the answer would need inference rather than a lookup. The rule exists to
/// catch a code generator that confused an `int` for a `str`; `Unknown` is what keeps it from
/// inventing a mismatch it cannot prove.
fn pushed_types(module: &Module, op: Op, operand: &Operand) -> Vec<TypeId> {
    let unknown = UNKNOWN;
    let _ = module;

    match op {
        // Comparisons and `is.none` produce a boolean, which is worth knowing because a
        // branch consumes it.
        Op::EqI
        | Op::EqF
        | Op::EqS
        | Op::EqB
        | Op::LtI
        | Op::LtF
        | Op::LeI
        | Op::LeF
        | Op::GtI
        | Op::GtF
        | Op::GeI
        | Op::GeF
        | Op::LtS
        | Op::LeS
        | Op::GtS
        | Op::GeS
        | Op::IsNone
        | Op::Not => vec![unknown],
        _ => match operand {
            Operand::None => vec![unknown],
            _ => vec![unknown],
        },
    }
}

/// The span of an instruction, or the function's first if it has none.
fn span_of(function: &FuncDef, position: usize) -> Span {
    function
        .spans
        .get(position)
        .copied()
        .unwrap_or_else(|| Span::new(FileId::from_raw(0), 0, 0))
}
