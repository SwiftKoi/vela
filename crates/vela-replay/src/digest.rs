//! A digest for the save schema, and a checksum for a save's bytes.
//!
//! **Not cryptographic, and deliberately so.** The digest answers "did the schema change
//! since this save was written", not "did someone tamper with this file" — a save is the
//! player's own data, on the player's own disk, and a hash that resisted forgery would buy
//! nothing. What it must be is *stable*: the same schema hashes the same on every machine,
//! which a `DefaultHasher` does not (it is seeded per process).
//!
//! FNV-1a with four different offset bases, concatenated into 256 bits. The width is what
//! makes an accidental collision vanishingly unlikely; the function is four multiplications
//! per byte.

/// The offset basis FNV-1a starts from, and a second constant to vary it per lane.
const BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;
const RESTART: u64 = 0x9e37_79b9_7f4a_7c15;

/// A 256-bit digest of `bytes`.
#[must_use]
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for lane in 0..4u64 {
        let mut hash = BASIS ^ lane.wrapping_mul(RESTART);
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
        out[(lane as usize) * 8..(lane as usize + 1) * 8].copy_from_slice(&hash.to_le_bytes());
    }
    out
}

/// A 64-bit checksum of `bytes`, for detecting a truncated or altered save.
#[must_use]
pub fn checksum(bytes: &[u8]) -> u64 {
    let mut hash = BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// The digest as lowercase hex, for a message.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}
