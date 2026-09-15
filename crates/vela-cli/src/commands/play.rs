//! Playing a story in a window.
//!
//! The interactive counterpart of `vela run --headless`: the same module, the same session,
//! the same presenter, with a person in the loop instead of `TakeFirst`. The VM suspends at
//! each command (`ARCHITECTURE.md §7`), this presents it, and an action resumes it — which is
//! why the answer is *the player's* rather than a recording.
//!
//! Every presented command is also written to stdout, prefixed. That is not debug output: it
//! is how a harness with no way to see the window can still tell that a keypress reached the
//! story, and it is what `tools/drive.sh` reads.

use std::io::Write;
use std::path::PathBuf;

use vela_render::{DrawList, Presenter, RenderGraph, Surface};
use vela_replay::{Save, Timeline};
use vela_text::{Font, TextEngine};
use vela_world::Input;

use crate::command::Error;
use crate::commands::ui::{Screens, Stack, Watcher};

/// The bundled default face. See `assets/fonts/README.md`.
const FACE: &[u8] = include_bytes!("../../../../assets/fonts/LiberationSans-Regular.ttf");
const FACE_NAME: &str = "sans";

/// A story being played.
pub struct Player {
    /// The module, kept so a load can restore into it.
    module: vela_bytecode::Module,
    /// The story and its rollback history.
    timeline: Timeline,
    presenter: Presenter,
    surface: Option<Surface>,
    graph: RenderGraph,
    size: (u32, u32),
    finished: bool,
    /// Whether a frame has been drawn.
    drew: bool,
    /// The project's screens. A `dialogue` screen draws the dialogue when it is declared;
    /// without one the presenter's built-in box is used instead.
    screens: Screens,
    /// The screens the story opened, drawn over the dialogue and navigated by focus.
    overlays: Stack,
    /// Whether a screen asked to leave, so the host closes the window.
    quit: bool,
    /// Watches the project's sources, so an edit is recompiled into the running window.
    watcher: Watcher,
    /// Where saves are written.
    saves: PathBuf,
    /// The schema a save is written against: a load migrates an older save into it
    /// (`RUNTIME.md §6`), and refuses one whose schema it cannot reach.
    schema: vela_replay::Schema,
}

impl Player {
    /// Prepares a player for `module`, starting at `label`, in a frame of `size`.
    ///
    /// # Errors
    ///
    /// Fails if the entry label does not exist.
    pub fn new(
        module: &vela_bytecode::Module,
        label: &str,
        size: (u32, u32),
        screens: Screens,
        saves: PathBuf,
        schema: vela_replay::Schema,
        images: Vec<(String, u32, u32, Vec<u8>)>,
    ) -> Result<Self, Error> {
        let font = Font::from_bytes(FACE.to_vec(), 0)
            .ok_or_else(|| Error::internal("the bundled font failed to load".to_string()))?;
        let mut text = TextEngine::new();
        text.add_font(FACE_NAME, font);

        // Built before `screens` is moved: the watcher remembers the same files the screens
        // were compiled from.
        let watcher = Watcher::new(screens.paths());

        Ok(Self {
            timeline: Timeline::start(module, label)
                .map_err(|fault| Error::internal(format!("{fault}")))?,
            module: module.clone(),
            presenter: {
                let mut presenter = Presenter::new(text, FACE_NAME, size);
                for (name, width, height, rgba) in images {
                    presenter.stage_image(name, width, height, rgba);
                }
                presenter
            },
            surface: None,
            graph: graph(),
            size,
            finished: false,
            drew: false,
            screens,
            overlays: Stack::default(),
            quit: false,
            watcher,
            saves,
            schema,
        })
    }

    /// Applies every command up to the next suspension.
    fn consume(&mut self, mut step: vela_vm::Step, out: &mut dyn Write) {
        loop {
            match step {
                vela_vm::Step::Yield(command) => {
                    // Written before it is drawn, so a harness reading a pipe sees the same
                    // order a player watching the window does.
                    let _ = writeln!(out, "present {command}");
                    let _ = out.flush();
                    self.presenter.apply(&command);
                    return;
                }
                vela_vm::Step::Continue => step = self.timeline.advance(),
                vela_vm::Step::Halt => {
                    let _ = writeln!(out, "present <end>");
                    let _ = out.flush();
                    self.finished = true;
                    return;
                }
                vela_vm::Step::Fault(fault) => {
                    let _ = writeln!(out, "present <fault: {fault}>");
                    self.finished = true;
                    return;
                }
            }
        }
    }

    /// The first command.
    pub fn begin(&mut self, out: &mut dyn Write) {
        let step = self.timeline.advance();
        self.consume(step, out);
    }

    /// Answers the current suspension and runs on.
    pub fn advance(&mut self, out: &mut dyn Write) {
        let step = self.timeline.answer(Input::Ack);
        self.consume(step, out);
    }

