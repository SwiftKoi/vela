//! Memoized query results.

use vela_span::FileId;

/// What a cached value was computed from: each input file, with its revision.
///
/// A revision, not a hash. Revisions only ever increase, so comparing them is exact and
/// costs an integer compare, whereas a hash could collide and silently serve a stale
/// result — the worst possible failure for an incremental compiler, because it looks
/// like nothing is wrong.
pub type Deps = Vec<(FileId, u64)>;

/// A memoized value together with the inputs it depends on.
pub(crate) struct Cached<T> {
    /// The input revisions this value was computed from.
    pub(crate) deps: Deps,
    /// The value.
    pub(crate) value: T,
}
