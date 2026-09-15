//! The digest: that it is SHA-256, that it is stable, and that it refuses what it cannot read.

use crate::digest::Digest;

/// The published vectors. Checking the algorithm against FIPS 180-4's own answers is what makes
/// "the digest is sha256" a fact rather than a claim about a dependency's name — and a
/// hand-written test would not catch a wrong padding rule that happened to round-trip.
#[test]
fn the_digest_is_sha256() {
    assert_eq!(
        Digest::of(b"").as_str(),
        "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        Digest::of(b"abc").as_str(),
        "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    // The 448-bit boundary, where padding takes a second block.
    assert_eq!(
        Digest::of(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq").as_str(),
        "sha256:248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

/// The same bytes digest the same, which is the property content addressing rests on.
#[test]
fn the_digest_is_stable() {
    assert_eq!(Digest::of(b"forest"), Digest::of(b"forest"));
    assert_ne!(Digest::of(b"forest"), Digest::of(b"forests"));
}

/// A digest read back is the digest that was written.
#[test]
fn a_digest_round_trips_through_text() {
    let digest = Digest::of(b"abc");
    assert_eq!(Digest::parse(digest.as_str()).expect("parses"), digest);
}

/// Uppercase hex is accepted and normalized, because a manifest may have been hand-edited.
#[test]
fn uppercase_hex_is_normalized() {
    let text = "sha256:BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";
    assert_eq!(
        Digest::parse(text).expect("parses").as_str(),
        "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

/// Anything that is not `sha256:` and 64 hex digits is refused rather than compared.
#[test]
fn a_digest_that_is_not_one_is_refused() {
    for text in [
        "",
        "sha256:",
        "sha256:abc",
        "md5:ba7816bf8f01cfea414140de5dae2223",
        "sha256:zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    ] {
        assert!(Digest::parse(text).is_err(), "`{text}` was accepted");
    }
}
