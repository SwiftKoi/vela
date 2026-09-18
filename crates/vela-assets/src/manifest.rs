//! The asset manifest: what is in the game, content-addressed.
//!
//! `BUILD_AND_ASSETS.md §2`. The manifest is the single source of truth for a build's
//! contents: what the compiler resolves `@"path"` literals against, what a patch is computed
//! between, and what a player's install is verified with. So it holds *identity* — a digest per
//! source and per artifact — and no timestamps, absolute paths, or iteration order that a
//! second build on another machine could disagree with (`BUILD_AND_ASSETS.md §7`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::digest::Digest;
use crate::error::AssetError;

/// The manifest schema this build writes.
///
/// A reader that does not know a version refuses it rather than reading it as if the fields it
/// does not recognize were absent.
pub const MANIFEST_VERSION: u32 = 1;

/// Everything a build contains.
///
/// The manifest is both the asset index and the **bundle descriptor**: it is the one file a
/// built bundle carries about itself, so the runtime reads where the story starts from here
/// rather than from `vela.toml` (`BUILD_AND_ASSETS.md §2`, `RUNTIME.md §8`). The three project
/// fields below are absent from a bare `import_tree` — an import has no idea what project it
/// belongs to — and are filled in by `vela build`; because they are omitted while unset, a
/// manifest an import writes is byte-identical to one it wrote before they existed.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Manifest {
    /// The schema version.
    pub manifest_version: u32,
    /// The project's name, which a launcher and a window title use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Where the story starts, written `module.label`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// The frame the game was designed for, written `1280x720` (`SCREENS.md §2.6`).
    ///
    /// A bundle ships no source and no `vela.toml`, so this is the only way one can tell a screen
    /// what `variant("small")` measures against — and a bundle that recorded nothing would draw
    /// every frame as `small`, which is the wrong shape rather than a missing one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// Every `image` declaration, by name, mapped to the artifact that holds its picture.
    ///
    /// The runtime cannot resolve `scene bg.room` to `art/room.png` on its own: that mapping is
    /// a *project* fact, compiled from the source, and a bundle ships no source. Recording it
    /// here is what lets a built bundle stage its backgrounds without a compiler
    /// (`BUILD_AND_ASSETS.md §8`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub images: BTreeMap<String, String>,
    /// Every asset, ordered by id.
    pub assets: Vec<Asset>,
}

/// One source and what importing it produced.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Asset {
    /// The asset's name, unique within a build.
    pub id: String,
    /// Its path relative to the assets root, with `/` separators.
    pub source: String,
    /// The digest of the *source* bytes.
    pub digest: Digest,
    /// The artifacts it produced, ordered by path.
    pub artifacts: Vec<Artifact>,
    /// Total artifact size per target. Empty until targets exist, and omitted while it is.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub size: BTreeMap<String, u64>,
}

/// One file a build contains.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Artifact {
    /// Its path relative to the artifact root.
    pub path: String,
    /// The digest of its bytes.
    pub digest: Digest,
    /// What it is. A string, not an enum: importers are an extension point, so a plugin's own
    /// kind must not require editing a core enum.
    pub kind: String,
    /// Per-target alternatives, when a target needs different bytes. Empty until targets
    /// exist, and omitted while it is.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<Variant>,
}

/// An artifact that a particular target needs instead of the default one.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Variant {
    /// The target it is for.
    pub target: String,
    /// The digest of its bytes.
    pub digest: Digest,
}

impl Manifest {
    /// An empty manifest at the current version.
    #[must_use]
    pub fn new() -> Self {
        Self {
            manifest_version: MANIFEST_VERSION,
            name: None,
            entry: None,
            size: None,
            images: BTreeMap::new(),
            assets: Vec::new(),
        }
    }

    /// Records the project facts a built bundle carries about itself.
    ///
    /// Kept out of [`crate::import_tree`] on purpose: an import is a pure function of an assets
    /// directory, and a project's name, entry point and design size come from `vela.toml`, which
    /// the importer never sees.
    pub fn set_project(
        &mut self,
        name: Option<String>,
        entry: impl Into<String>,
        size: Option<String>,
    ) {
        self.name = name;
        self.entry = Some(entry.into());
        self.size = size;
    }

    /// The artifact holding an image, by the name a `scene` or `show` uses.
    #[must_use]
    pub fn image(&self, name: &str) -> Option<&str> {
        self.images.get(name).map(String::as_str)
    }

    /// The first artifact an asset's source produced, which is what a `@"path"` becomes.
    #[must_use]
    pub fn artifact_for_source(&self, source: &str) -> Option<&str> {
        self.assets
            .iter()
            .find(|asset| asset.source == source)
            .and_then(|asset| asset.artifacts.first())
            .map(|artifact| artifact.path.as_str())
    }

    /// The asset with this id, if it has one.
    #[must_use]
    pub fn asset(&self, id: &str) -> Option<&Asset> {
        self.assets.iter().find(|asset| asset.id == id)
    }

    /// Whether a source path is in this build.
    ///
    /// The question `@"path"` asks (`LANGUAGE.md §7.5`), which is why it is by *source*: an
    /// author writes the file they have, not the artifact it becomes.
    #[must_use]
    pub fn has_source(&self, source: &str) -> bool {
        self.assets.iter().any(|asset| asset.source == source)
    }

    /// Every artifact in the build, ordered by path.
    pub fn artifacts(&self) -> impl Iterator<Item = &Artifact> {
        self.assets.iter().flat_map(|asset| asset.artifacts.iter())
    }

    /// The manifest as canonical JSON.
    ///
    /// Sorted and complete, with no machine-dependent field anywhere, so two builds of one
    /// source produce the same bytes and a diff between two manifests is a diff between two
    /// builds.
    ///
    /// # Errors
    ///
    /// Fails only if a digest cannot be written as a JSON string, which cannot happen.
    pub fn to_json(&self) -> Result<String, AssetError> {
        let text =
            serde_json::to_string_pretty(self).map_err(|e| AssetError::Manifest(e.to_string()))?;
        Ok(format!("{text}\n"))
    }

    /// Reads a manifest back.
    ///
    /// # Errors
    ///
    /// Fails on malformed JSON, a digest that is not a `sha256:` hash, or a version this build
    /// does not write.
    pub fn from_json(text: &str) -> Result<Self, AssetError> {
        let manifest: Self =
            serde_json::from_str(text).map_err(|e| AssetError::Manifest(e.to_string()))?;
        if manifest.manifest_version != MANIFEST_VERSION {
            return Err(AssetError::Manifest(format!(
                "manifest version {} is not supported; this build writes {MANIFEST_VERSION}",
                manifest.manifest_version
            )));
        }
        Ok(manifest)
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self::new()
    }
}
