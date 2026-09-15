//! Content hashing: an artifact's identity.
//!
//! `BUILD_AND_ASSETS.md §2`: *"`id` is a human name; the `digest` is identity. Two identical
//! inputs produce one artifact."* That sentence is the whole reason this is a cryptographic
//! hash rather than the schema fingerprint `vela-replay` uses: identity is a promise that two
//! *different* inputs never collide, and "and here is the file you already have" is a promise
//! a delta patch makes to a player. A 64-bit hash is plenty to notice a changed schema; it is
//! not enough to be an identity.
//!
//! SHA-256, rendered `sha256:<hex>` — the form the manifest shows and the form a patch verifies
//! a downloaded chunk against.

use std::fmt;

use sha2::{Digest as _, Sha256};

/// The part of a digest's rendering that says which hash it is.
///
/// Carried in the text so a manifest written by this build can be read by one that hashes
/// differently, and refused rather than compared across algorithms.
pub const ALGORITHM: &str = "sha256";

/// A content hash, as `sha256:<64 hex digits>`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Digest(String);

impl Digest {
    /// The digest of some bytes.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self(format!("{ALGORITHM}:{}", hex(&hasher.finalize())))
    }

    /// Reads a digest back from a manifest.
    ///
    /// # Errors
    ///
    /// Fails if the text is not `sha256:` followed by 64 hex digits. A manifest that says
    /// something else is corrupt, and comparing it as though it were a digest would turn that
    /// into a mystery two steps later.
    pub fn parse(text: &str) -> Result<Self, String> {
        let Some(rest) = text
            .strip_prefix(ALGORITHM)
            .and_then(|r| r.strip_prefix(':'))
        else {
            return Err(format!("`{text}` is not a `{ALGORITHM}:` digest"));
        };
        if rest.len() != 64 || !rest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!(
                "`{text}` is not 64 hex digits after `{ALGORITHM}:`"
            ));
        }
        Ok(Self(format!("{ALGORITHM}:{}", rest.to_ascii_lowercase())))
    }

    /// The digest as text, `sha256:<hex>`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl serde::Serialize for Digest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for Digest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// Lowercase hex, two digits per byte.
fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}
