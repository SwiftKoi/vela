//! Delta patches: what changed between two builds (`BUILD_AND_ASSETS.md §6.2`).
//!
//! A patch is computed from content, not from a diff algorithm over the files: every file in a
//! bundle already has a digest, so what a patch *is* falls out of comparing two sets of them.
//! Added and changed files are the ones whose digest the new bundle has and the old one does
//! not; removed files are the ones that went the other way; everything else is already on the
//! player's disk and shipping it would be shipping bytes twice.
//!
//! The bytes of a changed file are stored under their own digest, so two changed files with the
//! same content cost one blob — the same property that makes the manifest an identity.
//!
//! **Applying verifies before it writes.** Every blob is hashed against the digest the index
//! records, and the bundle being patched is checked against the base the patch was computed
//! from, *before* anything is overwritten. A patch that half-applied would leave a bundle that
//! is neither version, and "fetch the full build instead" is only a real option if the old one
//! is still there.
//!
//! Not yet: sub-file chunks. A changed file ships whole, so a one-byte edit to a large texture
//! costs the texture. The acceptance bar `VISION.md §5` sets — a text change under 5% of the
//! bundle — is met at this granularity, because a text change alters a script and nothing else.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::digest::Digest;
use crate::error::AssetError;

/// The patch format this build writes.
pub const PATCH_VERSION: u32 = 1;

/// The index file a patch directory holds.
const INDEX: &str = "patch.json";

/// Where a patch's blobs live, by digest.
const BLOBS: &str = "blobs";

/// What changed between two bundles.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Patch {
    /// The format version.
    pub patch_version: u32,
    /// The identity of the bundle this applies to.
    pub base: Digest,
    /// Files the new bundle adds or replaces, ordered by path.
    pub entries: Vec<Entry>,
    /// Files the new bundle no longer has, ordered by path.
    pub removed: Vec<String>,
}

/// One file a patch carries.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Entry {
    /// Its path in the bundle.
    pub path: String,
    /// The digest of its bytes — and so the name of the blob holding them.
    pub digest: Digest,
}

impl Patch {
    /// The difference between two bundles, written to `destination`.
    ///
    /// # Errors
    ///
    /// Fails if either bundle cannot be read, or the patch cannot be written.
    pub fn write_between(
        base: &Path,
        current: &Path,
        destination: &Path,
    ) -> Result<Self, AssetError> {
        let before = index(base)?;
        let after = index(current)?;

        let mut entries = Vec::new();
        for (path, digest) in &after {
            if before.get(path) == Some(digest) {
                continue;
            }
            entries.push(Entry {
                path: path.clone(),
                digest: digest.clone(),
            });
        }

        let removed = before
            .keys()
            .filter(|path| !after.contains_key(*path))
            .cloned()
            .collect();

        let patch = Self {
            patch_version: PATCH_VERSION,
            base: identity(&before),
            entries,
            removed,
        };

        patch.write(destination, current)?;
        Ok(patch)
    }

    /// Reads a patch's index.
    ///
    /// # Errors
    ///
    /// Fails if the index is missing, malformed, or written by a version this build does not
    /// know.
    pub fn read(root: &Path) -> Result<Self, AssetError> {
        let path = root.join(INDEX);
        let text = fs::read_to_string(&path).map_err(|source| AssetError::Io {
            path: path.clone(),
            source,
        })?;
        let patch: Self =
            serde_json::from_str(&text).map_err(|e| AssetError::Patch(e.to_string()))?;
        if patch.patch_version != PATCH_VERSION {
            return Err(AssetError::Patch(format!(
                "patch version {} is not supported; this build writes {PATCH_VERSION}",
                patch.patch_version
            )));
        }
        Ok(patch)
    }

    /// Writes the index and the blobs.
    ///
    /// # Errors
    ///
    /// Fails if a source file cannot be read or the patch cannot be written.
    fn write(&self, destination: &Path, current: &Path) -> Result<(), AssetError> {
        let blobs = destination.join(BLOBS);
        fs::create_dir_all(&blobs).map_err(|source| AssetError::Io {
            path: blobs.clone(),
            source,
        })?;

        for entry in &self.entries {
            let from = current.join(&entry.path);
            let bytes = fs::read(&from).map_err(|source| AssetError::Io {
                path: from.clone(),
                source,
            })?;
            let to = blobs.join(blob_name(&entry.digest));
            fs::write(&to, bytes).map_err(|source| AssetError::Io { path: to, source })?;
        }

        let text =
            serde_json::to_string_pretty(self).map_err(|e| AssetError::Patch(e.to_string()))?;
        let path = destination.join(INDEX);
        fs::write(&path, format!("{text}\n")).map_err(|source| AssetError::Io { path, source })
    }

