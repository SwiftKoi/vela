//! `vela run` — run a story.
//!
//! Headless, plus `--capture`, which renders a frame to a PNG without opening anything. There
//! is still no *window*: `vela-host`'s native backend lands later in M6. What exists now is
//! everything below it — commands become a draw list, and a draw list becomes pixels — so a
//! run can be looked at without a display, and the same path will back the window.

use std::io::Write;
use std::path::PathBuf;

use vela_compile::Session;

use crate::command::{Command, Error};
use crate::commands::check::{collect, module_path};
use crate::commands::ui::Screens;

/// The `vela run` command.
pub struct Run {
    base: PathBuf,
}

impl Run {
    /// Creates the command rooted at `base`.
    #[must_use]
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    /// Creates the command rooted at the process's working directory.
    #[must_use]
    pub fn at_current_dir() -> Self {
        let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::new(base)
    }
}

impl Command for Run {
    fn name(&self) -> &'static str {
        "run"
    }

    fn about(&self) -> &'static str {
        "run a story"
    }

    fn run(&self, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
        let target =
            positional(args).map_or_else(|| self.base.clone(), |path| self.base.join(path));
        let project = collect(&target)?;

        let entry = flag_value(args, "--start")
            .map(ToString::to_string)
            .or_else(|| {
                project
                    .manifest
                    .as_ref()
                    .map(|manifest| manifest.project.entry.clone())
            })
            .ok_or_else(|| {
                Error::usage(
                    "no entry point: pass `--start module.label`, or set `entry` in vela.toml",
                )
            })?;

        let Some((module_name, label)) = entry.rsplit_once('.') else {
            return Err(Error::usage(format!(
                "`{entry}` is not a valid entry point; expected `module.label`"
            )));
        };

        let mut session = load(&project);

        // A story with errors is not worth running: the module would either refuse to verify
        // or, worse, run a program nobody wrote.
        if session
            .diagnostics()
            .iter()
            .any(|d| d.severity() == vela_diag::Severity::Error)
        {
            return Err(Error::diagnostics(
                "the project has errors; run `vela check` to see them".to_string(),
            ));
        }

        let compiled = compile_entry(&mut session, module_name)?;
        let module = vela_bytecode::compile(&compiled, true);

        let diagnostics = vela_bytecode::verify(&module);
        if !diagnostics.is_empty() {
            for diagnostic in &diagnostics {
                let _ = out.write_all(vela_diag::render(diagnostic, session.sources()).as_bytes());
            }
            return Err(Error::internal(
                "the compiled module does not verify".to_string(),
            ));
        }

        // The screens the presenter may draw. Compiled here rather than in the compiler, for
        // the rank reason `commands::ui` records: the CLI is the lowest layer that can see both
        // a screen and the widget registry.
        let screens = Screens::load(&project);

        if !args.iter().any(|arg| arg == "--headless") && flag_value(args, "--capture").is_none() {
            let saves = saves_dir(&target);
            let schema = crate::commands::ui::schema(&project.files);
            return play(&module, label, &entry, args, out, screens, saves, schema);
        }

        let mut host = vela_vm::TakeFirst;
        let execution = vela_vm::run(&module, label, &mut host)
            .map_err(|fault| Error::internal(format!("{fault}")))?;

        if let Some(path) = flag_value(args, "--capture") {
            return capture(&execution.commands, path, args, out, &screens);
        }

        for command in &execution.commands {
            let _ = writeln!(out, "{command}");
        }
        Ok(())
    }
}

/// Where a project's saves live: a `saves/` directory beside it.
fn saves_dir(target: &std::path::Path) -> std::path::PathBuf {
    let base = if target.is_dir() {
        target
    } else {
        target.parent().unwrap_or(std::path::Path::new("."))
    };
    base.join("saves")
}

