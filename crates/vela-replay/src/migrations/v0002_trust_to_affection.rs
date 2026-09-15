//! Save version 1 → 2: `trust` becomes `affection`, on a new scale.
//!
//! The worked example from `RUNTIME.md §6.1`, and the first step in the chain. It is what
//! proves the mechanism end to end: a save written at version 1 carries `trust`, and this
//! step renames it, seeds the default the new build declares, and rewrites the value.
//!
//! The scale doubled between the versions — what version 1 scored out of five, version 2
//! scores out of ten, and the cap moved with it — so the transform is a rescale rather than a
//! clamp, and the value has to be doubled to keep its meaning.
//!
//! The operations run in the order written, so `rename_field` sees `trust` and the transform
//! sees `affection` — the whole reason a step is a list rather than a single closure.

use vela_world::{Value, World};

use crate::migration;

migration! {
    from = 1,
    to = 2,
    rename_field = ("trust", "affection"),
    add_default = ("affection_cap", Value::Int(10)),
    transform = to_the_new_scale,
}

/// Rescales the renamed value onto the range the new build expects.
///
/// A `fn` rather than a closure, which is what the macro's `transform` accepts: a migration
/// must not depend on anything but the world it is handed, or a rebuild of the same save
/// could migrate differently.
fn to_the_new_scale(world: &mut World) {
    let scaled = match world.get("affection") {
        Some(Value::Int(value)) => Value::Int(value * 2),
        _ => return,
    };
    world.set("affection", scaled);
}
