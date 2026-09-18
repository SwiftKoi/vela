//! `vela run` — run a story, from a project or from a built bundle.
//!
//! Headless, plus `--capture`, which renders a frame to a PNG without opening anything. There
//! is still no *window* from a project: `vela-host`'s native backend lands later in M6. What
//! exists now is everything below it — commands become a draw list, and a draw list becomes
//! pixels — so a run can be looked at without a display, and the same path will back the window.
//!
//! A path naming a **built bundle** — a directory with `manifest.json` and no `vela.toml` — is
//! run by [`crate::commands::bundle_run`] instead, straight from the bytecode the build wrote.
//! That is the difference the whole M9 launcher turns on: a game can be given to someone who has
//! no compiler.

use std::io::Write;
use std::path::PathBuf;

use vela_compile::Session;

use crate::command::{Command, Error};
use crate::commands::bundle_run;
use crate::commands::check::{Project, collect, module_path};
use crate::commands::frame;
use crate::commands::ui::Screens;
use vela_ui::{Variant, Variants};

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

        // A bundle is not a project, and `collect` says so by insisting on `vela.toml`. Asking
        // which it is comes first, so a bundle never reaches the compiler.
        if bundle_run::is_bundle(&target) {
            return bundle_run::run(&target, args, out);
        }

        let project = collect(&target)?;
        // `entry` is `module.label`, and after linking that *is* the label's name: the whole
        // program is one module whose labels are qualified.
        let (module, entry, _) = compile_project(&project, args, out)?;

        // The screens the presenter may draw. Compiled here rather than in the compiler, for
        // the rank reason `commands::ui` records: the CLI is the lowest layer that can see both
        // a screen and the widget registry.
        let mut screens = Screens::load(&project);
        // A source run is a desktop run: there is no bundle and so no descriptor to ask, and the
        // platform the process is on *is* the answer (`SCREENS.md §2.6`). A bundle takes its answer
        // from `target.json` instead (`commands/target.rs`), which is what makes `--target web`
        // reach a condition.
        screens.set_variants(Variants::new().with(Variant::Pc));
        // And the frame this game is designed for, which is what `variant("small")` measures against
        // (`SCREENS.md §2.6`). A project that does not declare one is drawn against Vela's own
        // reference, which is the same answer `Frame::DEFAULT` gives.
        if let Some(manifest) = &project.manifest {
            let size = manifest.project.size;
            screens.set_design((size.width as f32, size.height as f32));
        }
        let images = stage_images(&project, out);

        if wants_a_window(args) {
            let saves = saves_dir(&target);
            let schema = crate::commands::ui::schema(&project.files);
            let title = title_of(project.manifest.as_ref(), &entry);
            return play(
                &module,
                &entry,
                &title,
                args,
                out,
                screens,
                saves,
                schema,
                images,
                vela_host::Bindings::new(),
            );
        }

        let mut host = vela_vm::TakeFirst;
        let execution = vela_vm::run(&module, &entry, &mut host)
            .map_err(|fault| Error::internal(format!("{fault}")))?;

        if let Some(dir) = flag_value(args, "--capture-dir") {
            return frame::capture_series(
                &execution.commands,
                dir,
                args,
                out,
                &mut screens,
                images,
            );
        }

        if let Some(path) = flag_value(args, "--capture") {
            return frame::capture(&execution.commands, path, args, out, &mut screens, images);
        }

        for command in &execution.commands {
            let _ = writeln!(out, "{command}");
        }
        Ok(())
    }
}

