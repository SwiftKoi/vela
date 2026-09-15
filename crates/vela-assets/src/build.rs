//! Importing a tree: an assets directory in, a manifest and its artifacts out.
//!
//! `BUILD_AND_ASSETS.md §1`. This is the half of the pipeline that has nothing to do with
//! scripts, which is what makes the two cacheable apart: a change to a `.vela` file does not
//! re-import an asset, and a change to an asset does not recompile a module.
//!
//! Everything here is ordered on purpose. The directory walk is sorted, assets are sorted by
//! id, and artifacts are sorted by path, because `Manifest` is compared byte-for-byte between
//! two builds and a filesystem's order is not a fact about the project.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::digest::Digest;
use crate::error::AssetError;
use crate::importers::ImporterRegistry;
use crate::manifest::{Artifact, Asset, MANIFEST_VERSION, Manifest};

/// A manifest and the artifact bytes it describes.
///
/// The bytes are returned rather than written: where a build's output goes is `vela build`'s
/// question, and a library that decided it would be a library that cannot be pointed at a
/// temporary directory by its own tests.
pub struct Built {
    /// What was imported.
    pub manifest: Manifest,
    /// Each artifact's bytes, by the path the manifest records.
    pub artifacts: Vec<(String, Vec<u8>)>,
}

impl std::fmt::Debug for Built {
    /// Deliberately not derived: an artifact is a whole file, and a `Result` that panicked
    /// while printing one would bury the message that mattered under megabytes of texture.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Built")
            .field("manifest", &self.manifest)
            .field("artifacts", &self.artifacts.len())
            .finish()
    }
}

/// Imports every file under `root`.
///
/// Files and directories whose name begins with `.` are skipped. That is a convention rather
/// than a guess — every other tree-walking tool skips them, and a build that failed on an
/// editor's swap file would teach people to keep assets somewhere else.
///
/// # Errors
///
/// Fails when a file cannot be read, when nothing claims one, when an importer refuses one, or
/// when two sources want the same id.
pub fn import_tree(root: &Path, registry: &ImporterRegistry) -> Result<Built, AssetError> {
    let mut paths = Vec::new();
    walk(root, root, &mut paths)?;
    paths.sort();

    let mut assets = Vec::with_capacity(paths.len());
    let mut artifacts: Vec<(String, Vec<u8>)> = Vec::new();
    let mut claimed: BTreeMap<String, String> = BTreeMap::new();

    for relative in &paths {
        let path = root.join(relative);
        let bytes = fs::read(&path).map_err(|source| AssetError::Io {
            path: path.clone(),
            source,
        })?;
        let source = slash(relative);

        let id = id_of(&source);
        if let Some(first) = claimed.get(&id) {
            return Err(AssetError::DuplicateId {
                id,
                first: first.clone(),
                second: source,
            });
        }
        claimed.insert(id.clone(), source.clone());

        let mut records = Vec::new();
        for output in registry.import(&source, &bytes)? {
            records.push(Artifact {
                digest: Digest::of(&output.bytes),
                path: output.path.clone(),
                kind: output.kind.to_string(),
                variants: Vec::new(),
            });
            artifacts.push((output.path, output.bytes));
        }
        records.sort_by(|a, b| a.path.cmp(&b.path));

        assets.push(Asset {
            id,
            source,
            digest: Digest::of(&bytes),
            artifacts: records,
            size: BTreeMap::new(),
        });
    }

    assets.sort_by(|a, b| a.id.cmp(&b.id));
    artifacts.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(Built {
        manifest: Manifest {
            manifest_version: MANIFEST_VERSION,
            assets,
        },
        artifacts,
    })
}

/// The id an author would use for a source: its path with the extension dropped and the
/// separators made dots, so `art/forest.png` is `art.forest`.
///
/// Derived from the path rather than declared, because nothing declares it yet — a project's
/// `image` declarations name *images*, not files. When they meet, the declaration wins and this
/// becomes the fallback; the field is in the manifest either way, which is why the manifest is
/// not the thing that changes.
fn id_of(source: &str) -> String {
    let without_extension = match source.rfind('.') {
        Some(at) if !source[at..].contains('/') => &source[..at],
        _ => source,
    };
    without_extension.replace('/', ".")
}

/// A path with `/` separators, whatever the platform used.
fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Every file under `root`, as paths relative to it.
///
/// The order is whatever the filesystem gave, because the caller sorts and a second sort here
/// would only hide it if it did not.
fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), AssetError> {
    let entries = fs::read_dir(dir).map_err(|source| AssetError::Io {
        path: dir.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| AssetError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let hidden = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with('.'));
        if hidden {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out)?;
        } else {
            out.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
    Ok(())
}