    /// Answers the current menu with the chosen entry.
    ///
    /// A menu is a suspension whose answer is an index, not an acknowledgement — answering it
    /// with `Ack` would send `none` where the story indexes a list.
    fn choose(&mut self, index: usize) {
        println!("choose {index}");
        let step = self.timeline.answer(Input::Choice(index));
        let mut sink = std::io::stdout();
        self.consume(step, &mut sink);
    }

    /// Steps back one command, replaying from the nearest snapshot.
    fn rollback(&mut self) {
        let position = self.timeline.position();
        if position == 0 {
            return;
        }
        let reached = self.timeline.rollback(position - 1);
        self.finished = false;
        self.refresh_presentation();
        println!("rollback {reached}");
    }

    /// Writes the current state to a slot.
    fn save(&mut self, slot: &str) {
        if std::fs::create_dir_all(&self.saves).is_err() {
            println!("save failed: cannot create {}", self.saves.display());
            return;
        }
        let save = Save::new(self.timeline.snapshot(), self.schema.digest(), slot);
        let path = self.saves.join(format!("{slot}.velasave"));
        match save.write_atomic(&path) {
            Ok(()) => println!("save {slot}"),
            Err(error) => println!("save failed: {error}"),
        }
    }

    /// Reads a slot back into the running story, migrating it if it is from an older build.
    fn load(&mut self, slot: &str) {
        let path = self.saves.join(format!("{slot}.velasave"));
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                println!("load failed: {error}");
                return;
            }
        };
        let written = vela_replay::Save::version_of(&bytes).unwrap_or(vela_replay::SAVE_VERSION);
        let save = match Save::load(&bytes, &vela_replay::chain(), &self.schema) {
            Ok(save) => save,
            Err(error) => {
                println!("load failed: {error}");
                return;
            }
        };
        match Timeline::resume(&self.module, &save.snapshot) {
            Ok(timeline) => {
                self.timeline = timeline;
                self.finished = false;
                self.refresh_presentation();
                if written == vela_replay::SAVE_VERSION {
                    println!("load {slot}");
                } else {
                    println!(
                        "load {slot} (migrated {written} -> {})",
                        vela_replay::SAVE_VERSION
                    );
                }
            }
            Err(fault) => println!("load failed: {fault}"),
        }
    }

    /// Re-applies the command now on screen, so the presenter matches a restored state.
    ///
    /// The scene is *not* rebuilt: the presenter's staged images come from the command stream,
    /// and a rollback or load only re-applies the current command. A rollback across a `scene`
    /// change therefore leaves the old backdrop. Stated rather than implied.
    fn refresh_presentation(&mut self) {
        if let Some(command) = self.timeline.current() {
            self.presenter.apply(command);
        }
    }

    /// Recompiles the project's screens after an edit, and swaps them into the running window.
    ///
    /// The story is left alone: hot reload is about the *screens*, which are a pure function of
    /// their arguments, so rebuilding one cannot lose game state — that is the payoff for
    /// `SCREENS.md §2`'s purity rule. A file that no longer parses keeps the last good screens.
    fn reload(&mut self) {
        let reloaded = self.screens.reload();
        if reloaded.errors > 0 {
            println!(
                "reload: {} file(s) with errors; keeping the last good screens",
                reloaded.errors
            );
            return;
        }
        self.overlays.relaid(
            &self.screens,
            self.size,
            self.presenter.text_mut(),
            FACE_NAME,
        );
        println!("reload: {} screen(s)", reloaded.screens);
    }

    /// Moves focus: in an open screen if one is up, otherwise in the menu.
    fn steer(&mut self, delta: isize) {
        if self.overlays.is_empty() {
            self.presenter.move_selection(delta);
        } else {
            self.overlays.move_focus(delta);
        }
    }

    /// Escape: close the top screen, or open `pause` when there is nothing to close.
    ///
    /// The "or open the menu" half of the host's `Cancel` binding. A project without a `pause`
    /// screen simply has no menu, which is the one-line script staying a one-line script.
    fn toggle_menu(&mut self) {
        if let Some(name) = self.overlays.close() {
            println!("screen close {name}");
            return;
        }
        if !self.screens.has("pause") {
            return;
        }
        if self.overlays.open(
            &self.screens,
            "pause",
            self.size,
            self.presenter.text_mut(),
            FACE_NAME,
        ) {
            println!("screen open pause");
        }
    }

    /// Activates the focused hotspot's action.
    fn activate(&mut self) {
        let Some(action) = self.overlays.focused().cloned() else {
            return;
        };
        println!("screen activate {action}");
        match action.name.as_str() {
            "open_screen" => {
                let Some(name) = action.first() else {
                    return;
                };
                if self.overlays.open(
                    &self.screens,
                    name,
                    self.size,
                    self.presenter.text_mut(),
                    FACE_NAME,
                ) {
                    println!("screen open {name}");
                } else {
                    println!("screen missing {name}");
                }
            }
            "close_screen" => {
                if let Some(name) = self.overlays.close() {
                    println!("screen close {name}");
                }
            }
            "quit" => {
                self.quit = true;
                println!("screen quit");
            }
            "quick_save" => self.save("quick"),
            "quick_load" => self.load("quick"),
            // `jump`, `set`, `play`, and the rest need the VM or `World` and are not wired
            // yet. Saying so beats a button that does nothing and looks broken.
            _ => {}
        }
    }
}

