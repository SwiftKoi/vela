//! The screens a project declares, and what can be asked of them.

use std::fs;
use std::path::{Path, PathBuf};

use vela_render::DrawList;
use vela_text::TextEngine;
use vela_ui::{Args, ScreenSet, Value};

use crate::commands::check::Project;

pub struct Screens {
    sets: Vec<ScreenSet>,
    /// How many of those sets are the *project's*.
    ///
    /// Kept rather than inferred, because `sets` ends with the interface (`SCREENS.md §2.7`) and a count
    /// that included it would make "reload: 2 screen(s)" the answer for a one-screen project.
    project: usize,
    paths: Vec<PathBuf>,
    /// What the player has chosen, so a screen drawn straight to a frame can read it (`RUNTIME.md §2.1`).
    ///
    /// A copy rather than the world itself: this layer compiles a project's screens and knows nothing about
    /// a story, and the caller that owns the session is what says what the player has chosen — the same
    /// shape the pictures and the variants arrive in.
    preferences: vela_world::Preferences,
    /// The slots the host found, for the same reason: a save screen asks `slots(6)` and the answer comes
    /// from the saves directory, which this layer is what knows about.
    slots: Vec<vela_ui::Slot>,
}

impl Screens {
    /// Compiles the screens in a project.
    ///
    /// A file with parse errors contributes nothing: `vela run` refuses a project with errors
    /// before it gets here, so this is a guard rather than a path a user can reach.
    #[must_use]
    pub fn load(project: &Project) -> Self {
        let mut sets = compile(&project.files).0;
        let project_screens: usize = sets.iter().map(|set| set.names().len()).sum();
        sets.push(interface(colours(&sets)));
        Self {
            sets,
            project: project_screens,
            paths: project.files.clone(),
            preferences: vela_world::Preferences::new(),
            slots: Vec::new(),
        }
    }

    /// The interface and nothing else.
    ///
    /// What a built bundle with no screens gets: `vela build` packs the *project's* screens
    /// (`SCREENS.md §13`) and a distribution bundle carries those, so a project that declared none draws
    /// from Vela's own interface — which belongs to the engine that is running rather than to the
    /// artifact, and is parsed once here (`vela_ui::interface`).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            sets: vec![interface(None)],
            project: 0,
            paths: Vec::new(),
            preferences: vela_world::Preferences::new(),
            slots: Vec::new(),
        }
    }

    /// Loads the screens a built bundle carries.
    ///
    /// `vela build` compiles each source module's screens into `screens/<module>.velspk`
    /// (`SCREENS.md §13`); this deserializes them. No `.vela` file is read and no parser runs — the
    /// interface is compiled at build time exactly as the story is, which is the whole point of a
    /// built artifact (`RUNTIME.md §8`).
    ///
    /// A bundle with no screens is not an error: `examples/hello` declares none, and its run falls
    /// back to the presenter's built-in box.
    ///
    /// # Errors
    ///
    /// Fails on a pack this build cannot read, or one written at a version it does not know —
    /// named rather than skipped, because a screen silently dropped is a blank box on screen.
    pub fn load_bundle(dir: &Path) -> Result<Self, vela_ui::PackError> {
        let Ok(entries) = fs::read_dir(dir.join("screens")) else {
            return Ok(Self::empty());
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "velspk"))
            .collect();
        // Sorted, so two runs of one bundle build the same sets in the same order.
        paths.sort();

        let mut sets = Vec::new();
        for path in &paths {
            sets.push(vela_ui::ScreenPack::read(path)?.into_set());
        }
        // The interface last, so a project's screen of the same name is found first: the resolution order
        // is the whole of the override rule (`SCREENS.md §2.1`), and its palette comes from the game.
        sets.push(interface(colours(&sets)));
        Ok(Self {
            sets,
            project: 0,
            paths,
            preferences: vela_world::Preferences::new(),
            slots: Vec::new(),
        })
    }

    /// The files this set was built from.
    #[must_use]
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Tells every set where its pictures are, once the platform has uploaded them.
    ///
    /// A second step rather than a constructor argument because the two halves happen at different
    /// moments: a set is compiled before a window exists, and a texture exists only after one has
    /// uploaded it (`SCREENS.md §3`). A set nobody tells draws no pictures and is otherwise correct,
    /// which is what a headless run is.
    pub fn set_images(&mut self, images: vela_ui::ImageTable) {
        for set in &mut self.sets {
            set.set_images(images.clone());
        }
    }

    /// Tells every set where it is running, from the bundle's descriptor (`SCREENS.md §2.6`).
    ///
    /// The size class is not here: a frame is what decides that, and every set receives one when it is
    /// laid out — so a window that is resized asks the question again with the new answer.
    pub fn set_variants(&mut self, variants: vela_ui::Variants) {
        for set in &mut self.sets {
            set.set_variants(variants);
        }
    }

    /// Tells every set which frame the game was designed for (`SCREENS.md §2.6`).
    ///
    /// From `vela.toml`'s `[project] size` in a project, and from a bundle's manifest in a built
    /// one. What `variant("small")` measures against: a frame is small once it has shrunk below
    /// three quarters of this, so a project designed at 1920×1080 is not `small` at 1500×900.
    pub fn set_design(&mut self, design: (f32, f32)) {
        for set in &mut self.sets {
            set.set_design(design);
        }
    }

    /// How many screens the project compiled.
    ///
    /// The interface's screens are not counted: they are Vela's (`SCREENS.md §2.7`), they do not change
    /// under a reload, and a player reading "reload: 2 screen(s)" after editing one file would be reading
    /// a number about the engine.
    #[must_use]
    pub fn count(&self) -> usize {
        self.project
    }

    /// Re-reads the files and rebuilds the sets — hot reload's one step.
    ///
    /// A file that no longer parses does **not** replace the working set: an author whose edit
    /// is half-written should see a message and the last good screen, not a blank window.
    pub fn reload(&mut self) -> Reloaded {
        let (mut sets, errors) = compile(&self.paths);
        if errors > 0 {
            return Reloaded {
                screens: self.count(),
                errors,
            };
        }
        self.project = sets.iter().map(|set| set.names().len()).sum();
        sets.push(interface(colours(&sets)));
        self.sets = sets;
        Reloaded {
            screens: self.count(),
            errors: 0,
        }
    }

    /// Whether any file declares this screen.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.sets.iter().any(|set| set.has(name))
    }

    /// The arguments a `dialogue` screen is called with for a spoken line.
    ///
    /// Positional by convention — `name` then `line` — because the alternative is the engine
    /// knowing the parameter names a project chose, which is the wrong direction. A screen that
    /// names its parameters differently draws the values its own way; one that reorders them
    /// gets the speaker and the line the wrong way round, which is visible immediately.
    #[must_use]
    pub fn dialogue(speaker: Option<&str>, line: &str) -> Args {
        let mut args = Args::new();
        args.set(
            "name",
            speaker.map_or(Value::None, |speaker| Value::Str(speaker.to_string())),
        );
        args.set("line", Value::Str(line.to_string()));
        args
    }

    /// Every set the project compiled, which is what a screen stack lays against (`vela-ui`'s
    /// `ScreenSource` is implemented for a slice of them).
    #[must_use]
    pub fn sets(&self) -> &[ScreenSet] {
        &self.sets
    }

    /// Paints a declared screen into `draw`, returning whether it was found.
    pub fn draw(
        &self,
        name: &str,
        args: &Args,
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
        draw: &mut DrawList,
    ) -> bool {
        // The player's settings travel with the call, the way the variants do (`RUNTIME.md §2.1`): a screen
        // drawn straight to a frame reads `setting("…")` and has to be answered.
        let args = args
            .clone()
            .with_preferences(self.preferences.clone())
            .with_slots(self.slots.clone());
        for set in &self.sets {
            if set.has(name) {
                return set.draw(name, &args, size, text, font, draw);
            }
        }
        false
    }

    /// Tells every set what the player has chosen, so a screen can read a setting (`RUNTIME.md §2.1`).
    pub fn set_preferences(&mut self, preferences: vela_world::Preferences) {
        self.preferences = preferences;
    }

    /// Tells every set what slots the host found, so a save screen can draw a page of them.
    ///
    /// The third answer a scope carries beside the variants and the settings, and the same shape: the layer
    /// that knows about files is this one, and a screen asks (`vela-ui::slots`).
    pub fn set_slots(&mut self, slots: Vec<vela_ui::Slot>) {
        self.slots = slots;
    }
}

