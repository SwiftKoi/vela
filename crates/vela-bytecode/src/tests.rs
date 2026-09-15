//! Unit tests for the instruction table and the diagnostics.

use vela_diag::{Diagnostic, Severity};
use vela_span::{FileId, Span};

use crate::module::Instr;
use crate::op::{OPS, Op, Operand, OperandKind};

/// A span for a diagnostic that never gets rendered.
fn span() -> Span {
    Span::new(FileId::from_raw(0), 0, 1)
}

#[test]
fn the_table_lists_every_op_once_and_in_order() {
    for (index, spec) in OPS.iter().enumerate() {
        assert_eq!(
            spec.op as usize, index,
            "`{}` is out of order in the table",
            spec.name
        );
    }
}

#[test]
fn every_opcode_decodes_and_nothing_else_does() {
    for (index, spec) in OPS.iter().enumerate() {
        let byte = u8::try_from(index).expect("fewer than 256 opcodes");
        assert_eq!(Op::from_byte(byte), Some(spec.op));
    }

    let past_the_end = u8::try_from(OPS.len()).expect("fewer than 256 opcodes");
    assert_eq!(Op::from_byte(past_the_end), None);
}

#[test]
fn mnemonics_are_unique() {
    let mut seen: Vec<&str> = Vec::new();
    for spec in OPS {
        assert!(!seen.contains(&spec.name), "`{}` is used twice", spec.name);
        seen.push(spec.name);
    }
}

/// The operand kind byte is what makes a corrupted opcode a rejected module rather than a
/// misread stream, so a kind that cannot be produced cannot be decoded either.
#[test]
fn operand_kinds_round_trip() {
    let cases = [
        (Operand::None, OperandKind::None),
        (Operand::U32(7), OperandKind::U32),
        (Operand::I64(-1), OperandKind::I64),
        (Operand::F64(0.5), OperandKind::F64),
        (Operand::Str(3), OperandKind::Str),
        (Operand::Pair(1, 2), OperandKind::U32U32),
        (Operand::Tables(0, vec![1, 2]), OperandKind::Tables),
    ];

    for (operand, kind) in cases {
        assert_eq!(operand.kind(), kind);
        assert_eq!(OperandKind::from_byte(kind as u8), Some(kind));
    }

    assert_eq!(OperandKind::from_byte(255), None);
}

#[test]
fn an_instruction_is_two_header_bytes_plus_its_operands() {
    assert_eq!(Instr::plain(Op::AddI).width(), 2);
    assert_eq!(
        Instr {
            op: Op::ConstI,
            operand: Operand::I64(1)
        }
        .width(),
        10
    );
    assert_eq!(
        Instr {
            op: Op::Dispatch,
            operand: Operand::Tables(0, vec![1, 2, 3])
        }
        .width(),
        2 + 8 + 12
    );
}

/// Every back-end diagnostic is registered, and names the rule it broke.
///
/// `check-diag-codes` requires each code to be referenced by a test. That requirement is
/// worth more here than elsewhere: a verifier failure is *always* a compiler bug, so a code
/// no test can produce is a rule nothing has exercised.
#[test]
fn every_diagnostic_is_registered_and_named() {
    let cases: Vec<(&str, Diagnostic)> = vec![
        ("E6001", crate::error::stack_depth("add.i", span())),
        (
            "E6002",
            crate::error::stack_type("add.i", "int", "str", span()),
        ),
        ("E6003", crate::error::bad_target(7, span())),
        ("E6004", crate::error::local_not_written(2, span())),
        ("E6005", crate::error::no_terminator(9, span())),
        ("E6006", crate::error::incomplete_table(1, "cold", span())),
        ("E6007", crate::error::wrong_arity("say", 5, 4, span())),
        ("E6008", crate::error::orphan_code(3, span())),
        ("E7101", crate::error::unknown_format(99, span())),
        ("E7102", crate::error::unknown_command("wave", span())),
        ("E7103", crate::error::corrupt("bad magic", span())),
    ];

    for (expected, diagnostic) in cases {
        assert_eq!(
            diagnostic.code.as_str(),
            expected,
            "a constructor reports the wrong code"
        );
        assert_eq!(
            diagnostic.severity(),
            Severity::Error,
            "{expected} is not an error"
        );
    }
}

/// Compiling every entry in the MIR corpus and verifying the result.
///
/// This is the round trip the whole back end rests on: if lowering emits something the
/// verifier rejects, either the code generator is wrong or the verifier is, and the corpus
/// is small enough to read which.
#[test]
fn the_mir_corpus_compiles_and_verifies() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/mir");
    let entries = std::fs::read_dir(&corpus).expect("read the MIR corpus");

    let mut checked = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "vela") {
            continue;
        }

        let text = std::fs::read_to_string(&path).expect("read an entry");
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let mut sources = vela_span::SourceMap::new();
        let id = sources.add(name.as_str(), &text);
        let parsed = vela_syntax::parse(id, &text);
        let (env, _) = vela_types::Env::build(&parsed.program);
        let lowered = vela_mir::lower(
            &vela_hir::ModuleName::new(name.as_str()),
            &parsed.program,
            &env,
        );

        let module = crate::compile(&lowered.module, true);
        let diagnostics = crate::verify(&module);

        assert!(
            diagnostics.is_empty(),
            "{} does not verify:\n{}",
            path.display(),
            diagnostics
                .iter()
                .map(|diagnostic| format!("  {}: {}", diagnostic.code.as_str(), diagnostic.message))
                .collect::<Vec<_>>()
                .join("\n")
        );
        checked += 1;
    }

    assert!(checked >= 10, "only {checked} entries were checked");
}
