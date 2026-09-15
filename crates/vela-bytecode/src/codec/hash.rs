//! FNV-1a: the one hash this crate uses.
//!
//! Not cryptographic. The checksum catches a truncated transfer and the schema digest
//! catches a rebuild; neither is defending against anyone.

/// Hashes bytes.
#[must_use]
pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
