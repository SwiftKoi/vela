//! Compiling a fixture into the program a debugger runs.
//!
//! The whole front end, like `vela-vm`'s harness and for the same reason: a fixture with a
//! mistake should fail here rather than become a different program that every later stage is
//! happy to describe.

use std::collections::BTreeMap;

use vela_bytecode::Module;
use vela_debug::Program;
use vela_span::{FileId, SourceMap};

/// Compiles `text` as a one-file project, returning the debugger's view of it.
///
/// The source name is `main.vela`, which is what the command line would name a file in a
/// project's `src/` — the round trip through a real path is what `Program::file_for_path` and
/// `Program::name_of` are tested against.
pub fn program(text: &str) -> Program {
    let mut sources = SourceMap::new();
    let id = sources.add("main.vela", text);
    let module = compile(id, text);

    let entry = module
        .labels
        .first()
        .and_then(|label| module.strings.get(label.name))
        .expect("the fixture has a label")
        .to_string();
    Program::new(module, entry, sources)
}

/// Compiles a fixture, asserting every stage is clean.
fn compile(id: FileId, text: &str) -> Module {
    let parsed = vela_syntax::parse(id, text);
    assert!(
        parsed.diagnostics.is_empty(),
        "the fixture does not parse: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| format!("{}: {}", d.code.as_str(), d.message))
            .collect::<Vec<_>>()
    );

    let (env, _) = vela_types::Env::build(&parsed.program);
    let lowered = vela_mir::lower(&vela_hir::ModuleName::new("main"), &parsed.program, &env);
    assert!(
        lowered.diagnostics.is_empty(),
        "the fixture does not lower: {:?}",
        lowered
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>()
    );

    let module = vela_bytecode::compile(&linked(&lowered.module), true);
    let diagnostics = vela_bytecode::verify(&module);
    assert!(
        diagnostics.is_empty(),
        "the module does not verify: {:?}",
        diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>()
    );
    module
}

/// Links one module, which is what qualifies its labels.
///
/// A fixture is linked rather than used straight from lowering because the command line links
/// too: a real session's labels are `main.start`, not `start`, and a test that skipped the link
/// would be describing a program the engine never runs.
fn linked(module: &vela_mir::Module) -> vela_mir::Module {
    let imports: BTreeMap<String, vela_hir::ModuleName> = BTreeMap::new();
    let units = [vela_mir::Unit {
        module,
        imports: &imports,
    }];
    vela_mir::link(&units).expect("a single module links")
}
