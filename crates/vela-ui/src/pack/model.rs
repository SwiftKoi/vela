//! What a screen pack holds, and the file it lives in.

use std::fs;
use std::path::Path;

use vela_syntax::{Item, ScreenDecl, StyleDecl};

use crate::error::PackError;
use crate::pack::{read, write};
use crate::screens::ScreenSet;
use crate::theme::{Fonts, Palette};

/// The pack format this build writes and the oldest it reads.
///
/// Bumped for any change to the shape below — including a change to the declaration fields the
/// pack carries, because those *are* the format. A newer pack is refused, not guessed at.
/// Version 2 adds a tag for `UnOp::Bang`: a pack can now carry a negation the reader of version 1
/// would have read as `not`, which is a *different* operator, so an old runtime must refuse rather
/// than misread it.
/// Version 3 adds a presence flag before a parameter's type, because a parameter may be written
/// without one (`LANGUAGE.md §3`). A version-2 reader would take that flag byte for a type tag, so
/// an old runtime must refuse rather than misread it.
/// Version 4 adds the two composition lines — `use` and `transclude` (`SCREENS.md §2`). A
/// version-3 reader would read either tag as a widget, which is a *different* tree, so an old
/// runtime must refuse rather than misread it.
/// Version 5 adds `style_prefix` (`SCREENS.md §5.2`). A version-4 reader would read it as a widget
/// named `""`, which draws nothing and would leave the screens it skins unskinned — a wrong screen
/// rather than a refused one.
/// Version 6 adds the theme's `font` tokens (`SCREENS.md §5`) as a fifth section. A version-5 reader
/// would read the four sections it knows and never see the table — it does not require the container
/// to be exhausted — leaving every `font = theme.<token>` falling back to the default font. A wrong
/// screen, so an old runtime must refuse rather than misread it.
/// Version 7 adds an `if`'s `elif` arms and its `else` (`SCREENS.md §4`). A version-6 reader would take
/// the `elif` count for the next line's tag, so a screen with an `elif` would decode as a tree the
/// author did not write — a wrong screen rather than a refused one.
/// Version 8 adds the two input lines, `key` and `timer` (`SCREENS.md §2.3`). A version-7 reader would
/// read either tag as a widget named `""`, which draws nothing — so it must refuse rather than draw a
/// screen with its bindings quietly missing.
pub const PACK_VERSION: u16 = 8;

/// The four bytes every pack starts with.
///
/// `VELS` — Vela screens — so a file that is not a pack says so before anything reads a length out
/// of it.
pub const MAGIC: [u8; 4] = *b"VELS";

/// A module's screens, in the form a bundle ships.
#[derive(Clone, Debug)]
pub struct PackedSet {
    /// The declared screens.
    pub screens: Vec<ScreenDecl>,
    /// The styles they resolve against.
    pub styles: Vec<StyleDecl>,
    /// The active theme's colours.
    pub palette: Palette,
    /// The active theme's font tokens, which a style's `font` names (`SCREENS.md §5`).
    pub fonts: Fonts,
}

/// One module's screens, versioned and named.
#[derive(Clone, Debug)]
pub struct ScreenPack {
    /// The format version.
    pub pack_version: u16,
    /// The module the screens came from, so a message can say where a screen is.
    pub module: String,
    /// What it declares.
    pub set: PackedSet,
}

impl ScreenPack {
    /// Compiles a module's parsed items into a pack.
    ///
    /// This is the one place a screen is *compiled*: everything after it — every run from a bundle
    /// — decodes the result instead. A module that declares no screens, styles, or theme still
    /// produces a pack of nothing, which [`Self::is_empty`] lets the caller skip writing.
    #[must_use]
    pub fn compile(module: impl Into<String>, items: &[Item]) -> Self {
        Self {
            pack_version: PACK_VERSION,
            module: module.into(),
            set: ScreenSet::from_items(items).packed(),
        }
    }

    /// Whether this module declared nothing at all for the interface.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.set.screens.is_empty()
            && self.set.styles.is_empty()
            && self.set.palette.is_empty()
            && self.set.fonts.is_empty()
    }

    /// The screens, ready to lay out.
    #[must_use]
    pub fn into_set(self) -> ScreenSet {
        ScreenSet::from_packed(self.set)
    }

    /// The pack as the bytes a bundle stores.
    ///
    /// Encoding cannot fail: a pack is plain data with no reference to the outside world, which is
    /// why this returns bytes rather than a `Result` while decoding does not.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        write::encode(self)
    }

    /// Reads a pack, refusing a version this build does not read.
    ///
    /// # Errors
    ///
    /// Fails on a bad magic number, a truncated container, a checksum that does not match, a
    /// version this build does not know, or a section that does not describe a screen.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PackError> {
        read::decode(bytes)
    }

    /// Writes the pack to `path`, creating its parents.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn write(&self, path: &Path) -> Result<(), PackError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| PackError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(path, self.to_bytes()).map_err(|source| PackError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Reads the pack at `path`.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be read, or [`Self::from_bytes`] would.
    pub fn read(path: &Path) -> Result<Self, PackError> {
        let bytes = fs::read(path).map_err(|source| PackError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_bytes(&bytes)
    }
}
