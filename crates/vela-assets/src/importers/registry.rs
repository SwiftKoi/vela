//! The importer registry: which importer claims a source, and how one is chosen.
//!
//! `BUILD_AND_ASSETS.md §3.1`: selection is **magic bytes first, extension second**. The order
//! is the point — a `.png` that is really a JPEG is imported as what it *is*, because the
//! alternative is decoding it as a texture and reporting a corrupt file, which sends an author
//! looking at the wrong thing.
//!
//! The rule lives here, once. An importer declares what it claims; it is never named in an
//! `if` anywhere else, which is what makes `CONVENTIONS.md §4.4` a three-file checklist.

use std::path::Path;

use crate::error::AssetError;

/// One source, as an importer sees it.
#[derive(Clone, Copy, Debug)]
pub struct ImportRequest<'a> {
    /// The source's path relative to the assets root, with `/` separators.
    ///
    /// Relative and normalized because it is written into the manifest, and a manifest with an
    /// absolute path in it is not reproducible on another machine (`BUILD_AND_ASSETS.md §7`).
    pub source: &'a str,
    /// The source's bytes.
    pub bytes: &'a [u8],
}

impl ImportRequest<'_> {
    /// The source's final extension, lowercased.
    #[must_use]
    pub fn extension(&self) -> String {
        Path::new(self.source)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
    }

    /// The source without its directory or extension — what an artifact is named after.
    #[must_use]
    pub fn stem(&self) -> &str {
        Path::new(self.source)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("asset")
    }

    /// The source's directory, with a trailing `/`, or empty at the root.
    #[must_use]
    pub fn directory(&self) -> String {
        match self.source.rfind('/') {
            Some(at) => self.source[..=at].to_string(),
            None => String::new(),
        }
    }
}

/// One artifact an importer produced.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Output {
    /// Its path relative to the artifact root.
    pub path: String,
    /// What it is, for the manifest's `kind`.
    ///
    /// A string rather than an enum: importers are an extension point
    /// (`ARCHITECTURE.md §6.4`), so a plugin's own kind must not require editing a core enum.
    pub kind: &'static str,
    /// Its bytes.
    pub bytes: Vec<u8>,
}

/// Something that turns a source file into artifacts.
///
/// Implementations must be pure functions of the request (`BUILD_AND_ASSETS.md §3.3`): the same
/// bytes must import to the same artifacts on every machine, or the manifest stops being
/// content-addressed and a delta patch starts shipping files that are already there.
pub trait Importer: Send + Sync {
    /// What it produces, for the manifest's `kind`.
    fn kind(&self) -> &'static str;

    /// The extensions it claims, when the magic bytes are inconclusive.
    fn extensions(&self) -> &'static [&'static str];

    /// Whether these bytes are its format.
    fn probe(&self, bytes: &[u8]) -> bool;

    /// Imports a source.
    ///
    /// # Errors
    ///
    /// Fails when the source is claimed but malformed. A source it does not claim never
    /// reaches it — the registry decides that.
    fn import(&self, request: &ImportRequest<'_>) -> Result<Vec<Output>, AssetError>;
}

/// Every importer this build has, and the rule for choosing one.
pub struct ImporterRegistry {
    importers: Vec<Box<dyn Importer>>,
}

impl ImporterRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            importers: Vec::new(),
        }
    }

    /// Adds an importer.
    ///
    /// Order is not significant: selection is by what an importer *claims*, and two importers
    /// claiming the same magic bytes would be a bug the first probe would hide. A test
    /// asserts the built-in set has no such overlap.
    pub fn register(&mut self, importer: Box<dyn Importer>) {
        self.importers.push(importer);
    }

    /// The importers this build ships.
    #[must_use]
    pub fn builtin() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(crate::importers::texture::Texture));
        registry.register(Box::new(crate::importers::data::Data));
        registry
    }

    /// Every importer, in registration order.
    pub fn importers(&self) -> impl Iterator<Item = &dyn Importer> {
        self.importers.iter().map(AsRef::as_ref)
    }

    /// Chooses an importer for a source: magic bytes first, extension second.
    ///
    /// `None` means nothing in this build handles the file, which is a report rather than a
    /// silent skip — an asset that is quietly dropped from the build is a blank rectangle in
    /// front of a playtester.
    #[must_use]
    pub fn select(&self, source: &str, bytes: &[u8]) -> Option<&dyn Importer> {
        let extension = Path::new(source)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        self.importers()
            .find(|importer| importer.probe(bytes))
            .or_else(|| {
                self.importers()
                    .find(|importer| importer.extensions().contains(&extension.as_str()))
            })
    }

    /// Imports one source, or explains why it could not.
    ///
    /// # Errors
    ///
    /// Fails when nothing claims the source, or when the importer that does refuses it.
    pub fn import(&self, source: &str, bytes: &[u8]) -> Result<Vec<Output>, AssetError> {
        let Some(importer) = self.select(source, bytes) else {
            return Err(AssetError::Unsupported {
                path: Path::new(source).to_path_buf(),
            });
        };
        let request = ImportRequest { source, bytes };
        importer.import(&request)
    }
}

impl Default for ImporterRegistry {
    fn default() -> Self {
        Self::new()
    }
}
