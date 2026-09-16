//! The assets a Ren'Py project has, and what the migration does with them.
//!
//! Two answers, split by whether Vela can *use* the file.
//!
//! **Images are migrated.** Ren'Py defines an image automatically from an image file's name, so
//! `bg lecturehall.jpg` is the image `bg lecturehall` and is what `scene bg lecturehall` means.
//! Vela has no such rule, so the migration copies the file and writes the `image` declaration —
//! which is what makes the picture appear rather than the name resolving to a placeholder.
//!
//! **Everything else is inventoried and reported.** `vela check` **imports** every file under
//! `assets/`, so a file no importer claims is a hard error rather than a skipped one; and the GUI
//! skin belongs to Ren'Py's screens, which Vela replaces rather than reads. Copying either would
//! turn a migration that compiles into one that does not, which is worse than naming them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::report::Report;

/// One media file the project has, and what the migration did with it.
#[derive(Clone, Debug)]
pub struct Asset {
    /// The path, relative to `game/`.
    pub path: String,
    /// What kind of thing it is, from its extension.
    pub kind: &'static str,
}

/// An image the migration copies, and the name it declares it under.
///
/// Ren'Py defines an image **automatically** from an image file's name — the words of the file
/// name are the image's name and its attributes, so `bg lecturehall.jpg` is `bg lecturehall` and is
/// what `scene bg lecturehall` means. Vela has no such rule (an `image` declaration is written by
/// hand), so the migration reproduces it: the file is copied and the declaration is generated.
#[derive(Clone, Debug)]
pub struct Image {
    /// Where it comes from.
    pub from: PathBuf,
    /// Where it goes, relative to the migrated project: `assets/images/bg lecturehall.jpg`.
    pub to: String,
    /// The dotted name the file's words make: `bg.lecturehall`.
    pub name: String,
}

/// The dotted image name Ren'Py derives from a file name.
///
/// `None` when the name has nothing in it — a file called `.png` — which is a file Ren'Py would
/// refuse as an image too.
pub(crate) fn image_name(path: &str) -> Option<String> {
    let stem = Path::new(path).file_stem()?.to_string_lossy();
    let words: Vec<&str> = stem.split_whitespace().collect();
    (!words.is_empty()).then(|| words.join("."))
}

/// What kind of media an extension is, if it is one.
pub(crate) fn media(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "png" | "jpg" | "jpeg" | "webp" | "ktx2" => "image",
        "opus" | "ogg" | "mp3" | "wav" => "audio",
        "ttf" | "otf" => "font",
        "webm" | "mp4" => "video",
        _ => return None,
    })
}

/// Splits the media into the images to migrate and the rest of the inventory.
///
/// An image under `images/` is one Ren'Py would have defined automatically, so it is copied and
/// declared. Everything else — the GUI skin, the music — is inventoried and reported: the skin
/// belongs to Ren'Py's screens rather than to Vela's, and there is no audio importer yet, so
/// copying either would break `vela check` rather than make the project look right.
pub(crate) fn partition(assets: Vec<Asset>, game: &Path) -> (Vec<Image>, Vec<Asset>) {
    let mut images = Vec::new();
    let mut inventory = Vec::new();
    for asset in assets {
        match (asset.kind, image_name(&asset.path)) {
            ("image", Some(name)) if asset.path.starts_with("images/") => images.push(Image {
                from: game.join(&asset.path),
                to: format!("assets/{}", asset.path),
                name,
            }),
            _ => inventory.push(asset),
        }
    }
    (images, inventory)
}

/// Reports the media that was not copied, one entry per *kind*.
///
/// Per kind rather than per file: Ren'Py's default GUI ships 48 skin images, and 48 entries saying
/// the same thing is a report nobody reads.
pub(crate) fn report_inventory(inventory: &[Asset], report: &mut Report) {
    let mut kinds: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for asset in inventory {
        kinds
            .entry(asset.kind)
            .or_default()
            .push(asset.path.as_str());
    }

    for (kind, paths) in kinds {
        let reason = match kind {
            "image" => {
                "the GUI skin: these pictures belong to Ren'Py's screens, and Vela draws its own — \
                 port the screens and their images by hand"
            }
            "audio" => {
                "audio: this build has no audio importer yet (`M09-build.md`, item 1), and copying \
                 it would make `vela check` fail rather than make the project play sound"
            }
            _ => "no Vela counterpart",
        };
        report.push(
            "assets",
            1,
            &format!("{} {} file(s), e.g. {}", paths.len(), kind, paths[0]),
            reason,
        );
    }
}

/// The `image` declarations for the copied files, as a module of their own.
///
/// A module rather than the story file: these are not story, and a project with three modules'
/// worth of pictures would bury the dialogue.
pub(crate) fn declarations(images: &[Image]) -> String {
    let mut out = String::from(
        "# Image declarations, derived from the images' own file names. Ren'Py defines an image\n\
         # automatically from the file name and Vela does not, so the migration writes them out;\n\
         # the story's `scene`/`show` names are rewritten to match.\n",
    );
    for image in images {
        let path = image.to.trim_start_matches("assets/");
        out.push_str(&format!("image {} = @\"{path}\"\n", image.name));
    }
    out
}
