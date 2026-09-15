//! What the presentation has been told to do.
//!
//! The accumulated result of the `scene`, `show`, `hide`, and `play` commands. It lives in
//! `World` because it is *state*: a save that restores the dialogue but not the screen has
//! restored half a game.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What is on stage.
///
/// The accumulated result of the `scene`/`show`/`hide` commands. Plain data, like
/// everything else in `World`, which is what makes it snapshottable.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct SceneState {
    images: Vec<StagedImage>,
}

/// One image on stage.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct StagedImage {
    /// The image, as a dotted path.
    pub image: String,
    /// The attributes it was shown with.
    pub attributes: Vec<String>,
    /// The transforms applied.
    pub transforms: Vec<String>,
}

impl SceneState {
    /// Every image, in the order it was shown.
    #[must_use]
    pub fn images(&self) -> &[StagedImage] {
        &self.images
    }

    /// Replaces the scene.
    pub fn scene(&mut self, image: impl Into<String>) {
        self.images.clear();
        self.show(image);
    }

    /// Adds an image, replacing one with the same name.
    pub fn show(&mut self, image: impl Into<String>) {
        let image = image.into();
        self.images.retain(|staged| staged.image != image);
        self.images.push(StagedImage {
            image,
            attributes: Vec::new(),
            transforms: Vec::new(),
        });
    }

    /// Removes an image.
    pub fn hide(&mut self, image: &str) {
        self.images.retain(|staged| staged.image != image);
    }
}

/// What is playing.
///
/// One entry per channel, ordered by channel name so that serialization is reproducible.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct AudioState {
    channels: BTreeMap<String, Playing>,
}

/// One thing playing.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Playing {
    /// The source, as a path.
    pub source: String,
    /// Whether it loops.
    pub looping: bool,
}

impl AudioState {
    /// Starts something on a channel, replacing whatever was there.
    pub fn play(&mut self, channel: impl Into<String>, source: impl Into<String>, looping: bool) {
        self.channels.insert(
            channel.into(),
            Playing {
                source: source.into(),
                looping,
            },
        );
    }

    /// Stops a channel.
    pub fn stop(&mut self, channel: &str) {
        self.channels.remove(channel);
    }

    /// What is playing on a channel.
    #[must_use]
    pub fn playing(&self, channel: &str) -> Option<&Playing> {
        self.channels.get(channel)
    }

    /// Every channel, in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Playing)> {
        self.channels.iter()
    }
}