impl vela_host::App for Player {
    fn opened(&mut self, window: &vela_host::Window) {
        // The atlas is uploaded after the first draw list is built, because building is what
        // rasterises glyphs; an upload before it sends an empty image. Images are the other way
        // round — they are already pixels, and the presenter needs their texture ids *before*
        // it builds a frame, so they go up now, once.
        self.surface = Surface::new(window.raw(), self.size, None);
        let (presenter, surface) = (&mut self.presenter, &mut self.surface);
        if let Some(surface) = surface.as_mut() {
            presenter.upload_images(surface.renderer_mut());
        }
    }

    fn action(&mut self, action: vela_host::Action, _window: &vela_host::Window) {
        match action {
            vela_host::Action::Cancel => self.toggle_menu(),
            vela_host::Action::MenuUp => self.steer(-1),
            vela_host::Action::MenuDown => self.steer(1),
            // Backspace steps back one command, replaying from the nearest snapshot. A screen
            // or a menu on top takes the key first.
            vela_host::Action::Rollback if self.overlays.is_empty() => self.rollback(),
            // Advancing answers whatever is waiting: an open screen's focused control, then a
            // menu's highlighted choice, and only then the story.
            vela_host::Action::Advance | vela_host::Action::Confirm => {
                if !self.overlays.is_empty() {
                    self.activate();
                } else if let Some(index) = self.presenter.selection() {
                    self.choose(index);
                } else if !self.finished {
                    let mut sink = std::io::stdout();
                    self.advance(&mut sink);
                }
            }
            _ => {}
        }
    }

    fn should_close(&self) -> bool {
        self.quit
    }

    fn frame(&mut self, _window: &vela_host::Window) {
        // Polled on the idle tick the host drives, so an edit is picked up without input.
        if self.watcher.changed() {
            self.reload();
        }
        if !self.drew {
            self.drew = true;
            // Once, and only when tracing: whether the render loop runs at all is the
            // difference between "the window is up and idle" and "the app is stuck before its
            // first frame", and those need different fixes.
            if std::env::var_os("VELA_INPUT_TRACE").is_some() {
                println!("frame first");
            }
        }
        // Built first, and through the field rather than a `&mut self` helper: building
        // rasterises glyphs, and the atlas upload below has to see them. `draw_list` takes all
        // of `self`, which would collide with the surface borrow; these two borrows are of
        // disjoint fields, so they do not.
        let mut draw = DrawList::new();

        // Read the line out first: `line` borrows the presenter, and drawing the screen needs
        // it mutably. Copied into owned values so the two borrows do not overlap.
        let line = self
            .presenter
            .line()
            .map(|(speaker, text)| (speaker.map(str::to_string), text.to_string()));

        self.presenter.build_backdrop(&mut draw);
        if let Some((speaker, text)) = line.filter(|_| self.screens.has("dialogue")) {
            let dialogue = Screens::dialogue(speaker.as_deref(), &text);
            self.screens.draw(
                "dialogue",
                &dialogue,
                self.size,
                self.presenter.text_mut(),
                FACE_NAME,
                &mut draw,
            );
        } else {
            self.presenter.build_dialogue(&mut draw);
        }
        // The menu is the runtime's, not a screen's, so it is drawn whichever way the dialogue
        // was. Open screens go over it, with the focus highlight on top of those.
        self.presenter.build_menu(&mut draw);
        self.overlays
            .paint(self.presenter.text_mut(), FACE_NAME, &mut draw);

        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        surface
            .renderer_mut()
            .upload_atlas(self.presenter.text().atlas());
        match surface.render(&self.graph, &draw) {
            vela_render::Presented::Lost => surface.rebuild(),
            vela_render::Presented::Outdated => {
                let size = surface.size();
                surface.resize((size.0, size.1));
            }
            vela_render::Presented::Drawn => {}
        }
    }
}

/// The stages a frame runs.
fn graph() -> RenderGraph {
    let mut graph = RenderGraph::new();
    graph.push(Box::new(vela_render::ClearStage {
        color: vela_render::Style::default().background,
    }));
    graph.push(Box::new(vela_render::GeometryStage));
    graph
}

/// A window and its story, wired together.
///
/// The `Arc` is not shared: `winit` owns the window and `vela-render` borrows the handle for
/// the surface, which is the one place the two adapters have to agree on something.
pub fn run(player: Player, title: &str, size: (u32, u32)) -> Result<(), Error> {
    let config = vela_host::Config {
        title: title.to_string(),
        size,
        bindings: vela_host::Bindings::new(),
    };
    let mut player = player;
    vela_host::run(config, &mut player)
        .map_err(|error| Error::internal(format!("the window could not run: {error}")))
}