/// Compiles a project into the one module a run executes.
///
/// The whole front end in order — resolve the entry, check the project, **link every module**,
/// verify — and `run` is about *what to do with* the module it produces. Linking is why this is
/// the program and not just its entry file: a story split across files is one program by the time
/// anything runs it (`LANGUAGE.md §6`).
///
/// The sources come back with it because a second caller needs them: `vela debug` maps a
/// module's spans — which carry file ids — back to the files they came from, and the only map
/// that answers that is the one the compiler used.
///
/// # Errors
///
/// Fails if there is no entry point, the project has errors, the modules cannot be linked, the
/// entry label is not in the linked program, or the result does not verify.
pub(crate) fn compile_project(
    project: &Project,
    args: &[String],
    out: &mut dyn Write,
) -> Result<(vela_bytecode::Module, String, vela_span::SourceMap), Error> {
    let entry = flag_value(args, "--start")
        .map(ToString::to_string)
        .or_else(|| {
            project
                .manifest
                .as_ref()
                .map(|manifest| manifest.project.entry.clone())
        })
        .ok_or_else(|| {
            Error::usage("no entry point: pass `--start module.label`, or set `entry` in vela.toml")
        })?;
    if !entry.contains('.') {
        return Err(Error::usage(format!(
            "`{entry}` is not a valid entry point; expected `module.label`"
        )));
    }

    let mut session = load(project);

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

    let linked = session
        .linked()
        .map_err(|error| Error::diagnostics(error.to_string()))?;
    // Checked after linking rather than before: the entry names a label in the *linked* program,
    // and a module that is never reached is not a run.
    if !linked.labels.iter().any(|body| body.name.as_str() == entry) {
        return Err(Error::usage(format!(
            "there is no label `{entry}` in this project"
        )));
    }

    let module = vela_bytecode::compile(&linked, true);
    let diagnostics = vela_bytecode::verify(&module);
    if !diagnostics.is_empty() {
        for diagnostic in &diagnostics {
            let _ = out.write_all(vela_diag::render(diagnostic, session.sources()).as_bytes());
        }
        return Err(Error::internal(
            "the compiled module does not verify".to_string(),
        ));
    }
    Ok((module, entry, session.sources().clone()))
}

/// Whether a run should open a window rather than print its commands.
///
/// `--headless`, `--capture` and `--capture-dir` are all ways of asking for no window, and any one
/// of them decides it; a bundle run reaches the same question.
#[must_use]
pub(crate) fn wants_a_window(args: &[String]) -> bool {
    !args.iter().any(|arg| arg == "--headless")
        && flag_value(args, "--capture").is_none()
        && flag_value(args, "--capture-dir").is_none()
}

/// Every `image` declaration's picture, decoded, by the name a scene stages it under.
///
/// `scene bg.room` names an image; `image bg.room = @"art/room.png"` says what that name is a
/// picture of. Resolving one to the other is what this does, and it is the CLI's job because
/// it is the layer that can see both a project's sources and `vela-assets` — the renderer must
/// not know a file format, and the compiler must not know a filesystem.
///
/// A picture that cannot be read is **reported and skipped**, not fatal: the story still plays,
/// with the placeholder where the background would be, which is both more useful than refusing
/// to start and more honest than an empty screen. `vela check` is what makes it an error.
fn stage_images(project: &Project, out: &mut dyn Write) -> Vec<Picture> {
    let Some(root) = project.source_root.parent() else {
        return Vec::new();
    };
    let assets = root.join("assets");
    let mut images = Vec::new();

    for path in &project.files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let parsed = vela_syntax::parse(vela_span::FileId::from_raw(0), &text);
        for item in &parsed.program.items {
            let vela_syntax::Item::Image(declaration) = item else {
                continue;
            };
            let name = declaration.name.join(".");
            // An image is a picture or it is a colour. A colour is a *solid* — `image black =
            // 0x000000` — and it draws as a filled rectangle rather than as a texture, which is what
            // makes Ren'Py's built-in `black`, and any project's own flat backdrop, come across.
            if let vela_syntax::Expr::Int { value, .. } = &declaration.value
                && let Ok(rgb) = u32::try_from(*value)
            {
                let bytes = rgb.to_be_bytes();
                images.push(Picture::Solid(name, [bytes[1], bytes[2], bytes[3]]));
                continue;
            }

            let vela_syntax::Expr::Path { value, .. } = &declaration.value else {
                // Any other value names nothing to draw, and the checker says so. Skipped here
                // rather than reported twice.
                continue;
            };

            let file = assets.join(value);
            match std::fs::read(&file)
                .map_err(|error| error.to_string())
                .and_then(|bytes| vela_assets::decode(&bytes).map_err(|error| error.to_string()))
            {
                Ok(image) => {
                    images.push(Picture::File(name, image.width, image.height, image.rgba))
                }
                Err(error) => {
                    let _ = writeln!(out, "image {name}: {error}");
                }
            }
        }
    }
    images
}