/// Opens a window and plays the story in it.
#[allow(clippy::too_many_arguments)]
fn play(
    module: &vela_bytecode::Module,
    label: &str,
    entry: &str,
    args: &[String],
    out: &mut dyn Write,
    screens: Screens,
    saves: std::path::PathBuf,
    schema: vela_replay::Schema,
) -> Result<(), Error> {
    let size = parse_size(args).unwrap_or((1280, 720));
    // Printed, not assumed. A windowed run is the one thing here that can appear on someone's
    // screen, and this line is how a harness — or a person reading a log — can tell where it
    // actually went rather than where it was meant to go.
    let display = std::env::var("DISPLAY").unwrap_or_else(|_| "<unset>".to_string());
    writeln!(out, "window display={display} size={}x{}", size.0, size.1)
        .map_err(|error| Error::internal(error.to_string()))?;

    let mut player =
        crate::commands::play::Player::new(module, label, size, screens, saves, schema)?;
    // The first command is presented before the window opens, so the first frame has
    // something to draw rather than appearing blank for a moment.
    player.begin(out);
    writeln!(out, "playing {entry}").map_err(|error| Error::internal(error.to_string()))?;
    out.flush()
        .map_err(|error| Error::internal(error.to_string()))?;
    crate::commands::play::run(player, entry, size)
}

/// Renders a frame of the story to a PNG.
///
/// The frame is taken *at a command*, not at the end: a visual novel's state is a point in
/// time, and the last command usually leaves a dialogue on screen. `--frame` picks which one;
/// the default is the last `Say`, because "what did the player see when they read the last
/// line" is the question a screenshot is usually asked.
fn capture(
    commands: &[vela_world::Command],
    path: &str,
    args: &[String],
    out: &mut dyn Write,
    screens: &Screens,
) -> Result<(), Error> {
    let size = parse_size(args).unwrap_or((1280, 720));
    // The face is compiled in. `BUILD_AND_ASSETS.md` makes fonts project assets with a
    // manifest and a subsetting step, none of which exists yet; until it does, an engine
    // that ships with a default face is better than one that cannot draw without a flag.
    let font = vela_text::Font::from_bytes(DEFAULT_FACE.to_vec(), 0)
        .ok_or_else(|| Error::internal("the bundled font failed to load".to_string()))?;

    let mut text = vela_text::TextEngine::new();
    text.add_font(FACE_NAME, font);

    let chosen = flag_value(args, "--frame")
        .and_then(|value| value.parse::<usize>().ok())
        .or_else(|| {
            commands
                .iter()
                .rposition(|command| matches!(command, vela_world::Command::Say { .. }))
        })
        .unwrap_or(commands.len().saturating_sub(1));

    let mut presenter = vela_render::Presenter::new(text, FACE_NAME, size);
    for command in commands.iter().take(chosen + 1) {
        presenter.apply(command);
    }

    let Some(mut capture) = vela_render::Capture::new(size.0, size.1) else {
        return Err(Error::internal(
            "no GPU adapter available: `--capture` renders with wgpu".to_string(),
        ));
    };
    let screen = named_screen(args, screens)?;

    // The draw list first, the atlas second. Building is what *rasterises* glyphs, so an
    // upload before it sends an empty image and every glyph samples nothing — which renders
    // as a perfectly good dialogue box with no text in it.
    let mut draw = vela_render::DrawList::new();
    build_frame(
        &mut presenter,
        screens,
        commands,
        chosen,
        size,
        screen,
        &mut draw,
    );
    capture
        .renderer_mut()
        .upload_atlas(presenter.text().atlas());

    let mut graph = vela_render::RenderGraph::new();
    graph.push(Box::new(vela_render::ClearStage {
        color: presenter.style().background,
    }));
    graph.push(Box::new(vela_render::GeometryStage));

    let path = std::path::Path::new(path);
    capture
        .save(&graph, &draw, path)
        .map_err(|error| Error::internal(format!("cannot write {}: {error}", path.display())))?;
    report_capture(out, path, screen, chosen, commands.len());
    Ok(())
}

/// The screen `--screen` names, checked against the project so a bad name is a usage error
/// rather than a picture of nothing. It renders instead of the current dialogue, which is how
/// a menu or a settings panel can be looked at without a window.
fn named_screen<'a>(args: &'a [String], screens: &Screens) -> Result<Option<&'a str>, Error> {
    let Some(name) = flag_value(args, "--screen") else {
        return Ok(None);
    };
    if screens.has(name) {
        Ok(Some(name))
    } else {
        Err(Error::usage(format!("there is no screen called `{name}`")))
    }
}

