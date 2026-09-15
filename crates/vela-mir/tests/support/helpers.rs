//! Shared helpers for the MIR test binaries.
//!
//! Included by more than one test binary, so each compiles the whole module and uses part
//! of it. That is what the allow below is for — not an exemption from anything.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use vela_diag::Diagnostic;
use vela_hir::ModuleName;
use vela_span::SourceMap;
use vela_types::Env;

use vela_mir::{Module, OptLevel};

/// The directory holding `<name>.vela` inputs and their `<name>.expected` output.
#[must_use]
pub fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/mir")
}

/// Whether to rewrite goldens rather than compare against them.
#[must_use]
pub fn blessing() -> bool {
    std::env::var_os("VELA_BLESS").is_some()
}

/// Every corpus entry, sorted.
#[must_use]
pub fn corpus() -> Vec<PathBuf> {
    let dir = golden_dir();
    let mut inputs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "vela"))
        .collect();
    inputs.sort();
    assert!(!inputs.is_empty(), "no corpus entries in {}", dir.display());
    inputs
}

/// What one corpus entry lowered to.
pub struct Lowered {
    /// The module.
    pub module: Module,
    /// What lowering itself reported.
    pub diagnostics: Vec<Diagnostic>,
    /// The file names, for rendering a diagnostic.
    pub sources: SourceMap,
    /// The entry's name.
    pub name: String,
}

/// Lowers one corpus entry.
///
/// A `pass_`-prefixed entry is optimized at `-O2` so that its golden shows the pass's
/// effect; everything else is left at `-O0`, because the milestone's first criterion is
/// that every construct *lowers* — a statement about what lowering produces, not about
/// what survives being optimized.
#[must_use]
pub fn lower_entry(path: &Path) -> Lowered {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut sources = SourceMap::new();
    let id = sources.add(name.as_str(), &text);
    let parsed = vela_syntax::parse(id, &text);
    let (env, _) = Env::build(&parsed.program);
    let mut lowered = vela_mir::lower(&ModuleName::new(name.as_str()), &parsed.program, &env);

    let level = if name.starts_with("pass_") {
        OptLevel::O2
    } else {
        OptLevel::None
    };
    vela_mir::optimize(&mut lowered.module, level);

    Lowered {
        module: lowered.module,
        diagnostics: lowered.diagnostics,
        sources,
        name,
    }
}
