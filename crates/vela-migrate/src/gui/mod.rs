//! `gui.rpy` → a Vela theme and its styles (`SCREENS.md §5`, `§2.6`).
//!
//! Ren'Py's GUI is a flat bag of `define gui.<name> = <value>` lines — 126 of them in the sample —
//! and what `screens.rpy` does with them is narrower than the bag suggests: it reads them in
//! exactly two shapes. `gui.<name>` directly, and `properties gui.text_properties("name")`, which
//! means *"every `gui.name_*` is a property of the `name` style"*. That second shape is what makes
//! a translation possible at all, because the naming convention **is** the style: `gui.name_xpos`
//! is the `name` style's `xpos`, and `gui.button_text_hover_color` is `button`'s `hover_color` key
//! — Ren'Py puts the interaction state in the *identifier* where `SCREENS.md §5.1` puts it in the
//! *key*.
//!
//! # What is translated, and what is not
//!
//! A **colour** is a theme token, because that is what a theme is for. A **font** is a theme font
//! token, named after the style that uses it. A live `<group>_<state>_<prop>` is a `style` line —
//! but only when `<prop>` is one of the properties a Vela style **paints** (`color`, `background`,
//! `size`, `font`). That set was measured rather than assumed: a style setting `size = 40` draws
//! bigger text, while a style setting `xpos = 400` or `xalign = 1.0` is accepted and ignored,
//! because placement lives on the widget in Vela and not on the style. Emitting the GUI's
//! placement variables would produce a file that looks migrated and draws in the wrong place, so
//! they are *reported*, grouped by style, with the reason — which is the point of this pass rather
//! than its failure mode.
//!
//! # Files
//!
//! [`values`] parses the file; [`names`] decides what each variable means, which is where the
//! naming convention lives; [`theme`] builds the output; [`report`] writes down what was left out;
//! and [`pass`] is the entry point that runs the four in order.

mod names;
mod pass;
mod report;
mod theme;
mod values;

pub use pass::{Names, skin};

// The rules a second pass shares: it honours the same paint-versus-place boundary, and it reads
// the theme's tokens (`screens.rs`).
pub(crate) use names::{PAINTED, strip_state};
