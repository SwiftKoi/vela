//! Save version 2 → 3: the payload gains the suspension anchor.
//!
//! Every other step in this directory rewrites the *world*. This one rewrites nothing: what
//! changed between 2 and 3 is the shape of the file, not the state in it. A frame now records
//! the source range of the statement it is suspended at (`vela_vm::Resume`), which is what
//! lets a save resume after the body moved under it — a different optimization level, a
//! rebuilt compiler, either one.
//!
//! The step exists anyway, and that is the point. `save_version` is a claim about the *file*,
//! and a version-2 file is one this build accepts — so "accepted" is spelled as a step in the
//! chain rather than as a special case in the loader. A step with no operations says exactly
//! what is true here: nothing about the world needs bringing forward, and the file is still
//! old enough to say so.
//!
//! A save that predates the anchor keeps working: it has no statement to resume at, so the
//! loader falls back to the index it recorded — which is all a version-2 build had.

use crate::migration;

migration! {
    from = 2,
    to = 3,
}