/// The save schema a project declares — every `default`, `struct`, and `enum` across its files.
///
/// Derived here rather than in the compiler for the same rank reason screens are: the CLI is
/// the layer that can see a project's sources and a save's header at once.
#[must_use]
pub fn schema(files: &[PathBuf]) -> vela_replay::Schema {
    let mut items = Vec::new();
    for path in files {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let parsed = vela_syntax::parse(vela_span::FileId::from_raw(0), &text);
        items.extend(parsed.program.items);
    }
    vela_replay::Schema::derive(&items)
}

/// What a reload found.
pub struct Reloaded {
    /// How many screens the compiled set holds.
    pub screens: usize,
    /// How many files failed to parse, and so were not compiled.
    pub errors: usize,
}

/// The colours a game's screens declare, which are the ones the interface draws in.
///
/// The *first* set that declares a theme, because a set is one file and the rule is already "the first
/// theme in a file is the active one" (`SCREENS.md §5`). A project with several themes has the open
/// question that rule has — nothing selects between them yet — and a project with none gets `None`,
/// which leaves the interface in Vela's own colours.
fn colours(sets: &[vela_ui::ScreenSet]) -> Option<&vela_ui::Palette> {
    sets.iter()
        .map(|set| set.palette())
        .find(|palette| !palette.is_empty())
}

/// The interface's set, every time a project is loaded or reloaded.
///
/// A fresh set per load rather than one shared, because a set is mutated as a runner tells it things —
/// where the pictures are, where it is running, which frame it was designed for
/// (`set_images`/`set_variants`/`set_design`) — and two callers sharing one would be two callers sharing
/// those answers. The game's colours go over Vela's, token by token (`SCREENS.md §2.7`).
fn interface(colours: Option<&vela_ui::Palette>) -> vela_ui::ScreenSet {
    let mut set = vela_ui::interface::set();
    if let Some(colours) = colours {
        let palette = set.palette().over(colours);
        set.set_palette(palette);
    }
    set
}

/// Compiles every file's screens, and counts the files that would not parse.
///
/// Per file, like the checker: a screen resolves its styles where they are declared.
fn compile(files: &[PathBuf]) -> (Vec<ScreenSet>, usize) {
    let mut sets = Vec::new();
    let mut errors = 0;
    for path in files {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let parsed = vela_syntax::parse(vela_span::FileId::from_raw(0), &text);
        if !parsed.diagnostics.is_empty() {
            errors += 1;
            continue;
        }
        sets.push(ScreenSet::from_items(&parsed.program.items));
    }
    (sets, errors)
}
