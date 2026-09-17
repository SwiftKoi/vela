//! A project's screens, compiled into something that can be laid out and painted.
//!
//! This is the unit the presenter consumes. A screen is not a value the VM yields — nothing in
//! the command vocabulary names one yet — so the runtime reaches a screen through this: it
//! holds the declared screens, the styles they resolve against, and the theme colours those
//! styles name, and it turns a name plus arguments into pixels.
//!
//! Styles are resolved **per file**, which mirrors the checker's own deliberate limit
//! (`check.rs` records why): a style table is read where it is declared, so a screen is
//! compiled against the styles in its own module and a cross-module reference is not yet seen.
//! A [`ScreenSet`] is therefore one file's worth, and a caller with several files keeps several.

use vela_render::DrawList;
use vela_syntax::{Item, ScreenDecl, StyleDecl};
use vela_text::TextEngine;

use crate::eval::{Args, Ctx};
use crate::focus::{self, Hotspot};
use crate::instantiate;
use crate::layout::{Constraints, Frame, layout};
use crate::pack::PackedSet;
use crate::paint;
use crate::theme::{self, Fonts, Palette};
use crate::tree::{Node, Size};
use crate::widgets::WidgetRegistry;

/// A screen evaluated and laid out, ready to paint and to be navigated.
#[derive(Clone, PartialEq, Debug)]
pub struct Laid {
    /// The evaluated widget tree.
    pub node: Node,
    /// Where every node goes.
    pub frame: Frame,
    /// Action-bearing nodes, in focus order (`focus::hotspots`).
    pub hotspots: Vec<Hotspot>,
}

/// The screens, styles, and theme of one file.
pub struct ScreenSet {
    screens: Vec<ScreenDecl>,
    styles: Vec<StyleDecl>,
    palette: Palette,
    fonts: Fonts,
    registry: WidgetRegistry,
}

impl ScreenSet {
    /// Compiles a parsed file's items.
    #[must_use]
    pub fn from_items(items: &[Item]) -> Self {
        let mut screens = Vec::new();
        let mut styles = Vec::new();
        let mut palette = Palette::default();
        let mut fonts = Fonts::default();
        let mut theme_taken = false;

        for item in items {
            match item {
                Item::Screen(screen) => screens.push(screen.clone()),
                Item::Style(style) => styles.push(style.clone()),
                // The first theme is the active one. Selecting between themes is a runtime
                // concern (`SCREENS.md §5`) and there is no selection syntax yet; a project
                // with one theme — every project today — does not notice the choice.
                Item::Theme(theme) if !theme_taken => {
                    palette = theme::palette(theme);
                    fonts = theme::fonts(theme);
                    theme_taken = true;
                }
                _ => {}
            }
        }

        Self {
            screens,
            styles,
            palette,
            fonts,
            registry: WidgetRegistry::builtin(),
        }
    }

    /// Replaces the widget vocabulary, which a plugin extends.
    #[must_use]
    pub fn with_registry(mut self, registry: WidgetRegistry) -> Self {
        self.registry = registry;
        self
    }

    /// This set's inputs, for a pack that will be read back without a parse.
    ///
    /// The widget registry is not taken: it is the engine's built-in vocabulary, identical in the
    /// build and in the run, and shipping it would let a bundle claim a widget this engine does
    /// not have.
    #[must_use]
    pub fn packed(&self) -> PackedSet {
        PackedSet {
            screens: self.screens.clone(),
            styles: self.styles.clone(),
            palette: self.palette.clone(),
            fonts: self.fonts.clone(),
        }
    }

    /// Rebuilds a set from a pack, against this build's widget vocabulary.
    #[must_use]
    pub fn from_packed(packed: PackedSet) -> Self {
        Self {
            screens: packed.screens,
            styles: packed.styles,
            palette: packed.palette,
            fonts: packed.fonts,
            registry: WidgetRegistry::builtin(),
        }
    }

    /// Whether a screen by this name is declared.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.screen(name).is_some()
    }

    /// A declared screen.
    #[must_use]
    pub fn screen(&self, name: &str) -> Option<&ScreenDecl> {
        self.screens.iter().find(|screen| screen.name == name)
    }

    /// Every screen name, in declaration order.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.screens
            .iter()
            .map(|screen| screen.name.as_str())
            .collect()
    }

    /// Evaluates a screen to a widget tree, or `None` if it is not declared.
    #[must_use]
    pub fn build(
        &self,
        name: &str,
        args: &Args,
        text: &mut TextEngine,
        font: &str,
        max_width: f32,
    ) -> Option<Node> {
        let screen = self.screen(name)?;
        // The file's screens as references, so a `use` in this one can find the screen it names.
        // Built here rather than stored, because a set that held both the screens and references to
        // them would be a struct borrowing from itself.
        let screens: Vec<&ScreenDecl> = self.screens.iter().collect();
        let ctx = Ctx {
            registry: &self.registry,
            palette: &self.palette,
            fonts: &self.fonts,
            styles: &self.styles,
            screens: &screens,
        };
        Some(instantiate::build(
            &screen.body,
            &ctx,
            args,
            text,
            font,
            max_width,
        ))
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
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
    ) -> Option<Laid> {
        let (width, height) = (size.0 as f32, size.1 as f32);
        let node = self.build(name, args, text, font, width)?;
        let frame = layout(&node, Constraints::exact(Size::new(width, height)));
        let hotspots = focus::hotspots(&node, &frame);
        Some(Laid {
            node,
            frame,
            hotspots,
        })
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
        let Some(laid) = self.lay(name, args, size, text, font) else {
            return false;
        };
        paint::paint(&laid.node, &laid.frame, text, font, draw, None);
        true
    }
}
