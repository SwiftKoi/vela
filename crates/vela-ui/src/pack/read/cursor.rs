//! The header, the sections, and the bounds-checked cursor everything reads through.

use vela_span::{FileId, Span};

use crate::error::PackError;
use crate::pack::model::{MAGIC, PACK_VERSION, PackedSet, ScreenPack};

/// How many bytes the header takes: magic, version, flags.
const HEADER: usize = 4 + 2 + 4;

/// How deeply expressions may nest before the pack is refused.
///
/// A pack comes from a build, but it is read as though it came off a network: a crafted one could
/// nest a million lambdas and take the stack down with it. A screen's props and conditions are
/// shallow, so this is far past anything a real one reaches.
const MAX_DEPTH: usize = 64;

/// Decodes a pack.
///
/// # Errors
///
/// Fails on a bad magic number, a truncated container, a checksum that does not match, a version
/// this build does not read, or a section that does not describe a screen.
pub(crate) fn decode(bytes: &[u8]) -> Result<ScreenPack, PackError> {
    let version = header(bytes)?;

    // The checksum covers everything before it, which is what catches a truncated transfer.
    let covered = bytes.len().saturating_sub(8);
    let stored = u64::from_le_bytes(
        bytes[covered..]
            .try_into()
            .map_err(|_| truncated("the checksum"))?,
    );
    let computed = fnv1a(&bytes[..covered]);
    if stored != computed {
        return Err(PackError::Malformed(format!(
            "the checksum is {stored:#018x}, expected {computed:#018x}"
        )));
    }

    let mut reader = Reader::new(&bytes[HEADER..covered]);
    let module = reader.section(Reader::string)?;
    let screens = reader.section(Reader::screens)?;
    let styles = reader.section(Reader::styles)?;
    let palette = reader.section(Reader::palette)?;
    let fonts = reader.section(Reader::fonts)?;
    if reader.failed() {
        return Err(reader.refusal());
    }

    Ok(ScreenPack {
        pack_version: version,
        module,
        set: PackedSet {
            screens,
            styles,
            palette,
            fonts,
        },
    })
}

/// Decodes one expression on its own, for the codec's tests: the counterpart of the writer's
/// `encode_expr`, and the only caller of the expression reader outside a section.
#[cfg(test)]
pub(crate) fn decode_expr(bytes: &[u8]) -> Option<vela_syntax::Expr> {
    let mut reader = Reader::new(bytes);
    let value = reader.expr();
    (!reader.failed()).then_some(value)
}

/// The header's version, or why the bytes are not a pack.
fn header(bytes: &[u8]) -> Result<u16, PackError> {
    if bytes.len() < HEADER + 8 {
        return Err(truncated("the header"));
    }
    if bytes[..4] != MAGIC {
        return Err(PackError::Malformed(
            "bad magic: this is not a screen pack".to_string(),
        ));
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version > PACK_VERSION {
        return Err(PackError::Version(u32::from(version)));
    }
    // Bytes 6..10 are flags, reserved and unread by this version: a later one may put a bit there
    // and expect this reader to skip something rather than misread it.
    Ok(version)
}

/// Why the bytes stopped describing a pack.
fn truncated(what: &str) -> PackError {
    PackError::Malformed(format!("the container ended before {what}"))
}

/// FNV-1a, matching the writer's.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A bounds-checked cursor.
///
/// Reads past the end return zero and set an error rather than panicking, and every caller checks
/// once at the end — the established shape from `vela-bytecode`'s codec, and just as total: a pack
/// is either built or refused, and a half-read one is never returned.
pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    error: Option<String>,
    depth: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            at: 0,
            error: None,
            depth: 0,
        }
    }

    fn failed(&self) -> bool {
        self.error.is_some()
    }

    /// The reason a reader gave up, for the caller.
    fn refusal(&self) -> PackError {
        PackError::Malformed(
            self.error
                .clone()
                .unwrap_or_else(|| "the pack is not readable".to_string()),
        )
    }

    pub(super) fn fail(&mut self, reason: &str) {
        if self.error.is_none() {
            self.error = Some(reason.to_string());
        }
    }

    /// Takes `width` bytes, or fails.
    fn take(&mut self, width: usize) -> &'a [u8] {
        let end = self.at.saturating_add(width);
        if end > self.bytes.len() {
            self.fail("the container ended mid-value");
            return &[];
        }
        let slice = &self.bytes[self.at..end];
        self.at = end;
        slice
    }

    pub(super) fn u8(&mut self) -> u8 {
        self.take(1).first().copied().unwrap_or(0)
    }

    pub(super) fn u32(&mut self) -> u32 {
        let bytes = self.take(4);
        if bytes.len() < 4 {
            return 0;
        }
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    pub(super) fn u64(&mut self) -> u64 {
        let bytes = self.take(8);
        if bytes.len() < 8 {
            return 0;
        }
        let mut buffer = [0u8; 8];
        buffer.copy_from_slice(bytes);
        u64::from_le_bytes(buffer)
    }

    pub(super) fn i64(&mut self) -> i64 {
        self.u64() as i64
    }

    pub(super) fn f64(&mut self) -> f64 {
        f64::from_bits(self.u64())
    }

    pub(super) fn flag(&mut self) -> bool {
        self.u8() != 0
    }

    /// A length, refused if it could not possibly fit in what is left.
    ///
    /// The one place a corrupt pack can ask for an enormous allocation, so it is checked against
    /// what remains rather than trusted.
    pub(super) fn count(&mut self) -> usize {
        let count = self.u32() as usize;
        if count > self.bytes.len() {
            self.fail("a length is larger than the container");
            return 0;
        }
        count
    }

    pub(super) fn string(&mut self) -> String {
        let length = self.count();
        String::from_utf8_lossy(self.take(length)).into_owned()
    }

    pub(super) fn optional_string(&mut self) -> Option<String> {
        self.flag().then(|| self.string())
    }

    pub(super) fn span(&mut self) -> Span {
        let file = self.u32();
        let start = self.u32();
        let end = self.u32();
        Span::new(FileId::from_raw(file), start, end)
    }

    /// Reads a length-prefixed section.
    fn section<T>(&mut self, read: impl FnOnce(&mut Self) -> T) -> Result<T, PackError> {
        let length = self.u32() as usize;
        let end = self.at.saturating_add(length);
        if end > self.bytes.len() {
            return Err(truncated("a section"));
        }

        let mut inner = Self {
            bytes: &self.bytes[self.at..end],
            at: 0,
            error: None,
            depth: 0,
        };
        let value = read(&mut inner);
        if let Some(error) = inner.error {
            return Err(PackError::Malformed(error));
        }
        self.at = end;
        Ok(value)
    }

    /// Enters one expression, or refuses to go deeper.
    pub(super) fn descend(&mut self) -> bool {
        if self.depth >= MAX_DEPTH {
            self.fail("the pack nests expressions more deeply than a screen can");
            return false;
        }
        self.depth += 1;
        true
    }

    pub(super) fn ascend(&mut self) {
        self.depth -= 1;
    }
}