/// One picture a project declares, as the presenter can take it.
///
/// Two kinds because there are two ways to name a picture: a file, which is decoded before it is
/// staged, and a colour, which has no pixels at all.
pub(crate) enum Picture {
    /// A picture: its name, and the pixels it decoded to.
    File(String, u32, u32, Vec<u8>),
    /// A colour, as `image black = 0x000000`.
    Solid(String, [u8; 3]),
}

/// Hands every picture to a presenter.
pub(crate) fn stage(presenter: &mut vela_render::Presenter, pictures: Vec<Picture>) {
    for picture in pictures {
        match picture {
            Picture::File(name, width, height, rgba) => {
                presenter.stage_image(name, width, height, rgba)
            }
            Picture::Solid(name, rgb) => presenter.stage_solid(name, rgb),
        }
    }
}

/// The size of every picture that *is* one, for a screen that has to lay one out.
///
/// A solid has no size of its own — it fills — so it is not in this table: a screen that draws one
/// measures the space it was given, which is the truth about it.
pub(crate) fn picture_sizes(pictures: &[Picture]) -> Vec<(String, u32, u32)> {
    pictures
        .iter()
        .filter_map(|picture| match picture {
            Picture::File(name, width, height, _) => Some((name.clone(), *width, *height)),
            Picture::Solid(..) => None,
        })
        .collect()
}

/// What a window is titled.
///
/// The project's name, which `vela.toml` declares. It used to be the entry point, so a title
/// bar read `main.start` — a label path, which is a fact about the source rather than about the
/// game. A project that does not name itself still gets a window, titled by where it starts.
#[must_use]
pub(crate) fn title_of(manifest: Option<&crate::manifest::Manifest>, entry: &str) -> String {
    manifest
        .and_then(|manifest| manifest.project.name.clone())
        .unwrap_or_else(|| entry.to_string())
}

/// Where a project's saves live: a `saves/` directory beside it.
pub(crate) fn saves_dir(target: &std::path::Path) -> std::path::PathBuf {
    let base = if target.is_dir() {
        target
    } else {
        target.parent().unwrap_or(std::path::Path::new("."))
    };
    base.join("saves")
}

/// Opens a window and plays the story in it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn play(
    module: &vela_bytecode::Module,
    label: &str,
    title: &str,
    args: &[String],
    out: &mut dyn Write,
    screens: Screens,
    saves: std::path::PathBuf,
    schema: vela_replay::Schema,
    images: Vec<Picture>,
    bindings: vela_host::Bindings,
) -> Result<(), Error> {
    let size = parse_size(args).unwrap_or((1280, 720));
    // Printed, not assumed. A windowed run is the one thing here that can appear on someone's
    // screen, and this line is how a harness — or a person reading a log — can tell where it
    // actually went rather than where it was meant to go.
    let display = std::env::var("DISPLAY").unwrap_or_else(|_| "<unset>".to_string());
    writeln!(out, "window display={display} size={}x{}", size.0, size.1)
        .map_err(|error| Error::internal(error.to_string()))?;

    let mut player =
        crate::commands::play::Player::new(module, label, size, screens, saves, schema, images)?;
    // The first command is presented before the window opens, so the first frame has
    // something to draw rather than appearing blank for a moment.
    player.begin(out);
    writeln!(out, "playing {title}").map_err(|error| Error::internal(error.to_string()))?;
    out.flush()
        .map_err(|error| Error::internal(error.to_string()))?;
    crate::commands::play::run(player, title, size, bindings)
}

/// A `WxH` flag, if given.
pub(crate) fn parse_size(args: &[String]) -> Option<(u32, u32)> {
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

/// The value given to a `--flag value` argument, if the flag is present.
pub(crate) fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

/// The first positional argument, skipping flags *and* the values they take.
pub(crate) fn positional(args: &[String]) -> Option<&str> {
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--start"
            || arg == "--capture"
            || arg == "--capture-dir"
            || arg == "--size"
            || arg == "--frame"
            || arg == "--screen"
            || arg == "--port"
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
