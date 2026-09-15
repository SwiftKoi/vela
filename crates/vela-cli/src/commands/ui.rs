//! The screens a project declares, ready for the presenter to draw.
//!
//! The compiler and the VM know nothing about screens: a screen is not a command, and the
//! widget vocabulary lives at rank 8 while compiling is rank 7. So the party that reaches a
//! screen is the one that *consumes* it — here, the CLI — exactly as `check.rs` explains for
//! diagnostics.
//!
//! A screen is drawn when a known command calls for one by name. The one such call today is
//! `dialogue`: a `Say` carries a speaker and a line, which is precisely what the example's
//! `dialogue(name: str?, line: str)` takes. A project without a `dialogue` screen falls back to
//! the presenter's built-in box, so `examples/hello` is unaffected.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use vela_render::{Color, DrawList, RectQuad};
use vela_text::TextEngine;
use vela_ui::actions::Action as ScreenAction;
use vela_ui::screens::Laid;
use vela_ui::{Args, Rect, ScreenSet, Value};

use crate::commands::check::Project;

/// Every screen a project declares, one set per file.
///
/// The files are remembered so the set can be rebuilt from them — that is what hot reload is.
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
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
    ) -> Option<Laid> {
        for set in &self.sets {
            if set.has(name) {
                return set.lay(name, args, size, text, font);
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

/// Notices when any of a set of files changes, by modification time.
///
/// No clock is read: a stamp is compared with the last one seen, so this cannot be a source of
/// nondeterminism even though the values it compares are times. It polls rather than using an
/// OS watcher, because a watcher library is a dependency and a platform surface for a handful
/// of files — and the idle tick that drives it already exists.
pub struct Watcher {
    stamps: Vec<(PathBuf, Option<SystemTime>)>,
}

impl Watcher {
    /// Remembers the current stamp of every file.
    #[must_use]
    pub fn new(paths: &[PathBuf]) -> Self {
        Self {
            stamps: paths
                .iter()
                .map(|path| (path.clone(), stamp(path)))
                .collect(),
        }
    }

    /// Whether any file changed since the last call, updating the stamps.
    pub fn changed(&mut self) -> bool {
        let mut changed = false;
        for (path, last) in &mut self.stamps {
            let current = stamp(path);
            if *last != current {
                *last = current;
                changed = true;
            }
        }
        changed
    }
}

/// A file's modification time, or `None` if it cannot be read.
fn stamp(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
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
        let Some(laid) = screens.lay(name, &Args::new(), size, text, font) else {
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

    /// The action the focused hotspot asks for, if the top screen has one.
    #[must_use]
    pub fn focused(&self) -> Option<&ScreenAction> {
        let top = self.overlays.last()?;
        top.laid
            .hotspots
            .get(top.focus)
            .map(|hotspot| &hotspot.action)
    }

    /// Paints every open screen, bottom first, then a highlight over the focused hotspot.
    ///
    /// The highlight is the runtime's, not the screen's: focus has to be *visible* for the
    /// screen to be usable without sight of a pointer, and a screen that drew its own focus
    /// ring would be a screen that can forget to.
    pub fn paint(&self, text: &mut TextEngine, font: &str, draw: &mut DrawList) {
        for overlay in &self.overlays {
            vela_ui::paint(&overlay.laid.node, &overlay.laid.frame, text, font, draw);
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
            if let Some(laid) = screens.lay(&overlay.name, &Args::new(), size, text, font) {
                overlay.focus = overlay.focus.min(laid.hotspots.len().saturating_sub(1));
                overlay.laid = laid;
            }
        }
    }

    /// The rectangle of the focused hotspot, if any.
    #[must_use]
    pub fn focus_rect(&self) -> Option<Rect> {
        let top = self.overlays.last()?;
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
