//! Decoding a texture to pixels, for something that has to draw it.
//!
//! The importer in `importers/texture.rs` answers a *build* question — what does this source
//! become in the bundle — and normalizes toward it. This answers the runtime's: what are the
//! pixels? They are different questions with different outputs, which is why they are two
//! functions rather than one, and why the one thing they share is the decoder.
//!
//! It lives here rather than in `vela-render` because the renderer is an adapter that must not
//! know a file format (`ARCHITECTURE.md §1`, rule 2) — and rather than in the CLI because a
//! second caller, a tool that inspects an asset, would want the same answer.

use std::io::Cursor;

use crate::error::AssetError;

/// A texture's pixels, as the GPU wants them.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `RGBA8`, four bytes per pixel, row-major, top row first.
    pub rgba: Vec<u8>,
}

impl Image {
    /// How many bytes one row takes.
    #[must_use]
    pub fn stride(&self) -> usize {
        self.width as usize * 4
    }
}

/// Decodes a PNG to `RGBA8`.
///
/// Alpha is forced on: a PNG without an alpha channel is opaque, and a caller that had to know
/// which colour types carry alpha would be a caller that could get it wrong.
///
/// # Errors
///
/// Fails if the bytes are not a PNG, or the image does not decode — a truncated or malformed
/// file is reported with what was wrong with it rather than as a blank rectangle.
pub fn decode_png(bytes: &[u8]) -> Result<Image, AssetError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    // Expand to RGB or RGBA: the GPU texture is four components, and `EXPAND` is what turns a
    // palette or a sub-byte greyscale into one of those.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|error| AssetError::Codec(format!("not a PNG: {error}")))?;

    let capacity = reader
        .output_buffer_size()
        .ok_or_else(|| AssetError::Codec("the header does not describe an image".to_string()))?;
    let mut pixels = vec![0; capacity];
    let info = reader
        .next_frame(&mut pixels)
        .map_err(|error| AssetError::Codec(format!("the image does not decode: {error}")))?;

    let (width, height) = (info.width, info.height);
    let mut rgba = match info.color_type {
        png::ColorType::Rgba => pixels,
        // Greyscale, RGB, and the two-channel forms the decoder can produce.
        _ => expand_to_rgba(&pixels, info.color_type, width, height),
    };
    rgba.truncate(width as usize * height as usize * 4);

    Ok(Image {
        width,
        height,
        rgba,
    })
}

/// Decodes an image to `RGBA8`, whichever format it is.
///
/// The format is decided by the file's **magic bytes**, not by its name — the same rule the
/// importer registry selects an importer by (`BUILD_AND_ASSETS.md §3.1`), so a `.png` that holds
/// JPEG bytes is decoded as what it is rather than refused for what it says it is.
///
/// This is the entry point the *runtime* wants: `importers/texture.rs` answers the build's question
/// (what does this source become in a bundle) and this answers the presenter's (what are the
/// pixels), and a caller that has bytes in hand should not have to know which of the two it is
/// holding.
///
/// # Errors
///
/// Fails if the bytes are not an image this build decodes, or the image is malformed.
pub fn decode(bytes: &[u8]) -> Result<Image, AssetError> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return decode_png(bytes);
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return decode_jpeg(bytes);
    }
    Err(AssetError::Codec(
        "not an image this build reads (PNG and JPEG)".to_string(),
    ))
}

/// Decodes a JPEG to `RGBA8`.
///
/// # Errors
///
/// Fails on a malformed file, and on a CMYK one: `jpeg-decoder` decodes those to four channels
/// that are not RGBA, and guessing an inverted conversion would produce a picture that is
/// *wrong* rather than missing.
pub fn decode_jpeg(bytes: &[u8]) -> Result<Image, AssetError> {
    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
    let pixels = decoder
        .decode()
        .map_err(|error| AssetError::Codec(format!("not a JPEG: {error}")))?;
    let info = decoder
        .info()
        .ok_or_else(|| AssetError::Codec("the file has no frame header".to_string()))?;

    let (width, height) = (u32::from(info.width), u32::from(info.height));
    let count = width as usize * height as usize;
    let mut rgba = Vec::with_capacity(count * 4);

    match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 => {
            for &grey in pixels.iter().take(count) {
                rgba.extend_from_slice(&[grey, grey, grey, 255]);
            }
        }
        jpeg_decoder::PixelFormat::RGB24 => {
            for rgb in pixels.chunks_exact(3).take(count) {
                rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        other => {
            return Err(AssetError::Codec(format!(
                "a {other:?} JPEG: this build decodes greyscale and RGB, and a CMYK file needs a \
                 conversion it does not have"
            )));
        }
    }

    Ok(Image {
        width,
        height,
        rgba,
    })
}

/// Turns whatever the decoder produced into `RGBA8`.
///
/// `EXPAND` leaves greyscale without alpha as one component per pixel and greyscale with alpha
/// as two, so the two cases that are not already colour have to be widened here.
fn expand_to_rgba(pixels: &[u8], color: png::ColorType, width: u32, height: u32) -> Vec<u8> {
    let count = (width as usize * height as usize).min(pixels.len());
    let mut out = Vec::with_capacity(count * 4);
    match color {
        png::ColorType::Grayscale => {
            for &grey in pixels.iter().take(count) {
                out.extend_from_slice(&[grey, grey, grey, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for pair in pixels.chunks_exact(2).take(count) {
                out.extend_from_slice(&[pair[0], pair[0], pair[0], pair[1]]);
            }
        }
        png::ColorType::Rgb => {
            for rgb in pixels.chunks_exact(3).take(count) {
                out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        // Already four components, or a type the decoder said it would not produce.
        _ => out.extend_from_slice(&pixels[..pixels.len().min(count * 4)]),
    }
    out
}
