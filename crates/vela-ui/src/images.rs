//! The pictures a screen may draw, and where they are (`SCREENS.md §3`).
//!
//! A screen names a picture — `image bg.room`, or `image option.icon` when the name came from data —
//! and something has to turn that name into pixels. That something is the platform, not this crate: the
//! texture lives in the renderer's GPU memory and only the party that uploaded it knows its id. So the
//! mapping is **injected**, the same shape the theme's palette and the engine's fonts already have, and
//! this crate never loads a picture or touches a file.
//!
//! Two questions are answered by one table because they are asked at different moments and must agree:
//! *how big* a picture is (asked while a screen is built, so layout has a size) and *which texture* it
//! is (asked while a screen is painted, so there is something to sample). A second table would be a
//! second chance for a name to resolve in one and not the other, which is a screen that lays out
//! around a picture it does not draw.
//!
//! A name that is not here is not an error at this level: the checker is what reports a reference
//! nothing declares, and a screen whose picture has not arrived yet draws nothing rather than failing.

/// One uploaded picture.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Picture {
    /// Which uploaded texture it samples, as the renderer numbered it.
    pub texture: u32,
    /// Its width, in pixels — what layout measures it at.
    pub width: u32,
    /// Its height, in pixels.
    pub height: u32,
}

/// The pictures a screen may draw, by the name it refers to them by.
///
/// Ordered rather than hashed, like every other table that reaches output: two runs of one build must
/// resolve the same name to the same texture, and a table that iterated unpredictably could not be
/// printed, diffed, or golden-tested.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ImageTable {
    pictures: Vec<(String, Picture)>,
}

impl ImageTable {
    /// No pictures at all — what a project with none, or a headless run, gets.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A table of these pictures, in this order.
    #[must_use]
    pub fn from_entries(pictures: impl IntoIterator<Item = (String, Picture)>) -> Self {
        Self {
            pictures: pictures.into_iter().collect(),
        }
    }

    /// Registers a picture under a name, replacing any of the same name.
    pub fn insert(&mut self, name: impl Into<String>, picture: Picture) {
        let name = name.into();
        if let Some(existing) = self.pictures.iter_mut().find(|(known, _)| *known == name) {
            existing.1 = picture;
            return;
        }
        self.pictures.push((name, picture));
    }

    /// The picture a name refers to, if the platform supplied one.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Picture> {
        self.pictures
            .iter()
            .find(|(known, _)| known == name)
            .map(|(_, picture)| *picture)
    }

    /// Whether nothing is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pictures.is_empty()
    }

    /// How many pictures there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pictures.len()
    }

    /// The names, in order, for a diagnostic or a completion list.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.pictures
            .iter()
            .map(|(name, _)| name.as_str())
            .collect()
    }
}
