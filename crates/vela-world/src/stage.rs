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

    /// Adds an image, replacing one with the same **tag**.
    ///
    /// The tag is the first component of the name, which is Ren'Py's rule and the reason a second
    /// expression is the same sprite with a different face: `show sylvie.green.smile` after
    /// `show sylvie.green.normal` is one Sylvie, not two. Replacing in place rather than appending
    /// and dropping keeps the order — a sprite shown in front stays in front when its expression
    /// changes.
    pub fn show(&mut self, image: impl Into<String>) {
        let image = image.into();
        let tag = tag_of(&image);
        // A bare tag means "the same sprite, as it is": Ren'Py keeps the attributes an earlier `show`
        // set, so `show sylvie green normal` followed by `show sylvie` is the same Sylvie — and the
        // *name* is what has to stay, because in Vela the attributes are part of it. Replacing the
        // name with the tag would stage a picture nothing declares.
        let keeps_its_face = image == tag;
        match self
            .images
            .iter_mut()
            .find(|staged| tag_of(&staged.image) == tag)
        {
            Some(staged) => {
                if !keeps_its_face {
                    staged.image = image;
                    staged.attributes.clear();
                }
            }
            None => self.images.push(StagedImage {
                image,
                attributes: Vec::new(),
                transforms: Vec::new(),
            }),
        }
    }

    /// Removes an image, by tag or by full name: `hide sylvie` and `hide sylvie.green.smile` are
    /// the same instruction, which is what a layered name means.
    pub fn hide(&mut self, image: &str) {
        let tag = tag_of(image);
        self.images
            .retain(|staged| staged.image != image && tag_of(&staged.image) != tag);
    }
}

/// The tag of a dotted image name: the part before the first dot.
fn tag_of(image: &str) -> &str {
    image.split('.').next().unwrap_or(image)
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
