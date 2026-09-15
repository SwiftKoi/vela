//! The verifier-rejection corpus: one hand-crafted module per rule.
//!
//! `BYTECODE.md §4` names eight rules, and `ROADMAP.md` names the risk: *"a gap means a crash
//! in a shipped game."* The mitigation it prescribes is that this file comes **first** — each
//! rule has a failing test to satisfy before it is written. Written afterwards, a rule is
//! checked against the module that happened to be at hand, and the rules nothing exercised
//! are exactly the ones that stay broken.
//!
//! Every module here is malformed on purpose. One is rejected for one rule, so a test that
//! passes for the wrong reason fails loudly rather than quietly.

use vela_bytecode::{
    ByteTy, ConstPool, DebugInfo, DefaultDef, EnumDef, FuncDef, Header, Instr, LocalDef, Module,
    Op, Operand, StringId, StringTable, StructDef, TypeId, TypeTable, VariantDef, verify,
};

/// A module with one function, and nothing else.
fn module_with(code: Vec<Instr>) -> Module {
    Module {
        header: Header::new(true),
        strings: StringTable::from_items(vec!["test".into()]),
        consts: ConstPool::default(),
        types: TypeTable::from_items(vec![ByteTy::Unit]),
        structs: Vec::<StructDef>::new(),
        enums: Vec::<EnumDef>::new(),
        fns: vec![FuncDef {
            name: StringId(0),
            params: Vec::new(),
            ret: TypeId(0),
            locals: Vec::<LocalDef>::new(),
            code,
            spans: Vec::new(),
        }],
        labels: Vec::new(),
        defaults: Vec::<DefaultDef>::new(),
        effects: Vec::new(),
        cmds: Vec::new(),
        debug: DebugInfo::default(),
        checksum: 0,
    }
}

/// An instruction with an unsigned operand.
fn u32(op: Op, operand: u32) -> Instr {
    Instr::with_u32(op, operand)
}

/// The codes a module is rejected with.
fn codes(module: &Module) -> Vec<String> {
    verify(module)
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect()
}

/// Rule 1 — a block reached with two different stack depths.
#[test]
fn uneven_stack_depth_is_rejected() {
    // The jump skips the constant, so the join is reached at depth 0 and depth 1.
    let module = module_with(vec![
        Instr::with_u32(Op::ConstB, 1), // 0, width 6
        u32(Op::JumpIfFalse, 0x16),     // 6, width 6
        Instr {
            op: Op::ConstI,
            operand: Operand::I64(1),
        }, // 12, width 10
        u32(Op::Return, 0),             // 22
    ]);

    assert_eq!(codes(&module), vec!["E6001"]);
}

/// Rule 2 — an instruction given fewer operands than its effect requires.
#[test]
fn an_underfull_stack_is_rejected() {
    let module = module_with(vec![Instr::plain(Op::AddI), u32(Op::Return, 0)]);
    assert_eq!(codes(&module), vec!["E6002"]);
}

/// Rule 3 — a jump that lands inside an instruction.
#[test]
fn a_jump_into_the_middle_of_an_instruction_is_rejected() {
    let module = module_with(vec![
        // An eight-byte operand, so byte 5 is inside it rather than at a boundary.
        Instr {
            op: Op::ConstI,
            operand: Operand::I64(1),
        },
        u32(Op::Jump, 0x0005),
    ]);

    assert_eq!(codes(&module), vec!["E6003"]);
}

/// Rule 4 — a local read on a path that never wrote it.
#[test]
fn reading_an_unwritten_local_is_rejected() {
    let module = module_with(vec![u32(Op::LoadLocal, 0), u32(Op::Return, 0)]);
    assert_eq!(codes(&module), vec!["E6004"]);
}

/// Rule 4 again, from the other side: a parameter counts as written.
#[test]
fn reading_a_parameter_is_allowed() {
    let mut module = module_with(vec![u32(Op::LoadLocal, 0), u32(Op::Return, 1)]);
    module.fns[0].locals.push(LocalDef {
        name: StringId(0),
        ty: TypeId(0),
    });
    module.fns[0].params.push(0);

    assert!(codes(&module).is_empty(), "{:?}", codes(&module));
}

/// Rule 5 — control runs off the end of a function.
#[test]
fn falling_off_the_end_is_rejected() {
    let module = module_with(vec![Instr::plain(Op::ConstNone)]);
    assert_eq!(codes(&module), vec!["E6005"]);
}

/// Rule 6 — a dispatch table with a hole.
#[test]
fn an_incomplete_dispatch_table_is_rejected() {
    let mut module = module_with(vec![
        Instr::plain(Op::ConstNone), // 0, width 2
        Instr::plain(Op::EnumTag),   // 2, width 2
        Instr {
            op: Op::Dispatch,
            operand: Operand::Tables(0, vec![0x12]),
        }, // 4, width 2 + 8 + 4
        u32(Op::Return, 0),          // 18
    ]);

    module.enums.push(EnumDef {
        name: StringId(0),
        variants: vec![
            VariantDef {
                name: StringId(0),
                fields: Vec::new(),
            },
            VariantDef {
                name: StringId(0),
                fields: Vec::new(),
            },
        ],
    });

    // One entry against two variants, so the second has nowhere to go.
    assert_eq!(codes(&module), vec!["E6006"]);
}

/// Rule 7 — a command built with the wrong number of arguments.
#[test]
fn a_command_with_the_wrong_arity_is_rejected() {
    let mut module = module_with(vec![
        Instr::plain(Op::ConstNone), // 0
        Instr::plain(Op::ConstNone), // 2
        Instr {
            op: Op::Cmd,
            operand: Operand::Pair(0, 2),
        }, // 4
        u32(Op::Return, 0),          // 10
    ]);

    module.strings = StringTable::from_items(vec!["test".into(), "say".into()]);
    module.cmds.push(vela_bytecode::CommandSchema {
        name: StringId(1),
        fields: vec![
            vela_bytecode::FieldSchema {
                name: StringId(1),
                ty: TypeId(0),
            };
            5
        ],
    });

    assert_eq!(codes(&module), vec!["E6007"]);
}

/// Rule 8 — code nothing can reach.
#[test]
fn unreachable_code_is_rejected() {
    let module = module_with(vec![u32(Op::Return, 0), u32(Op::Return, 0)]);
    assert_eq!(codes(&module), vec!["E6008"]);
}

/// A well-formed module is accepted, so that the tests above are rejecting something rather
/// than rejecting everything.
#[test]
fn a_well_formed_module_is_accepted() {
    let module = module_with(vec![Instr::plain(Op::ConstNone), u32(Op::Return, 1)]);
    assert!(codes(&module).is_empty(), "{:?}", codes(&module));
}
