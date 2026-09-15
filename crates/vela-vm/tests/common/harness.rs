//! The whole pipeline for a fixture: source to verified bytecode.

use vela_bytecode::Module;

/// Compiles a fixture through the whole front end, asserting every stage is clean.
///
/// The front end is asserted clean *before* anything is compiled, not only after: a fixture
/// with a syntax error parses into a recovered tree that lowers and verifies perfectly well —
/// it is simply a different program. Without this a typo in a fixture becomes a wrong story
/// that passes every later stage, which is how a stray double colon in a menu fixture silently
/// deleted its first choice and turned a test about branching into a test about a two-choice
/// menu.
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

    assert!(
        lowered.diagnostics.is_empty(),
        "the fixture does not lower: {:?}",
        lowered
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>()
    );

    let module = vela_bytecode::compile(&lowered.module, true);

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
