//! The screens a story has open, bottom first, and what can be asked of the top one.
//!
//! The runtime's own state, and it lives here rather than in the player because *two* callers have a
//! screen stack now: `vela run` shows one in a window, and a `vela test` will drive one without a
//! window — a test that clicks a control is a player, minus the window (`TOOLING.md §5`). What the
//! two share is everything below: which screen is on top, where focus is, what a control *says*, and
//! what activating it asks for. What they do not share is what happens after an action — a window
//! redraws, a test keeps stepping the story — which is why an action is returned rather than run.
//!
//! [`ScreenSource`] is the seam: this crate holds a laid screen, and the layer that can compile a
//! project's `.vela` files produces one.
//!
//! What an action a screen asks for *does* is `dispatch`'s, beside this rather than in either
//! caller, because both callers need the same answer (`SCREENS.md §7`).

mod dispatch;

pub use dispatch::Done;

use vela_render::{Color, DrawList, RectQuad};
use vela_text::TextEngine;
use vela_world::Preferences;

use crate::actions::Action as ScreenAction;
use crate::screens::Laid;
use crate::{Args, ImageTable, Rect, ScreenSet, ScreenState, Value};

/// Where a stack gets its screens: the compiled screens of a project, by name.
///
/// A trait rather than a `ScreenSet` because a project's screens are *several* sets — one per source
/// file, since a screen resolves within the file that declares it (`SCREENS.md §5`) — and the layer
/// that knows which is which is the caller's.
pub trait ScreenSource {
    /// Evaluates and lays out a declared screen, or `None` when it is not declared.
    fn lay(
        &self,
        name: &str,
        args: &Args,
        state: &ScreenState,
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
    ) -> Option<Laid>;
}

/// One file's screens, which is what a small project has.
impl ScreenSource for ScreenSet {
    fn lay(
        &self,
        name: &str,
        args: &Args,
        state: &ScreenState,
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
    ) -> Option<Laid> {
        ScreenSet::lay(self, name, args, state, size, text, font)
    }
}

/// A project's screens are one set per file, so the search is across them.
impl ScreenSource for [ScreenSet] {
    fn lay(
        &self,
        name: &str,
        args: &Args,
        state: &ScreenState,
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
    ) -> Option<Laid> {
        self.iter()
            .find(|set| set.has(name))?
            .lay(name, args, state, size, text, font)
    }
}

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
    /// What the player has chosen, so a screen over the story can read it (`RUNTIME.md §2.1`).
    ///
    /// Held here rather than reached for, because a screen's scope is built by the layer that lays it out:
    /// `setting("text_speed")` has to answer the same value in an arm, a loop body and a `use` argument,
    /// which is what makes the store part of the scope rather than a lookup (`SCREENS.md §2.6`).
    preferences: Preferences,
}

impl Stack {
    /// Sets the player's settings every screen this stack lays out is answered with.
    ///
    /// A caller that changes one re-lays the stack (`relaid`), which is how a settings screen draws what
    /// it just set.
    pub fn set_preferences(&mut self, preferences: Preferences) {
        self.preferences = preferences;
    }

    /// The scope a screen of this stack is laid out in: no arguments, and what the host says.
    fn args(&self) -> Args {
        Args::new().with_preferences(self.preferences.clone())
    }

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
        screens: &(impl ScreenSource + ?Sized),
        name: &str,
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
    ) -> bool {
        let Some(laid) = screens.lay(name, &self.args(), &ScreenState::new(), size, text, font)
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
        screens: &(impl ScreenSource + ?Sized),
        name: &str,
        value: Value,
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
    ) -> bool {
        // The scope first: it borrows `self`, and the overlay below is borrowed mutably.
        let args = self.args();
        let Some(top) = self.overlays.last_mut() else {
            return false;
        };
        let mut state = top.laid.state.clone();
        state.set(name, value);
        // Through the same call that opened it, so a write goes through exactly the path an argument
        // does and a screen cannot draw differently depending on why it was laid out.
        let Some(laid) = screens.lay(&top.name, &args, &state, size, text, font) else {
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

    /// Moves focus to the top screen's control whose words contain `text`.
    ///
    /// Containment and casefolding, which is Ren'Py's own rule for a text selector
    /// (`testfocus.find_focus`: *a pattern in the text*, both folded) — and the reason a test may say
    /// `click "history"` for a button that reads `History`. Among the controls that match, the
    /// *shortest* label wins: that is the one whose words are about the match rather than a container
    /// that happens to hold a similar phrase.
    ///
    /// Focus rather than the action, so a caller that clicks does what a player does — the control is
    /// selected, and the action is read from [`Stack::focused`]: one path to an action rather than
    /// two, and a focus ring that lands where the click did.
    pub fn focus_label(&mut self, text: &str) -> bool {
        let wanted = text.to_lowercase();
        let Some(top) = self.overlays.last_mut() else {
            return false;
        };
        let found = top
            .laid
            .hotspots
            .iter()
            .enumerate()
            .filter(|(_, hotspot)| hotspot.label.to_lowercase().contains(&wanted))
            .min_by_key(|(_, hotspot)| hotspot.label.len())
            .map(|(index, _)| index);
        match found {
            Some(index) => {
                top.focus = index;
                true
            }
            None => false,
        }
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
        images: &ImageTable,
    ) {
        let top = self.overlays.len().saturating_sub(1);
        for (index, overlay) in self.overlays.iter().enumerate() {
            let focused = (index == top).then_some(overlay.focus);
            crate::paint(
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
        screens: &(impl ScreenSource + ?Sized),
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
    ) {
        // Built before the loop: the scope borrows `self` while an overlay is laid out mutably, and a scope
        // per overlay would be the same one built four times.
        let args = self.args();
        for overlay in &mut self.overlays {
            // The screen's own variables come across: a reload is an edit to the *source*, and a
            // variable the player has already changed is state rather than source (`SCREENS.md §2.5`).
            if let Some(laid) =
                screens.lay(&overlay.name, &args, &overlay.laid.state, size, text, font)
            {
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
