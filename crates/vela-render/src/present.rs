//! Turning commands into a draw list.
//!
//! `ARCHITECTURE.md §2` gives `vela-render` "command consumption", and this is it. A command
//! is the only thing the runtime tells the outside world (`ARCHITECTURE.md §7`), so this is
//! the one place a story becomes pixels — and because it is plain data in and plain data out,
//! it is testable without a GPU, which is most of the point.
//!
//! The presenter holds *state*, not a frame: a `Say` stays on screen until the next one, and
//! a `Scene` replaces what a `Show` added. A frame is a picture of that state, which is why
//! `apply` and `build` are separate — a frame can be re-rendered without replaying commands.

use vela_text::TextEngine;
use vela_world::{Command, SceneState, Stage};

use crate::draw::{Color, DrawList, ImageQuad, RectQuad};
use crate::menu::Menu;
use crate::renderer::Renderer;
use crate::text::{self, Placement};

/// How a dialogue box looks.
///
/// A theme, in the plainest sense: the numbers a theme file will provide at M7. Defaults are
/// here rather than in the presenter so a caller can change one without a builder.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Style {
    /// The frame background.
    pub background: Color,
    /// The dialogue box fill.
    pub box_fill: Color,
    /// The speaker's colour.
    pub name: Color,
    /// The dialogue colour.
    pub body: Color,
    /// Body text size in pixels.
    pub size: f32,
    /// Speaker text size in pixels.
    pub name_size: f32,
    /// Distance from the frame edge to the box.
    pub margin: f32,
    /// The box's height.
    pub box_height: f32,
    /// The wash behind the highlighted menu choice.
    pub selection: Color,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            background: Color::rgb(18, 20, 28),
            box_fill: Color {
                r: 0.04,
                g: 0.05,
                b: 0.09,
                a: 0.88,
            },
            name: Color::rgb(204, 190, 150),
            body: Color::rgb(235, 235, 240),
            size: 24.0,
            name_size: 28.0,
            margin: 60.0,
            box_height: 190.0,
            selection: Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.14,
            },
        }
    }
}

/// The line currently being said.
struct Dialogue {
    speaker: Option<String>,
    text: String,
}

/// What is on stage, and what is being said.
pub struct Presenter {
    text: TextEngine,
    scene: SceneState,
    style: Style,
    size: (u32, u32),
    dialogue: Option<Dialogue>,
    menu: Option<Menu>,
    font: String,
    /// Images waiting to reach the GPU, by the name a scene stages them under.
    staged: Vec<(String, u32, u32, Vec<u8>)>,
    /// Where each staged image ended up, once it has been uploaded.
    uploaded: Vec<(String, u32)>,
}

impl Presenter {
    /// A presenter for a frame of `size`, drawing with `font`.
    #[must_use]
    pub fn new(text: TextEngine, font: &str, size: (u32, u32)) -> Self {
        Self {
            text,
            scene: SceneState::default(),
            style: Style::default(),
            size,
            dialogue: None,
            menu: None,
            font: font.to_string(),
            staged: Vec::new(),
            uploaded: Vec::new(),
        }
    }

    /// Adds an image a scene can stage, as pixels.
    ///
    /// Pixels and not a path: the caller decodes (`vela-assets` owns file formats), and the
    /// renderer is handed the result. Nothing here can read a file, which is the adapter rule
    /// doing its job rather than a limitation.
    pub fn stage_image(&mut self, name: impl Into<String>, width: u32, height: u32, rgba: Vec<u8>) {
        self.staged.push((name.into(), width, height, rgba));
    }

    /// Sends every staged image to the GPU, once.
    ///
    /// Called where the renderer becomes available rather than per frame: uploading appends a
    /// texture, so doing it in the frame loop would add one picture per frame to GPU memory.
    /// Calling it twice is harmless — the second call finds nothing staged.
    pub fn upload_images(&mut self, renderer: &mut Renderer) {
        for (name, width, height, rgba) in std::mem::take(&mut self.staged) {
            let id = renderer.upload_image(width, height, &rgba);
            self.uploaded.push((name, id));
        }
    }

