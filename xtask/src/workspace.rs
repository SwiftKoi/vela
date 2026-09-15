//! Workspace discovery: which crates exist, what rank each declares, and which
//! internal edges exist between them.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Prefix that marks a dependency as belonging to this workspace.
pub const INTERNAL_PREFIX: &str = "vela-";

#[derive(Debug, Deserialize)]
struct RawManifest {
    package: RawPackage,
    #[serde(default)]
    dependencies: BTreeMap<String, toml::Value>,
    #[serde(default, rename = "dev-dependencies")]
    dev_dependencies: BTreeMap<String, toml::Value>,
    #[serde(default, rename = "build-dependencies")]
    build_dependencies: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Deserialize)]
struct RawPackage {
    name: String,
    #[serde(default)]
    metadata: RawMetadata,
}

#[derive(Debug, Default, Deserialize)]
struct RawMetadata {
    #[serde(default)]
    vela: RawVela,
}

#[derive(Debug, Default, Deserialize)]
struct RawVela {
    rank: Option<u32>,
    #[serde(default)]
    adapter: bool,
}

#[derive(Debug, Deserialize)]
struct RawWorkspaceFile {
    workspace: RawWorkspaceTable,
}

#[derive(Debug, Deserialize)]
struct RawWorkspaceTable {
    #[serde(default)]
    members: Vec<String>,
}

/// One workspace member.
#[derive(Debug)]
pub struct Crate {
    /// Package name, e.g. `vela-vm`.
    pub name: String,
    /// Path to its `Cargo.toml`.
    pub manifest: PathBuf,
    /// Raw manifest text, kept so violations can cite a line number.
    pub manifest_text: String,
    /// Declared rank, if any. Every `vela-*` crate must declare one.
    pub rank: Option<u32>,
    /// Whether this crate is a platform/GPU adapter (`ARCHITECTURE.md §1` rule 2).
    pub adapter: bool,
    /// Internal (`vela-*`) dependencies, deduplicated and sorted.
    pub internal_deps: Vec<String>,
    /// Number of entries in `[dependencies]`, for the budget in `REPO_LAYOUT.md §3`.
    pub normal_dep_count: usize,
}

impl Crate {
    /// Whether this crate participates in the rank graph.
    #[must_use]
    pub fn is_ranked(&self) -> bool {
        self.name.starts_with(INTERNAL_PREFIX)
    }

    /// 1-indexed line in the manifest containing `needle`, for actionable messages.
    #[must_use]
    pub fn line_of(&self, needle: &str) -> Option<usize> {
        self.manifest_text
            .lines()
            .position(|l| l.contains(needle))
            .map(|i| i + 1)
    }
}

/// Every member of the workspace.
#[derive(Debug)]
pub struct Workspace {
    /// Members, sorted by name for deterministic reporting.
    pub crates: Vec<Crate>,
}

impl Workspace {
    /// Loads the workspace rooted at `root`.
    pub fn load(root: &Path) -> Result<Self, String> {
        let root_manifest = root.join("Cargo.toml");
        let text = fs::read_to_string(&root_manifest)
            .map_err(|e| format!("cannot read {}: {e}", root_manifest.display()))?;
        let parsed: RawWorkspaceFile = toml::from_str(&text)
            .map_err(|e| format!("cannot parse {}: {e}", root_manifest.display()))?;

        let mut crates = Vec::new();
        for pattern in &parsed.workspace.members {
            for dir in expand_member(root, pattern)? {
                crates.push(load_crate(&dir)?);
            }
        }
        crates.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self { crates })
    }

    /// Finds a member by package name.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&Crate> {
        self.crates.iter().find(|c| c.name == name)
    }

    /// All ranked (`vela-*`) crates.
    pub fn ranked(&self) -> impl Iterator<Item = &Crate> {
        self.crates.iter().filter(|c| c.is_ranked())
    }
}

/// Expands a workspace member pattern (`crates/*` or a literal path) into directories.
fn expand_member(root: &Path, pattern: &str) -> Result<Vec<PathBuf>, String> {
    if !pattern.contains('*') {
        return Ok(vec![root.join(pattern)]);
    }

    let parent = pattern
        .split('*')
        .next()
        .unwrap_or("")
        .trim_end_matches('/');
    let dir = root.join(parent);
    let entries = fs::read_dir(&dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;

    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join("Cargo.toml").is_file() {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

/// Parses one crate's manifest.
fn load_crate(dir: &Path) -> Result<Crate, String> {
    let manifest = dir.join("Cargo.toml");
    let text = fs::read_to_string(&manifest)
        .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?;
    let raw: RawManifest =
        toml::from_str(&text).map_err(|e| format!("cannot parse {}: {e}", manifest.display()))?;

    let mut internal: Vec<String> = raw
        .dependencies
        .keys()
        .chain(raw.dev_dependencies.keys())
        .chain(raw.build_dependencies.keys())
        .filter(|k| k.starts_with(INTERNAL_PREFIX))
        .cloned()
        .collect();
    internal.sort();
    internal.dedup();

    Ok(Crate {
        name: raw.package.name,
        manifest,
        manifest_text: text,
        rank: raw.package.metadata.vela.rank,
        adapter: raw.package.metadata.vela.adapter,
        internal_deps: internal,
        normal_dep_count: raw.dependencies.len(),
    })
}
