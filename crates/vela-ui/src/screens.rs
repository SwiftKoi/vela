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
//!
//! How a screen becomes something to look at is `laying.rs`, beside this.

use vela_syntax::{Item, ScreenDecl, StyleDecl};

use crate::actions::Action;
use crate::eval::{Ctx, ScreenState};
use crate::focus::Hotspot;
use crate::images::ImageTable;
use crate::layout::Frame;
use crate::pack::PackedSet;
use crate::theme::{self, Fonts, Palette};
use crate::tree::Node;
use crate::variants::Variants;
use crate::widgets::WidgetRegistry;

mod laying;

/// A screen evaluated and laid out, ready to paint and to be navigated.
#[derive(Clone, PartialEq, Debug)]
pub struct Laid {
    /// The evaluated widget tree.
    pub node: Node,
    /// Where every node goes.
    pub frame: Frame,
    /// Action-bearing nodes, in focus order (`focus::hotspots`).
    pub hotspots: Vec<Hotspot>,
    /// The semantic actions this screen answers while it is shown (`SCREENS.md §2.3`).
    pub keys: Vec<KeyBinding>,
    /// The deadlines it declares. Data, because nothing fires them yet — see [`Timer`].
    pub timers: Vec<Timer>,
    /// The screen's own variables, after this layout (`SCREENS.md §2.5`).
    ///
    /// Carried here rather than kept by the runtime, because a layout is what *initializes* them: the
    /// value of a name the caller already had is kept, and one it does not is its initializer. The
    /// caller holds this and hands it back to the next `lay`, which is how `set_screen_variable` and a
    /// hot reload both survive.
    pub state: ScreenState,
}

impl Laid {
    /// The action this screen answers `name` with, if it binds it.
    ///
    /// The first binding wins, as the first arm of an `if` does: a screen that binds one action twice
    /// has one answer, and it is the one a reader meets first.
    #[must_use]
    pub fn key(&self, name: &str) -> Option<&Action> {
        self.keys
            .iter()
            .find(|binding| binding.name == name)
            .map(|binding| &binding.action)
    }
}

/// A semantic action a screen answers, and what it does about it (`SCREENS.md §2.3`).
#[derive(Clone, PartialEq, Debug)]
pub struct KeyBinding {
    /// The host's semantic action — `cancel`, `menu_up`, … — as the screen wrote it.
    pub name: String,
    /// What the screen does when it arrives.
    pub action: Action,
}

/// A deadline a screen declares (`SCREENS.md §2.3`).
///
/// Both halves are resolved — the delay to seconds, the action to a call — and nothing fires it.
/// `SCREENS.md §6` runs animation from `World::clock`, no clock reaches the screen runtime, and a
/// `timer` is therefore a declaration the runtime carries and does not yet act on. Said here, on the
/// type, rather than only in the docs: a `timer` that quietly never fires is exactly the failure that
/// looks like the project's mistake.
#[derive(Clone, PartialEq, Debug)]
pub struct Timer {
    /// How long, in seconds.
    pub seconds: f32,
    /// What the screen does when it elapses.
    pub action: Action,
    /// Whether it fires again.
    pub repeat: bool,
}

/// The screens, styles, and theme of one file.
pub struct ScreenSet {
    screens: Vec<ScreenDecl>,
    styles: Vec<StyleDecl>,
    palette: Palette,
    fonts: Fonts,
    images: ImageTable,
    variants: Variants,
    design: (f32, f32),
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
            images: ImageTable::new(),
            // No bundle has said where this runs, and no frame has been laid out yet (`§2.6`): a
            // `variant(...)` question is false until a runner that knows the answer supplies one.
            variants: Variants::new(),
            design: crate::REFERENCE_FRAME,
            registry: WidgetRegistry::builtin(),
        }
    }

    /// Supplies the pictures this set's screens may draw.
    ///
    /// Injected rather than compiled, because only the platform knows which texture a name became
    /// (`images.rs`): `vela build` compiles the *screens*, and the runner that uploaded the assets is
    /// the one that can say where they are. A set that was never given a table draws no pictures and is
    /// otherwise correct — which is what a headless run is.
    #[must_use]
    pub fn with_images(mut self, images: ImageTable) -> Self {
        self.set_images(images);
        self
    }

    /// Replaces the pictures this set resolves names against.
    ///
    /// The in-place half of [`with_images`](Self::with_images), for a caller holding a `Vec` of sets
    /// rather than building them.
    pub fn set_images(&mut self, images: ImageTable) {
        self.images = images;
    }

    /// The pictures this set resolves names against.
    #[must_use]
    pub fn images(&self) -> &ImageTable {
        &self.images
    }

    /// Replaces what the host says about where these screens are running (`SCREENS.md §2.6`).
    ///
    /// The platform half of the variant set, from the bundle's descriptor
    /// (`commands/target.rs`); the size class is added per frame by [`lay`](Self::lay), which is the
    /// only place a frame is known.
    pub fn set_variants(&mut self, variants: Variants) {
        self.variants = variants;
    }

    /// Replaces what the host says about where these screens are running.
    ///
    /// The in-place half of this, for a caller holding a `Vec` of sets rather than building them.
    #[must_use]
    pub fn with_variants(mut self, variants: Variants) -> Self {
        self.set_variants(variants);
        self
    }

    /// What the host says about where these screens are running.
    #[must_use]
    pub fn variants(&self) -> Variants {
        self.variants
    }

    /// Sets the frame this project is *designed* for (`SCREENS.md §2.6`).
    ///
    /// What `variant("small")` is measured against: a screen is small when the frame it is drawn in
    /// has shrunk below this one. `vela.toml`'s `[project] size` is where it comes from, and
    /// [`REFERENCE_FRAME`](crate::REFERENCE_FRAME) is what a project that declares none gets.
    pub fn set_design(&mut self, design: (f32, f32)) {
        self.design = design;
    }

    /// The frame this project is designed for.
    #[must_use]
    pub fn design(&self) -> (f32, f32) {
        self.design
    }

    /// The colours this set's theme declares.
    #[must_use]
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// Replaces the colours this set resolves `theme.` tokens against (`SCREENS.md §2.7`).
    ///
    /// What a caller does with a *second* palette rather than a project's own: the engine's interface is
    /// laid out with the game's colours laid over Vela's, so a frame drawn from the interface is the
    /// game's colours. The tokens are the ones the set declares — [`Palette::over`] keeps both sides.
    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
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
            // From a bundle nobody has uploaded anything yet, so the pictures arrive the same way they
            // do for a project: `set_images`, once a window exists (`SCREENS.md §3`).
            images: ImageTable::new(),
            // Likewise the variants: the descriptor carries them (`§2.6`), and the runner that read the
            // bundle is what knows them.
            variants: Variants::new(),
            design: crate::REFERENCE_FRAME,
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

    /// Runs a visitor over a screen body with this set's resolution context.
    ///
    /// The context borrows this set's styles, palette, fonts, and screens, so it cannot be handed out —
    /// and it is built twice for one screen (`build` wants a tree, `bindings` wants the inputs), which
    /// is why it is built here once rather than at each call site.
    fn with_ctx<R>(&self, visit: impl FnOnce(&Ctx<'_>) -> R) -> R {
        // Built here rather than stored, because a set that held both the screens and references to
        // them would be a struct borrowing from itself.
        let screens: Vec<&ScreenDecl> = self.screens.iter().collect();
        let ctx = Ctx {
            registry: &self.registry,
            palette: &self.palette,
            fonts: &self.fonts,
            images: &self.images,
            styles: &self.styles,
            screens: &screens,
        };
        visit(&ctx)
    }
}