    /// Applies a patch, read from `root`, to the bundle in `target`.
    ///
    /// Everything is verified first and written second, so a failed apply leaves the bundle
    /// exactly as it was rather than half of the new version. That ordering is the point:
    /// `BUILD_AND_ASSETS.md §6.2` promises a corrupted download is *detected* and the full build
    /// fetched instead, and that promise is empty if the detection happens after the damage.
    ///
    /// # Errors
    ///
    /// Fails if the bundle is not the one the patch was computed from, or a blob's bytes do not
    /// match the digest the index records.
    pub fn apply(&self, root: &Path, target: &Path) -> Result<(), AssetError> {
        let current = index(target)?;
        if identity(&current) != self.base {
            return Err(AssetError::Patch(
                "this patch is for a different build of the game".to_string(),
            ));
        }

        // Read and verify every blob up front.
        let mut pending = Vec::new();
        for entry in &self.entries {
            let path = root.join(BLOBS).join(blob_name(&entry.digest));
            let bytes = fs::read(&path).map_err(|source| AssetError::Io {
                path: path.clone(),
                source,
            })?;
            let found = Digest::of(&bytes);
            if found != entry.digest {
                return Err(AssetError::Patch(format!(
                    "`{}` does not match the digest the patch records ({found}, expected {})",
                    entry.path, entry.digest
                )));
            }
            pending.push((entry.path.clone(), bytes));
        }

        for (path, bytes) in pending {
            let full = target.join(&path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).map_err(|source| AssetError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            fs::write(&full, bytes).map_err(|source| AssetError::Io { path: full, source })?;
        }

        for path in &self.removed {
            let full = target.join(path);
            if full.exists() {
                fs::remove_file(&full).map_err(|source| AssetError::Io { path: full, source })?;
            }
        }
        Ok(())
    }

    /// The patch's size on disk, index and blobs together.
    ///
    /// # Errors
    ///
    /// Fails if the patch directory cannot be read.
    pub fn size(&self, root: &Path) -> Result<u64, AssetError> {
        Ok(measure(&root.join(INDEX))? + measure(&root.join(BLOBS))?)
    }
}

/// A bundle's identity: its paths and their digests, and nothing else.
///
/// Deliberately not the bytes: a patch applies to a build, and a build is identified by *what is
/// in it*. Two bundles with the same files are the same build however they were produced.
#[must_use]
pub fn identity(files: &BTreeMap<String, Digest>) -> Digest {
    let mut text = String::new();
    for (path, digest) in files {
        text.push_str(&format!("{path} {digest}\n"));
    }
    Digest::of(text.as_bytes())
}

/// The identity of the bundle in `root`.
///
/// # Errors
///
/// Fails if the directory cannot be read.
pub fn identity_of(root: &Path) -> Result<Digest, AssetError> {
    Ok(identity(&index(root)?))
}

/// Every file in a bundle, by path, with its digest.
///
/// Files are digested and dropped one at a time: a patch is computed over a whole build, and a
/// build is measured in gigabytes.
fn index(root: &Path) -> Result<BTreeMap<String, Digest>, AssetError> {
    let mut files = BTreeMap::new();
    for path in files_under(root)? {
        let full = root.join(&path);
        let bytes = fs::read(&full).map_err(|source| AssetError::Io {
            path: full.clone(),
            source,
        })?;
        files.insert(path, Digest::of(&bytes));
    }
    Ok(files)
}

/// A blob's file name: the digest without its algorithm prefix.
fn blob_name(digest: &Digest) -> String {
    digest
        .as_str()
        .split_once(':')
        .map_or_else(|| digest.as_str().to_string(), |(_, hex)| hex.to_string())
}

/// Every file under `root`, as paths relative to it with `/` separators.
fn files_under(root: &Path) -> Result<Vec<String>, AssetError> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];

    while let Some(dir) = pending.pop() {
        let entries = fs::read_dir(&dir).map_err(|source| AssetError::Io {
            path: dir.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| AssetError::Io {
                path: dir.clone(),
                source,
            })?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(name) = path.strip_prefix(root) {
                found.push(name.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    Ok(found)
}

/// The size of a path: a file's length, or everything under a directory.
fn measure(path: &Path) -> Result<u64, AssetError> {
    if path.is_file() {
        let meta = fs::metadata(path).map_err(|source| AssetError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        return Ok(meta.len());
    }

    let mut total = 0u64;
    for entry in fs::read_dir(path).map_err(|source| AssetError::Io {
        path: path.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| AssetError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        total += measure(&entry.path())?;
    }
    Ok(total)
}
