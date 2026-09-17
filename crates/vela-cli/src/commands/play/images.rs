//! The pictures a running player can draw (`SCREENS.md §3`).
//!
//! A picture is two facts that arrive at different moments: the build said how big it is, and the
//! platform says which texture it became once a window has uploaded it. Both are needed before a screen
//! can draw one, and neither source knows about the other — so the joining is a function here rather than
//! a few lines inside the frame loop, which is the difference between something a test can check and
//! something only a window can.
//!
//! A name the platform did not upload is *left out*. A texture id of zero standing in for "none" would
//! sample whatever texture was uploaded first, which is a wrong picture drawn confidently — worse than
//! drawing nothing.

use vela_ui::{ImageTable, Picture};

/// The pictures a screen may draw, from the sizes a build produced and the textures a platform uploaded.
pub(crate) fn table(
    sizes: &[(String, u32, u32)],
    texture_of: impl Fn(&str) -> Option<u32>,
) -> ImageTable {
    ImageTable::from_entries(sizes.iter().filter_map(|(name, width, height)| {
        Some((
            name.clone(),
            Picture {
                texture: texture_of(name)?,
                width: *width,
                height: *height,
            },
        ))
    }))
}