    /// The texture id an image name was uploaded as, if it was — public because a screen draws
    /// pictures by the same names a scene does (`SCREENS.md §3`).
    #[must_use]
    pub fn texture_of(&self, name: &str) -> Option<u32> {
        self.uploaded
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, id)| *id)
    }

    /// The injected text engine, so a caller can upload the atlas.
    #[must_use]
    pub fn text(&self) -> &TextEngine {
        &self.text
    }

    /// The text engine mutably.
    pub fn text_mut(&mut self) -> &mut TextEngine {
        &mut self.text
    }

    /// Overrides the default style.
    pub fn set_style(&mut self, style: Style) {
        self.style = style;
    }

    /// The current style.
    #[must_use]
    pub fn style(&self) -> &Style {
        &self.style
    }

    /// What is on stage.
    #[must_use]
    pub fn scene(&self) -> &SceneState {
        &self.scene
    }

    /// Applies one command to the presentation state.
    ///
    /// Commands with no visual consequence — audio, pauses, transitions — are ignored rather
    /// than unsupported. They are *not* this layer's business, and a `match` that refused
    /// them would make the presenter a second copy of the command vocabulary that has to be
    /// updated whenever it grows.
    pub fn apply(&mut self, command: &Command) {
        // A menu stands until it is answered, and the answer *is* the next command arriving.
        // Any other command therefore means the menu is gone — without this, a menu would sit
        // on screen forever behind the dialogue that replaced it.
        if !matches!(command, Command::Menu { .. }) {
            self.menu = None;
        }

        match command {
            Command::Say { speaker, text, .. } => {
                self.dialogue = Some(Dialogue {
                    speaker: speaker.clone(),
                    text: text.clone(),
                });
            }
            Command::Menu { prompt, choices } => {
                self.menu = Some(Menu::new(
                    prompt.clone(),
                    choices.iter().map(|choice| choice.text.clone()).collect(),
                ));
            }
            Command::Stage { kind, image, .. } => match kind {
                Stage::Scene => self.scene.scene(image.clone()),
                Stage::Show => self.scene.show(image.clone()),
                Stage::Hide => self.scene.hide(image),
            },
            Command::Transition { .. }
            | Command::Audio { .. }
            | Command::Pause { .. }
            | Command::WaitClick => {}
        }
    }

    /// The menu awaiting an answer, if one is.
    #[must_use]
    pub fn menu(&self) -> Option<&Menu> {
        self.menu.as_ref()
    }

    /// The highlighted choice, if a menu is up.
    #[must_use]
    pub fn selection(&self) -> Option<usize> {
        self.menu.as_ref().map(|menu| menu.selected)
    }

    /// Moves the menu selection, wrapping at both ends. Returns whether a menu took it.
    pub fn move_selection(&mut self, delta: isize) -> bool {
        self.menu.as_mut().is_some_and(|menu| menu.move_by(delta))
    }

    /// Applies every command in order.
    pub fn apply_all<'a>(&mut self, commands: impl IntoIterator<Item = &'a Command>) {
        for command in commands {
            self.apply(command);
        }
    }

    /// Draws the current state: the backdrop, the dialogue, and the menu over it.
    pub fn build(&mut self, draw: &mut DrawList) {
        self.build_backdrop(draw);
        self.build_dialogue(draw);
        self.build_menu(draw);
    }

    /// Draws everything but the dialogue box — the frame background and the staged images.
    ///
    /// A caller that draws the dialogue from a screen uses this, so the built-in box does not
    /// paint underneath the screen that replaced it.
    pub fn build_backdrop(&mut self, draw: &mut DrawList) {
        let (width, height) = (self.size.0 as f32, self.size.1 as f32);
        draw.push_rect(RectQuad::from_corners(
            0.0,
            0.0,
            width,
            height,
            self.style.background,
        ));
        self.build_stage(draw, width, height);
    }

    /// The dialogue line currently on screen, if any.
    ///
    /// The presenter already knows what it would draw; this exposes it so a caller can draw the
    /// same line itself — through a screen — rather than the presenter keeping two pictures of
    /// the state.
    #[must_use]
    pub fn line(&self) -> Option<(Option<&str>, &str)> {
        self.dialogue
            .as_ref()
            .map(|dialogue| (dialogue.speaker.as_deref(), dialogue.text.as_str()))
    }

    /// The staged images.
    ///
    /// Placeholders, and honestly so: decoding an image is an *asset* question
    /// (`BUILD_AND_ASSETS.md`), not a presentation one, and there is no decoder yet. Each
    /// placeholder is a rectangle tinted by a hash of the image's name, so the same story
    /// always produces the same colours and a test can tell two images apart. The name is
    /// drawn on it, because a wrong image is much easier to see than a wrong colour.
    fn build_stage(&mut self, draw: &mut DrawList, width: f32, height: f32) {
        let images: Vec<String> = self
            .scene
            .images()
            .iter()
            .map(|staged| staged.image.clone())
            .collect();
        let count = images.len().max(1);
        let slot = width / count as f32;

        for (index, image) in images.iter().enumerate() {
            let left = index as f32 * slot;

            // The picture, when there is one. A scene stages a *name*, so a name with no image
            // behind it — a typo, or an asset the build has not produced — falls through to the
            // placeholder rather than to nothing: a blank screen reads as a broken renderer, and
            // a labelled one reads as a missing background.
            if let Some(texture) = self.texture_of(image) {
                draw.push_image(ImageQuad {
                    x: left,
                    y: 0.0,
                    width: slot,
                    height,
                    uv: [0.0, 0.0, 1.0, 1.0],
                    image: texture,
                    color: Color {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0,
                    },
                });
                continue;
            }

            let tint = tint_of(image);
            draw.push_rect(RectQuad::from_corners(left, 0.0, left + slot, height, tint));
            text::place(
                &mut self.text,
                &self.font,
                draw,
                image,
                Placement {
                    size: self.style.size * 1.6,
                    left: left + 24.0,
                    top: 40.0,
                    max_width: width,
                },
                Color {
                    a: 0.9,
                    ..Color::rgb(255, 255, 255)
                },
            );
        }
    }

    /// The dialogue box, or nothing when no line is on screen.
    ///
    /// Public so a caller drawing the dialogue from a screen can still have the menu, which is
    /// not a screen and never will be — a choice list's length is the story's.
    pub fn build_dialogue(&mut self, draw: &mut DrawList) {
        let (width, height) = (self.size.0 as f32, self.size.1 as f32);
        let Some(dialogue) = self.dialogue.as_ref() else {
            return;
        };
        let speaker = dialogue.speaker.clone();
        let text = dialogue.text.clone();

        let margin = self.style.margin;
        let top = height - self.style.box_height - margin;
        draw.push_rect(RectQuad::from_corners(
            margin,
            top,
            width - margin,
            height - margin,
            self.style.box_fill,
        ));

        let left = margin + 32.0;
        let width = width - left - margin - 32.0;
        let mut baseline = top + 34.0;
        if let Some(speaker) = speaker {
            let height = text::place(
                &mut self.text,
                &self.font,
                draw,
                &speaker,
                Placement {
                    size: self.style.name_size,
                    left,
                    top: baseline,
                    max_width: width,
                },
                self.style.name,
            );
            baseline += height;
        }
        text::place(
            &mut self.text,
            &self.font,
            draw,
            &text,
            Placement {
                size: self.style.size,
                left,
                top: baseline,
                max_width: width,
            },
            self.style.body,
        );
    }

    /// The menu, centred over whatever is behind it.
    pub fn build_menu(&mut self, draw: &mut DrawList) {
        if let Some(menu) = self.menu.as_ref() {
            menu.build(&mut self.text, &self.font, &self.style, self.size, draw);
        }
    }
}

/// A stable colour for an image name.
///
/// A hash, not a random colour, and not `DefaultHasher` — which is seeded per process and
/// would give a different picture each run, making a capture useless as a comparison.
#[must_use]
pub fn tint_of(name: &str) -> Color {
    let mut hash: u32 = 2166136261;
    for byte in name.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16777619);
    }
    Color {
        r: 0.18 + (hash & 0xFF) as f32 / 255.0 * 0.22,
        g: 0.18 + ((hash >> 8) & 0xFF) as f32 / 255.0 * 0.22,
        b: 0.22 + ((hash >> 16) & 0xFF) as f32 / 255.0 * 0.22,
        a: 1.0,
    }
}
