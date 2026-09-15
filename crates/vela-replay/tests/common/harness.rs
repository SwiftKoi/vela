//! The whole pipeline for a fixture: source to verified bytecode.

use vela_bytecode::Module;

/// Compiles a fixture through the whole front end, asserting every stage is clean.
pub fn compile(name: &str, text: &str) -> Module {
    let mut sources = vela_span::SourceMap::new();
    let id = sources.add(name, text);
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
    let lowered = vela_mir::lower(&vela_hir::ModuleName::new(name), &parsed.program, &env);
    assert!(lowered.diagnostics.is_empty(), "the fixture does not lower");

    let module = vela_bytecode::compile(&lowered.module, true);
    assert!(
        vela_bytecode::verify(&module).is_empty(),
        "the module does not verify"
    );
    module
}
