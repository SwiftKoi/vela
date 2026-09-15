//! Textures: PNG, validated and normalized.
//!
//! `BUILD_AND_ASSETS.md §3.1` asks a texture importer for `ktx2` — Basis, BC7, or ASTC per
//! target. That needs a GPU texture transcoder, which this build does not have, so what this
//! importer does is the half that can be done honestly today and is worth doing either way:
//!
//! * **It is the format's real detector.** PNG has magic bytes, so a `.png` that is not one is
//!   caught by what it *is* rather than by its name — the rule §3.1 states and, until now, the
//!   registry could only be tested for.
//! * **It normalizes.** The image is decoded and re-encoded with its own colour type and bit
//!   depth and nothing else: no ancillary chunks, no colour profiles, no creation timestamps,
//!   no editor metadata. Two exports of one image that differ only in the metadata a tool wrote
//!   into them import to the same artifact, which is what makes the digest an identity rather
//!   than a record of who last opened the file.
//! * **It refuses a broken one.** A truncated or malformed PNG fails the build with the file
//!   named, instead of becoming a blank rectangle in front of a playtester.
//!
//! What it deliberately does *not* do is transcode, because a transcoder that is not there
//! cannot be approximated: a file named `.ktx2` that holds PNG bytes would pass every check in
//! this repository and fail on a player's GPU.

use std::io::Cursor;

use crate::error::AssetError;
use crate::importers::registry::{ImportRequest, Importer, Output};

/// The eight bytes every PNG starts with.
const MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// The texture importer.
pub struct Texture;

impl Importer for Texture {
    fn kind(&self) -> &'static str {
        "texture"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["png"]
    }

    fn probe(&self, bytes: &[u8]) -> bool {
        bytes.starts_with(MAGIC)
    }

    fn import(&self, request: &ImportRequest<'_>) -> Result<Vec<Output>, AssetError> {
        let path = std::path::PathBuf::from(request.source);
        let refuse = |message: String| AssetError::Import {
            path: path.clone(),
            kind: "texture",
            message,
        };

        let decoder = png::Decoder::new(Cursor::new(request.bytes));
        let mut reader = decoder
            .read_info()
            .map_err(|error| refuse(format!("not a PNG: {error}")))?;

        // `None` means the header did not say enough to size the output — which is the same
        // class of problem as a decode failure, and is reported the same way.
        let capacity = reader
            .output_buffer_size()
            .ok_or_else(|| refuse("the header does not describe an image".to_string()))?;
        let mut pixels = vec![0; capacity];
        let info = reader
            .next_frame(&mut pixels)
            .map_err(|error| refuse(format!("the image does not decode: {error}")))?;
        pixels.truncate(info.buffer_size());

        // Re-encoded with no transformations: the colour type and bit depth are the ones the
        // file already had, so a palette image stays a palette image rather than quadrupling on
        // the way through.
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, info.width, info.height);
            encoder.set_color(info.color_type);
            encoder.set_depth(info.bit_depth);
            let mut writer = encoder
                .write_header()
                .map_err(|error| refuse(format!("the image does not encode: {error}")))?;
            writer
                .write_image_data(&pixels)
                .map_err(|error| refuse(format!("the image does not encode: {error}")))?;
        }

        Ok(vec![Output {
            path: format!("{}{}.png", request.directory(), request.stem()),
            kind: self.kind(),
            bytes,
        }])
    }
}
