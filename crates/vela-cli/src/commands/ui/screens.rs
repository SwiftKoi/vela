//! The screens a project declares, and what can be asked of them.

use std::fs;
use std::path::{Path, PathBuf};

use vela_render::DrawList;
use vela_text::TextEngine;
use vela_ui::screens::Laid;
use vela_ui::{Args, ScreenSet, ScreenState, Value};

use crate::commands::check::Project;

pub struct Screens {
    sets: Vec<ScreenSet>,
    paths: Vec<PathBuf>,
}

impl Screens {
    /// Compiles the screens in a project.
    ///
    /// A file with parse errors contributes nothing: `vela run` refuses a project with errors
    /// before it gets here, so this is a guard rather than a path a user can reach.
    #[must_use]
    pub fn load(project: &Project) -> Self {
        Self {
            sets: compile(&project.files).0,
            paths: project.files.clone(),
        }
    }

    /// No screens at all.
    ///
    /// What a built bundle gets: screens are compiled from source, and a distribution bundle
    /// ships none. The presenter's built-in dialogue box and menu are what draw instead, which
    /// is a real game rather than a placeholder — a project that declared screens simply gets
    /// the engine's own until screens are packed too.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            sets: Vec::new(),
            paths: Vec::new(),
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
        Ok(Self { sets, paths })
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

    /// How many screens are compiled.
    #[must_use]
    pub fn count(&self) -> usize {
        self.sets.iter().map(|set| set.names().len()).sum()
    }

    /// Re-reads the files and rebuilds the sets — hot reload's one step.
    ///
    /// A file that no longer parses does **not** replace the working set: an author whose edit
    /// is half-written should see a message and the last good screen, not a blank window.
    pub fn reload(&mut self) -> Reloaded {
        let (sets, errors) = compile(&self.paths);
        if errors > 0 {
            return Reloaded {
                screens: self.count(),
                errors,
            };
        }
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

    /// Evaluates and lays out a declared screen, or `None` if it is not declared.
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
        for set in &self.sets {
            if set.has(name) {
                return set.lay(name, args, state, size, text, font);
            }
        }
        None
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
        for set in &self.sets {
            if set.has(name) {
                return set.draw(name, args, size, text, font, draw);
            }
        }
        false
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
