//! Loading a *built* story: bytecode straight to the machine, no compiler in the path.
//!
//! `BUILD_AND_ASSETS.md §1` and `RUNTIME.md §8`: a distribution bundle is assets plus bytecode,
//! and `ARCHITECTURE.md §8` promises that startup *"loads bytecode without recompilation"*. This
//! file is that promise made real. It reads the module a build wrote and the entry point the
//! bundle's manifest records, and hands back a [`Module`] and a label — it never parses `.vela`.
//!
//! # What a bundle looks like
//!
//! ```text
//!   dist/
//!     manifest.json      manifest_version, name, entry ("main.start"), assets
//!     scripts/**/*.velac  one compiled module per source file
//!     assets/**           imported artifacts
//! ```
//!
//! The entry point lives in the manifest because it is a *project* fact (`vela.toml`'s
//! `[project] entry`), and a bundle ships no `vela.toml`. A lone `.velac` with no bundle around
//! it starts at its first label, which is what makes `Session::load("story.velac")` work.
//!
//! # Native only
//!
//! Reading a directory is `std::fs`, and a browser has no bundle directory: a page receives
//! module bytes from the network and constructs `vela_web::Player` from them directly
//! (`BUILD_AND_ASSETS.md §5`). Gating the loader off `wasm32` also keeps the JSON reader it needs
//! — the manifest is JSON — out of the size-gated wasm module, which is the reason the split is
//! drawn here rather than at the file system alone.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use vela_bytecode::{DecodeError, Module};

use crate::fault::Fault;

/// Where a bundle keeps its compiled modules.
const SCRIPTS: &str = "scripts";

/// The file a bundle describes itself in.
const MANIFEST: &str = "manifest.json";

/// Why a story could not be loaded from disk.
#[derive(Debug)]
pub enum LoadError {
    /// A file the loader needed could not be read.
    Io {
        /// The path it tried.
        path: PathBuf,
        /// What the operating system said.
        source: std::io::Error,
    },
    /// A directory was given that is not a bundle: it has no `manifest.json`.
    NotABundle(PathBuf),
    /// The manifest is not readable JSON.
    Manifest {
        /// The manifest it tried.
        path: PathBuf,
        /// What the JSON reader said.
        message: String,
    },
    /// Nothing says where the story starts.
    ///
    /// For a bundle, its manifest has no `entry`; for a lone module, it has no labels.
    NoEntry(PathBuf),
    /// The entry point is not `module.label`.
    BadEntry(String),
    /// The module is not a container this engine reads.
    ///
    /// Carries the codec's own error, which distinguishes a format this build is too old for
    /// (`E7101`) from bytes that are not a module at all (`E7103`).
    Decode(DecodeError),
    /// The entry label is not in the module it was loaded from.
    Fault(Fault),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "cannot read {}: {source}", path.display()),
            Self::NotABundle(path) => write!(
                f,
                "`{}` is not a bundle: it has no {MANIFEST}",
                path.display()
            ),
            Self::Manifest { path, message } => {
                write!(
                    f,
                    "{} is not a readable manifest: {message}",
                    path.display()
                )
            }
            Self::NoEntry(path) => write!(
                f,
                "`{}` does not say where the story starts",
                path.display()
            ),
            Self::BadEntry(entry) => {
                write!(
                    f,
                    "`{entry}` is not an entry point; expected `module.label`"
                )
            }
            Self::Decode(error) => write!(f, "{error}"),
            Self::Fault(fault) => write!(f, "{fault}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// The slice of a bundle manifest the loader reads.
///
/// Deliberately not `vela_assets::Manifest`: the VM must not depend on the import pipeline — a
/// renderer cannot be linked below rank 3 and neither should a PNG decoder be — and the one
/// field it needs is the entry point. An unknown field is ignored, which is what lets the
/// manifest grow without this type following it.
#[derive(Deserialize)]
struct Descriptor {
    #[serde(default)]
    entry: Option<String>,
}

/// Reads a story from a bundle directory or a compiled module file.
///
/// # Errors
///
/// Fails if the path cannot be read, a directory is not a bundle, a bundle names no entry, or a
/// module does not decode.
pub(crate) fn read(path: &Path) -> Result<(Module, String), LoadError> {
    if path.is_dir() {
        read_bundle(path)
    } else {
        let module = read_module(path)?;
        let label = first_label(&module, path)?;
        Ok((module, label))
    }
}

/// Reads the entry module a bundle's manifest names.
fn read_bundle(root: &Path) -> Result<(Module, String), LoadError> {
    let manifest = root.join(MANIFEST);
    let text = match fs::read_to_string(&manifest) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(LoadError::NotABundle(root.to_path_buf()));
        }
        Err(source) => {
            return Err(LoadError::Io {
                path: manifest,
                source,
            });
        }
    };

    let descriptor: Descriptor =
        serde_json::from_str(&text).map_err(|error| LoadError::Manifest {
            path: manifest.clone(),
            message: error.to_string(),
        })?;

    let entry = descriptor.entry.ok_or(LoadError::NoEntry(manifest))?;
    let (module, label) = entry
        .rsplit_once('.')
        .ok_or_else(|| LoadError::BadEntry(entry.clone()))?;

    // The bundle mirrors the source tree, so `main.start` is `scripts/main.velac`. The label is
    // checked against the module when the session starts, not guessed at here.
    let path = root.join(SCRIPTS).join(module).with_extension("velac");
    Ok((read_module(&path)?, label.to_string()))
}

/// Decodes one `.velac` container.
fn read_module(path: &Path) -> Result<Module, LoadError> {
    let bytes = fs::read(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    vela_bytecode::decode(&bytes).map_err(LoadError::Decode)
}

/// The label a lone module starts at: its first, in declaration order.
///
/// A bundle does not need this — its manifest says where to start — but a module with no bundle
/// around it has nowhere else to look, and "the first label" is the one answer that is a fact
/// about the file rather than a guess.
fn first_label(module: &Module, path: &Path) -> Result<String, LoadError> {
    module
        .labels
        .first()
        .and_then(|label| module.strings.get(label.name))
        .map(str::to_string)
        .ok_or_else(|| LoadError::NoEntry(path.to_path_buf()))
}
