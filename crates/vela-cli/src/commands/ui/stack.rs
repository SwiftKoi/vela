//! The screens a story has open, bottom first, and what can be asked of the top one.

use vela_render::{Color, DrawList, RectQuad};
use vela_text::TextEngine;
use vela_ui::actions::Action as ScreenAction;
use vela_ui::screens::Laid;
use vela_ui::{Args, Rect, ScreenState, Value};

use super::screens::Screens;

/// One screen the story opened, laid out and ready to navigate.
pub struct Overlay {
    /// The screen's declared name.
    pub name: String,
    /// Its evaluated, laid-out body.
    pub laid: Laid,
    /// Which hotspot has focus.
    pub focus: usize,
}

/// The screens drawn above the story, bottom first.
///
/// A stack rather than a set because screens layer: a pause menu over a dialogue, a settings
/// panel over the pause menu. `close_screen` pops the top, which is why opening a screen with
/// nothing to close it would be a trap — the runtime's `Cancel` closes the top as well.
#[derive(Default)]
pub struct Stack {
    overlays: Vec<Overlay>,
}

impl Stack {
    /// Whether nothing is open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.overlays.is_empty()
    }

    /// Opens a screen, laying it out without arguments.
    ///
    /// Returns `false` if no such screen is declared, so a caller can say so rather than
    /// presenting an empty frame. Screen *arguments* are not passed yet: an `open_screen` call
    /// names a screen, and a screen with required parameters has nothing to bind them to.
    pub fn open(
        &mut self,
        screens: &Screens,
        name: &str,
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
    ) -> bool {
        let Some(laid) = screens.lay(name, &Args::new(), &ScreenState::new(), size, text, font)
        else {
            return false;
        };
        self.overlays.push(Overlay {
            name: name.to_string(),
            laid,
            focus: 0,
        });
        true
    }

    /// Closes the topmost screen, returning its name.
    pub fn close(&mut self) -> Option<String> {
        self.overlays.pop().map(|overlay| overlay.name)
    }

    /// Closes a named screen, wherever it sits in the stack.
    ///
    /// What `hide(name)` needs and `close_screen` cannot give: a HUD screen asked to hide itself is
    /// often not the topmost one, and closing "the top" would dismiss whatever is above it instead.
    pub fn close_named(&mut self, name: &str) -> bool {
        let Some(index) = self
            .overlays
            .iter()
            .rposition(|overlay| overlay.name == name)
        else {
            return false;
        };
        self.overlays.remove(index);
        true
    }

    /// Writes one of the top screen's own variables, and lays it out again (`SCREENS.md §2.5`).
    ///
    /// Only the topmost screen, for the reason only the topmost screen is navigable: a write is a
    /// button being pressed, and a button under another is behind it — so the press that reaches a
    /// variable is a press on the screen the player can see, and one screen's tab cannot change
    /// another's.
    ///
    /// Returns `false` when nothing is open or the screen is no longer declared, so a caller can say so
    /// rather than leave a press that did nothing unexplained.
    pub fn set_variable(
        &mut self,
        screens: &Screens,
        name: &str,
        value: Value,
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
    ) -> bool {
        let Some(top) = self.overlays.last_mut() else {
            return false;
        };
        let mut state = top.laid.state.clone();
        state.set(name, value);
        // Through the same call that opened it, so a write goes through exactly the path an argument
        // does and a screen cannot draw differently depending on why it was laid out.
        let Some(laid) = screens.lay(&top.name, &Args::new(), &state, size, text, font) else {
            return false;
        };
        // The hotspot list is what focus indexes into, and a write can change how long it is — a tab
        // that reveals two buttons where there was one. Clamped rather than reset, so the player's
        // place survives a write that changed nothing about the shape.
        top.focus = top.focus.min(laid.hotspots.len().saturating_sub(1));
        top.laid = laid;
        true
    }

    /// Moves focus within the topmost screen, wrapping at both ends.
    pub fn move_focus(&mut self, delta: isize) -> bool {
        let Some(top) = self.overlays.last_mut() else {
            return false;
        };
        let count = top.laid.hotspots.len();
        if count == 0 {
            return false;
        }
        top.focus = (top.focus as isize + delta).rem_euclid(count as isize) as usize;
        true
    }

    /// The screen on top, if any — the one the player is looking at and answering.
    ///
    /// What focus, the key bindings, the focus ring, and a write to a variable all mean by "the
    /// screen", said once rather than four times.
    #[must_use]
    pub fn top(&self) -> Option<&Overlay> {
        self.overlays.last()
    }

    /// The action the focused hotspot asks for, if the top screen has one.
    #[must_use]
    pub fn focused(&self) -> Option<&ScreenAction> {
        let top = self.top()?;
        top.laid
            .hotspots
            .get(top.focus)
            .map(|hotspot| &hotspot.action)
    }

    /// The action the top screen answers `name` with, if it binds one.
    ///
    /// Only the top screen, for the same reason only the top screen is navigable: a binding under
    /// another screen is behind it, and input goes to what the player can see (`SCREENS.md §2.3`).
    #[must_use]
    pub fn key_action(&self, name: &str) -> Option<&ScreenAction> {
        self.top()?.laid.key(name)
    }

    /// Paints every open screen, bottom first, then a highlight over the focused hotspot.
    ///
    /// The highlight is the runtime's, not the screen's: focus has to be *visible* for the
    /// screen to be usable without sight of a pointer, and a screen that drew its own focus
    /// ring would be a screen that can forget to.
    ///
    /// Only the topmost screen is given the focus cursor, because only the topmost one can be
    /// navigated — a screen underneath draws as it stands, and its `selected` values stay unseen
    /// until it is the one on top (`SCREENS.md §5`).
    pub fn paint(
        &self,
        text: &mut TextEngine,
        font: &str,
        draw: &mut DrawList,
        images: &vela_ui::ImageTable,
    ) {
        let top = self.overlays.len().saturating_sub(1);
        for (index, overlay) in self.overlays.iter().enumerate() {
            let focused = (index == top).then_some(overlay.focus);
            vela_ui::paint(
                &overlay.laid.node,
                &overlay.laid.frame,
                text,
                font,
                draw,
                focused,
                images,
            );
        }
        if let Some(rect) = self.focus_rect() {
            draw.push_rect(RectQuad {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                color: focus_colour(),
            });
        }
    }

    /// Rebuilds every open screen from a new compile, keeping focus where it still exists.
    ///
    /// A screen that vanished from the project is left as it was: a blank window is a worse
    /// answer to "you deleted this screen's declaration" than the screen still being there.
    pub fn relaid(
        &mut self,
        screens: &Screens,
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
    ) {
        for overlay in &mut self.overlays {
            // The screen's own variables come across: a reload is an edit to the *source*, and a
            // variable the player has already changed is state rather than source (`SCREENS.md §2.5`).
            if let Some(laid) = screens.lay(
                &overlay.name,
                &Args::new(),
                &overlay.laid.state,
                size,
                text,
                font,
            ) {
                overlay.focus = overlay.focus.min(laid.hotspots.len().saturating_sub(1));
                overlay.laid = laid;
            }
        }
    }

    /// The rectangle of the focused hotspot, if any.
    #[must_use]
    pub fn focus_rect(&self) -> Option<Rect> {
        let top = self.top()?;
        top.laid.hotspots.get(top.focus).map(|hotspot| hotspot.rect)
    }
}

/// The focus highlight: a faint white wash, so the focused control reads through it.
///
/// The runtime has no palette of its own — each screen resolves its own theme — so this is a
/// fixed colour rather than the project's. A project theming focus is a prop on the runtime,
/// not a value a screen can set, and there is no such prop yet.
fn focus_colour() -> Color {
    Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 0.22,
    }
}
