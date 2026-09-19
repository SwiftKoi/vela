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
use vela_replay::Timeline;
use vela_text::{Font, TextEngine};
use vela_world::Input;

pub(crate) mod waiting;

use crate::command::Error;
use crate::commands::play::waiting::waits_for_the_player;
use crate::commands::run::{Picture, stage};
use crate::commands::ui::{Screens, Watcher};
use vela_ui::Done;
use vela_ui::Stack;
use vela_ui::actions::Action as ScreenAction;

pub(crate) mod images;
mod saves;
pub(crate) mod settings;

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
    /// The pictures a screen may draw, once the platform has uploaded them (`SCREENS.md §3`).
    ///
    /// Empty until the window opens: a picture is a name *and* a texture, and the texture does not
    /// exist until the renderer has the bytes. A screen that draws one before then draws nothing, which
    /// is the same answer a screen with no picture at all gets.
    images: vela_ui::ImageTable,
    /// The name and pixel size of every staged picture, kept so the table can be built at that moment.
    image_sizes: Vec<(String, u32, u32)>,
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
        images: Vec<Picture>,
    ) -> Result<Self, Error> {
        let font = Font::from_bytes(FACE.to_vec(), 0)
            .ok_or_else(|| Error::internal("the bundled font failed to load".to_string()))?;
        let mut text = TextEngine::new();
        text.add_font(FACE_NAME, font);

        // Built before `screens` is moved: the watcher remembers the same files the screens
        // were compiled from.
        let watcher = Watcher::new(screens.paths());

        // The sizes are kept and the bytes handed to the presenter: a screen needs a picture's
        // *dimensions* to lay out and its *texture* to draw, and the texture only exists once the
        // window has uploaded the bytes (`SCREENS.md §3`).
        let image_sizes: Vec<(String, u32, u32)> = images
            .iter()
            .map(|picture| match picture {
                Picture::File(name, width, height, _) => (name.clone(), *width, *height),
                Picture::Solid(name, _) => (name.clone(), 0, 0),
            })
            .collect();

        let mut timeline =
            Timeline::start(module, label).map_err(|fault| Error::internal(format!("{fault}")))?;
        // The player's settings, from beside their saves (`RUNTIME.md §2.1`): a setting is the player's
        // rather than the playthrough's, so this is where one comes back — a load keeps what the session
        // already has, and a *start* is the moment the file is the only place they exist.
        if let Some(settings) = settings::read_settings(&saves) {
            timeline.preferences_mut().clone_from(&settings.preferences);
        }
        // And the screens are told before the first frame, so `setting("…")` is answerable from the start:
        // a dialogue box that reads a text speed and a settings screen that draws one both ask on the way in.
        let mut screens = screens;
        screens.set_preferences(timeline.world().preferences.clone());

        Ok(Self {
            timeline,
            module: module.clone(),
            presenter: {
                let mut presenter = Presenter::new(text, FACE_NAME, size);
                stage(&mut presenter, images);
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
            images: vela_ui::ImageTable::new(),
            image_sizes,
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
                    // A `scene`, a sprite, a transition and a music cue happen *now*: they are
                    // stage directions, not things to read. Only a line, a choice and a pause hold
                    // the story for the player — which is why one press at the start of a scene
                    // plus a line lands on the line, and why a `pause 2.0` advances itself.
                    if waits_for_the_player(&command) {
                        return;
                    }
                    step = self.timeline.answer(Input::Ack);
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
            self.screens.sets(),
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
            self.screens.sets(),
            "pause",
            &[],
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
        self.run(action);
    }

    /// Runs an action a screen asked for, however it asked.
    ///
    /// What the screen *stack* does with an action is `vela-ui`'s (`Stack::dispatch`), because a test
    /// that clicks drives a stack too and "what does `hide` mean" should have one answer. What is left
    /// here is what only a player has: the window, the saves, and the rollback history.
    fn run(&mut self, action: ScreenAction) {
        let done = self.overlays.dispatch(
            &action,
            self.screens.sets(),
            self.size,
            self.presenter.text_mut(),
            FACE_NAME,
        );
        match done {
            Done::Opened(name) => println!("screen open {name}"),
            Done::Missing(name) => println!("screen missing {name}"),
            Done::Closed(name) => println!("screen close {name}"),
            Done::Hidden(name) => println!("screen hide {name}"),
            Done::Set(name) => println!("screen set {name}"),
            Done::Stale(name) => println!("screen stale {name}"),
            Done::Nothing => {}
            Done::NotOurs => self.host_action(action),
        }
    }

    /// The actions a player has and a headless run does not: the VM's, the save system's, the window's.
    fn host_action(&mut self, action: ScreenAction) {
        // The player's settings come first, because they are the one vocabulary whose subject is the
        // *player's* state rather than the story's (`RUNTIME.md §2.1`) — and the file is rewritten at
        // once: a setting a player chose and a crash did not keep is one they have to choose again.
        if matches!(
            action.name.as_str(),
            vela_ui::actions::PREFERENCE | vela_ui::actions::TOGGLE_PREFERENCE
        ) {
            match vela_ui::settings::write(self.timeline.preferences_mut(), &action) {
                Some(name) => {
                    println!("screen setting {name}");
                    self.save_settings();
                    // And every screen is laid out again, so what the player just changed is what the screens
                    // read: a checkbox draws its own new state, and a text speed a dialogue box reads is the
                    // one they chose (`SCREENS.md §7.1`).
                    let preferences = self.timeline.world().preferences.clone();
                    self.screens.set_preferences(preferences.clone());
                    self.overlays.set_preferences(preferences);
                    self.overlays.relaid(
                        self.screens.sets(),
                        self.size,
                        self.presenter.text_mut(),
                        FACE_NAME,
                    );
                }
                // A bundle whose vocabulary moved on, or a value the world cannot hold. Saying so beats
                // a press that quietly did nothing.
                None => println!("screen setting refused: {action}"),
            }
            return;
        }
        match action.name.as_str() {
            "quit" => {
                self.quit = true;
                println!("screen quit");
            }
            "quick_save" => self.save("quick"),
            "quick_load" => self.load("quick"),
            "rollback" => self.rollback(),
            // The rest need the VM or `World` and are not wired yet — `SCREENS.md §7` says which, and
            // the registry carries the same answer. Saying so beats a button that quietly does
            // nothing, which is the failure that looks like the project's mistake.
            other => println!("screen action {other} (declared, not dispatched)"),
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

        // Now that every picture has a texture, the screens can be told where its name leads — and a
        // name only enters the table once it has one, so a screen never samples a texture that is not
        // there (`SCREENS.md §3`).
        let images = images::table(&self.image_sizes, |name| self.presenter.texture_of(name));
        self.screens.set_images(images.clone());
        self.images = images;
    }

    fn action(&mut self, action: vela_host::Action, _window: &vela_host::Window) {
        // A screen may answer this itself, and it answers first: a modal screen that binds `cancel` is
        // what keeps Escape from dismissing it, which is the whole point of the binding
        // (`SCREENS.md §2.3`).
        if let Some(bound) = self.overlays.key_action(action.as_str()).cloned() {
            println!("screen key {} {bound}", action.as_str());
            self.run(bound);
            return;
        }
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
        self.overlays.paint(
            self.presenter.text_mut(),
            FACE_NAME,
            &mut draw,
            &self.images,
        );

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
///
/// `bindings` are the target's input defaults (`BUILD_AND_ASSETS.md §4`): a bundle names a
/// profile and the window installs it, rather than every run getting the built-in table.
pub fn run(
    player: Player,
    title: &str,
    size: (u32, u32),
    bindings: vela_host::Bindings,
) -> Result<(), Error> {
    let config = vela_host::Config {
        title: title.to_string(),
        size,
        bindings,
    };
    let mut player = player;
    vela_host::run(config, &mut player)
        .map_err(|error| Error::internal(format!("the window could not run: {error}")))
}
