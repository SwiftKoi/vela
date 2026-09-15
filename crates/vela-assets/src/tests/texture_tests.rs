//! The texture importer: detection, normalization, and refusal.

use std::io::Cursor;

use crate::error::AssetError;
use crate::importers::{Importer, ImporterRegistry, texture::Texture};
use crate::{Digest, import_tree};

/// A PNG of `width` × `height` pixels, encoded at a compression level of its own.
///
/// The level is the point of the parameter: two encoders produce different bytes for the same
/// pixels, which is what the normalization has to collapse.
fn png(width: u32, height: u32, level: png::Compression) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&[(x * 16) as u8, (y * 16) as u8, 32, 255]);
        }
    }

    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(level);
        let mut writer = encoder.write_header().expect("write the header");
        writer.write_image_data(&pixels).expect("write the pixels");
    }
    bytes
}

/// A temporary directory of its own per test.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("vela-texture-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the scratch directory");
    dir
}

/// The importer claims PNG by its bytes, not by its name.
#[test]
fn a_png_is_recognized_by_its_magic_bytes() {
    let texture = Texture;
    assert!(texture.probe(&png(2, 2, png::Compression::Balanced)));
    assert!(!texture.probe(b"{ \"not\": \"a png\" }"));
    assert!(!texture.probe(b""));
}

/// Magic bytes win over the extension, with a real format rather than a stand-in.
///
/// A PNG named `.json` is imported as a texture, because that is what it *is*. The registry had
/// no importer with magic bytes until now, so this rule was only ever tested against a fake.
#[test]
fn a_png_named_json_is_imported_as_a_texture() {
    let registry = ImporterRegistry::builtin();
    let bytes = png(2, 2, png::Compression::Balanced);

    let outputs = registry.import("art/hero.json", &bytes).expect("imports");
    assert_eq!(outputs[0].kind, "texture");
    assert_eq!(outputs[0].path, "art/hero.png");
}

/// Two encodings of one image are one artifact.
///
/// This is what "normalized" has to mean for the digest to be an identity: everything the
/// encoder chose — compression strategy, filter, and any metadata a tool wrote — is not part of
/// what the image *is*.
#[test]
fn two_encodings_of_one_image_are_one_artifact() {
    let texture = Texture;
    let fast_bytes = png(8, 8, png::Compression::Fastest);
    let best_bytes = png(8, 8, png::Compression::High);
    assert_ne!(
        fast_bytes, best_bytes,
        "the two encodings are byte-identical, so this test would prove nothing"
    );

    let fast = texture
        .import(&crate::importers::ImportRequest {
            source: "art/hero.png",
            bytes: &fast_bytes,
        })
        .expect("imports");
    let best = texture
        .import(&crate::importers::ImportRequest {
            source: "art/hero.png",
            bytes: &best_bytes,
        })
        .expect("imports");

    assert_eq!(
        fast[0].bytes, best[0].bytes,
        "the same pixels became two artifacts"
    );
}

/// The artifact is the image, not a claim about it: decoding it gives back the pixels.
#[test]
fn the_artifact_decodes_to_the_same_pixels() {
    let texture = Texture;
    let original = png(4, 3, png::Compression::Balanced);
    let imported = texture
        .import(&crate::importers::ImportRequest {
            source: "art/hero.png",
            bytes: &original,
        })
        .expect("imports");

    let decoder = png::Decoder::new(Cursor::new(&imported[0].bytes));
    let mut reader = decoder.read_info().expect("the artifact is a PNG");
    let mut pixels = vec![0; reader.output_buffer_size().expect("sized")];
    let info = reader.next_frame(&mut pixels).expect("decodes");

    assert_eq!((info.width, info.height), (4, 3));
    assert_eq!(
        Digest::of(&pixels[..info.buffer_size()]),
        Digest::of(&decoded_pixels(&original)),
        "the artifact is not the image that went in"
    );
}

/// The pixels of an encoded PNG.
fn decoded_pixels(bytes: &[u8]) -> Vec<u8> {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("a PNG");
    let mut pixels = vec![0; reader.output_buffer_size().expect("sized")];
    let info = reader.next_frame(&mut pixels).expect("decodes");
    pixels.truncate(info.buffer_size());
    pixels
}

/// A file named `.png` that is not one is refused, with the file named.
#[test]
fn a_file_that_is_not_a_png_is_refused() {
    let dir = scratch("not-a-png");
    // PNG magic bytes and then nothing: enough to be *chosen* as a texture, not enough to be
    // one. This is the case the extension-only rule could not tell from a real image.
    std::fs::write(
        dir.join("broken.png"),
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\x00nothing here",
    )
    .expect("write the fixture");

    let error = import_tree(&dir, &ImporterRegistry::builtin()).expect_err("not a PNG");
    match error {
        AssetError::Import { ref path, kind, .. } => {
            assert_eq!(kind, "texture");
            assert!(path.ends_with("broken.png"), "{path:?}");
        }
        other => panic!("expected a texture failure, got {other:?}"),
    }
}

/// The extension is still the fallback when the bytes say nothing.
#[test]
fn the_extension_is_used_when_the_bytes_are_inconclusive() {
    let registry = ImporterRegistry::builtin();
    // Not PNG magic, but named as one: nothing claims it by magic, so the extension decides and
    // the importer gets to explain itself.
    let error = registry
        .import("art/hero.png", b"not a png at all")
        .expect_err("refused");
    assert!(matches!(error, AssetError::Import { .. }), "{error:?}");
}