/// Says what was written, and at which point in the story.
fn report_capture(
    out: &mut dyn Write,
    path: &std::path::Path,
    screen: Option<&str>,
    chosen: usize,
    total: usize,
) {
    match screen {
        Some(name) => {
            let _ = writeln!(out, "captured {} (screen {name})", path.display());
        }
        None => {
            let _ = writeln!(
                out,
                "captured {} at command {}/{}",
                path.display(),
                chosen + 1,
                total
            );
        }
    }
}

/// Fills `draw` with the frame: the backdrop, and then either a named screen or the dialogue.
///
/// A named screen draws no focus highlight — focus is runtime state, and a still has none.
fn build_frame(
    presenter: &mut vela_render::Presenter,
    screens: &Screens,
    commands: &[vela_world::Command],
    chosen: usize,
    size: (u32, u32),
    screen: Option<&str>,
    draw: &mut vela_render::DrawList,
) {
    if let Some(name) = screen {
        presenter.build_backdrop(draw);
        screens.draw(
            name,
            &vela_ui::Args::new(),
            size,
            presenter.text_mut(),
            FACE_NAME,
            draw,
        );
        return;
    }

    // The line the presenter would draw, read back from the commands so a screen is called
    // with the same speaker and text. Taken with `take`, not slicing, so an empty command
    // stream draws the backdrop rather than panicking on `[..=0]`.
    let line = commands
        .iter()
        .take(chosen + 1)
        .rev()
        .find_map(|command| match command {
            vela_world::Command::Say { speaker, text, .. } => Some((speaker.clone(), text.clone())),
            _ => None,
        });

    presenter.build_backdrop(draw);
    if let Some((speaker, text)) = line.filter(|_| screens.has("dialogue")) {
        let dialogue = Screens::dialogue(speaker.as_deref(), &text);
        screens.draw(
            "dialogue",
            &dialogue,
            size,
            presenter.text_mut(),
            FACE_NAME,
            draw,
        );
    } else {
        presenter.build_dialogue(draw);
    }
    // The menu is the runtime's, not a screen's, so it is drawn whichever way the dialogue was.
    presenter.build_menu(draw);
}

/// The name the bundled face is registered under.
const FACE_NAME: &str = "sans";

/// The default face, compiled into the binary.
///
/// See `assets/fonts/README.md` for provenance and licence.
const DEFAULT_FACE: &[u8] = include_bytes!("../../../../assets/fonts/LiberationSans-Regular.ttf");

/// A `WxH` flag, if given.
fn parse_size(args: &[String]) -> Option<(u32, u32)> {
    let value = flag_value(args, "--size")?;
    let (width, height) = value.split_once('x')?;
    Some((width.parse().ok()?, height.parse().ok()?))
}

/// Loads every file into a session.
fn load(project: &crate::commands::check::Project) -> Session {
    let mut session = Session::new();
    for path in &project.files {
        if let Ok(text) = std::fs::read_to_string(path) {
            session.set_file(module_path(&project.source_root, path), text);
        }
    }
    session
}

/// The module that holds the entry point.
fn compile_entry(session: &mut Session, module_name: &str) -> Result<vela_mir::Module, Error> {
    let file = session
        .file_ids()
        .into_iter()
        .find(|file| {
            session
                .module_of(*file)
                .is_some_and(|name| name.as_str() == module_name)
        })
        .ok_or_else(|| Error::usage(format!("there is no module `{module_name}`")))?;

    Ok(session.mir(file).module.clone())
}

/// The value given to a `--flag value` argument, if the flag is present.
fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

/// The first positional argument, skipping flags *and* the values they take.
fn positional(args: &[String]) -> Option<&str> {
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--start"
            || arg == "--capture"
            || arg == "--size"
            || arg == "--frame"
            || arg == "--screen"
        {
            index += 2;
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        return Some(arg);
    }
    None
}
