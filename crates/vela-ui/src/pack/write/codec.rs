//! The byte sink, and the header and sections that go into it.

use vela_span::Span;
#[cfg(test)]
use vela_syntax::Expr;

use crate::pack::model::{MAGIC, PACK_VERSION, ScreenPack};

/// Encodes a pack: header, five sections, checksum.
pub(crate) fn encode(pack: &ScreenPack) -> Vec<u8> {
    let mut writer = Writer::default();
    writer.bytes.extend_from_slice(&MAGIC);
    writer.u16(PACK_VERSION);
    // Reserved, and written zero so a later version can put a bit here and an older reader can be
    // told to skip something rather than misread it.
    writer.u32(0);

    writer.section(|section| section.string(&pack.module));
    writer.section(|section| section.screens(&pack.set.screens));
    writer.section(|section| section.styles(&pack.set.styles));
    writer.section(|section| section.palette(&pack.set.palette));
    writer.section(|section| section.fonts(&pack.set.fonts));

    let checksum = fnv1a(&writer.bytes);
    writer.u64(checksum);
    writer.bytes
}

/// Encodes one expression on its own, for the codec's tests.
///
/// A bare expression is never written by a pack — it always sits inside a screen — so this exists
/// only to check every variant's tag and fields against the reader without needing a source file
/// that happens to contain all of them.
#[cfg(test)]
pub(crate) fn encode_expr(expr: &Expr) -> Vec<u8> {
    let mut writer = Writer::default();
    writer.expr(expr);
    writer.bytes
}

/// FNV-1a, as `.velac` uses for the same job: it catches a truncated transfer or a half-written
/// file, which are the two ways a pack actually arrives broken. It is not a signature.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A length, saturated: a section larger than 4 GiB is not something this format can express, and
/// a build that met one would rather write a wrong length than panic.
pub(super) fn count(length: usize) -> u32 {
    u32::try_from(length).unwrap_or(u32::MAX)
}

/// A little-endian byte sink.
#[derive(Default)]
pub(super) struct Writer {
    pub(super) bytes: Vec<u8>,
}

impl Writer {
    pub(super) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub(super) fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(super) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(super) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(super) fn i64(&mut self, value: i64) {
        self.u64(value as u64);
    }

    pub(super) fn f64(&mut self, value: f64) {
        self.u64(value.to_bits());
    }

    pub(super) fn flag(&mut self, set: bool) {
        self.u8(u8::from(set));
    }

    pub(super) fn string(&mut self, text: &str) {
        self.u32(count(text.len()));
        self.bytes.extend_from_slice(text.as_bytes());
    }

    pub(super) fn optional_string(&mut self, text: Option<&str>) {
        match text {
            Some(text) => {
                self.flag(true);
                self.string(text);
            }
            None => self.flag(false),
        }
    }

    pub(super) fn span(&mut self, span: Span) {
        self.u32(span.file().as_raw());
        self.u32(span.start());
        self.u32(span.end());
    }

    /// Writes one length-prefixed section, so a reader can skip one it does not understand.
    pub(super) fn section(&mut self, write: impl FnOnce(&mut Self)) {
        let mut inner = Self::default();
        write(&mut inner);
        self.u32(count(inner.bytes.len()));
        self.bytes.extend_from_slice(&inner.bytes);
    }
}
