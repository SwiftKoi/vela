//! How a screen becomes something to look at: the widget tree, a frame for it, and paint.
//!
//! Split from `screens.rs` when `lay_opened` pushed the `impl` block past its budget, and it is the cut
//! the file was describing anyway: everything here *produces* something — a tree (`build`), a laid-out
//! screen (`lay`, `lay_opened`), pixels (`draw`) — while the rest of `screens.rs` is the set and what it
//! was told about the world it draws in.

use vela_render::DrawList;
use vela_text::TextEngine;

use crate::eval::{Args, ScreenState};
use crate::focus;
use crate::instantiate;
use crate::layout::{Constraints, layout};
use crate::paint;
use crate::stack::Call;
use crate::tree::{Node, Size};

use super::{Laid, ScreenSet};

impl ScreenSet {
    /// Evaluates a screen to a widget tree, or `None` if it is not declared.
    #[must_use]
    pub fn build(
        &self,
        name: &str,
        args: &Args,
        state: &mut ScreenState,
        text: &mut TextEngine,
        font: &str,
        max_width: f32,
    ) -> Option<Node> {
        let screen = self.screen(name)?;
        Some(self.with_ctx(|ctx| {
            instantiate::build(&screen.body, ctx, args, state, text, font, max_width)
        }))
    }

    /// Evaluates and lays out a screen, collecting where it goes and what can be activated.
    ///
    /// A caller that only wants a picture can `paint` the result; a caller that also wants to
    /// *drive* the screen — a runtime with a focus cursor — reads `hotspots`. Doing both from
    /// one walk is what keeps the drawn button and the focusable button the same button.
    #[must_use]
    pub fn lay(
        &self,
        name: &str,
        args: &Args,
        state: &ScreenState,
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
    ) -> Option<Laid> {
        let (width, height) = (size.0 as f32, size.1 as f32);
        let screen = self.screen(name)?;
        // The variants this frame adds to what the bundle declared (`§2.6`): a screen laid out in a
        // frame with less room than the game was designed for is `small`, and the frame is the only
        // thing that knows.
        let args = args
            .clone()
            .with_variants(self.variants.for_frame(width, height, self.design));
        // The screen's variables start from what the caller kept: a write survives a layout, and a
        // screen that has never been laid out initializes them from its own `default`s.
        let mut state = state.clone();
        // One context, two questions: the tree, and the input bindings that are not part of it.
        let (node, keys, timers) = self.with_ctx(|ctx| {
            let node = instantiate::build(&screen.body, ctx, &args, &mut state, text, font, width);
            let (keys, timers) = instantiate::bindings(&screen.body, ctx, &args);
            (node, keys, timers)
        });
        let frame = layout(&node, Constraints::exact(Size::new(width, height)));
        let hotspots = focus::hotspots(&node, &frame);
        Some(Laid {
            node,
            frame,
            hotspots,
            keys,
            timers,
            state,
        })
    }

    /// Evaluates and lays out a screen a *runtime* opened by name, with a call's values bound
    /// (`SCREENS.md §2.1`).
    ///
    /// The second way a screen is called, and not a second calling convention: the call's values bind in
    /// the opened screen's parameter order — what a runtime can know, since an action carries values
    /// and no argument names — and a parameter the call leaves out keeps its default, which is
    /// [`crate::compose::with_defaults`], the rule `use` binds by.
    ///
    /// The scope is what the host says rather than what the call passed: a screen opened over the story
    /// reads `variant(...)` and `setting(...)` exactly as the dialogue under it does
    /// (`SCREENS.md §2.6`). A value the call passed wins over a name the scope happens to hold.
    #[must_use]
    pub fn lay_opened(
        &self,
        name: &str,
        call: &Call<'_>,
        state: &ScreenState,
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
    ) -> Option<Laid> {
        let screen = self.screen(name)?;
        let mut args = call.scope.clone();
        for (value, param) in call.values.iter().zip(&screen.params) {
            // Replaced rather than added: `Args` answers with the *first* binding, so a call's value
            // has to take the name over rather than sit behind it.
            args = args.with(&param.name, value.clone());
        }
        let args = crate::compose::with_defaults(screen, args);
        self.lay(name, &args, state, size, text, font)
    }

    /// Lays out and paints a screen into `draw`, returning whether it was found.
    ///
    /// No focus cursor: this draws a screen as it stands, which is what a still frame wants. A caller
    /// with a cursor — a runtime the player is navigating — paints a [`Laid`] through
    /// [`paint::paint`], which takes the focused hotspot's index so the node under it draws in its
    /// `selected` state (`SCREENS.md §5`).
    pub fn draw(
        &self,
        name: &str,
        args: &Args,
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
        draw: &mut DrawList,
    ) -> bool {
        let Some(laid) = self.lay(name, args, &ScreenState::new(), size, text, font) else {
            return false;
        };
        paint::paint(
            &laid.node,
            &laid.frame,
            text,
            font,
            draw,
            None,
            &self.images,
        );
        true
    }
}
