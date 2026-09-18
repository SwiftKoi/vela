//! Rendering a frame of a story to a PNG, with no window and no display.
//!
//! `vela run --capture` renders one command's worth of presentation into a texture and writes it
//! out. Separated from `commands::run` because it is the *presentation* half of a run — the
//! command stream is what a headless run is about, and this is what turns a point in that stream
//! into pixels. Both a project run and a bundle run go through here, so a screenshot of a built
//! bundle and a screenshot of the source look the same because they are the same code.
//!
//! The face is compiled in: `BUILD_AND_ASSETS.md §3.2` makes fonts project assets with a subset,
//! and until that exists an engine that ships with a default face is better than one that cannot
//! draw without a flag.

use std::io::Write;

use crate::command::Error;
use crate::commands::run::{flag_value, parse_size};
use crate::commands::ui::Screens;

/// The name the bundled face is registered under.
pub(crate) const FACE_NAME: &str = "sans";

/// The default face, compiled into the binary.
///
/// See `assets/fonts/README.md` for provenance and licence.
pub(crate) const DEFAULT_FACE: &[u8] =
    include_bytes!("../../../../assets/fonts/LiberationSans-Regular.ttf");

/// Renders a frame of the story to a PNG.
///
/// The frame is taken *at a command*, not at the end: a visual novel's state is a point in
/// time, and the last command usually leaves a dialogue on screen. `--frame` picks which one;
/// the default is the last `Say`, because "what did the player see when they read the last
/// line" is the question a screenshot is usually asked.
///
/// # Errors
///
/// Fails if no GPU adapter is available, or the file cannot be written.
pub(crate) fn capture(
    commands: &[vela_world::Command],
    path: &str,
    args: &[String],
    out: &mut dyn Write,
    screens: &mut Screens,
    images: Vec<(String, u32, u32, Vec<u8>)>,
) -> Result<(), Error> {
    let chosen = flag_value(args, "--frame")
        .and_then(|value| value.parse::<usize>().ok())
        .or_else(|| {
            commands
                .iter()
                .rposition(|command| matches!(command, vela_world::Command::Say { .. }))
        })
        .unwrap_or(commands.len().saturating_sub(1));

    let mut film = Film::new(args, screens, images)?;
    let _ = film.write(commands, chosen, std::path::Path::new(path))?;
    report_capture(
        out,
        std::path::Path::new(path),
        film.screen.as_deref(),
        chosen,
        commands.len(),
    );
    Ok(())
}

/// Renders a story's **slides** — one frame per command that changes what is on screen.
///
/// A single screenshot answers "what does this look like"; a series answers "does this *work*",
/// which is the question a visual novel keeps failing: a transition that draws nothing, a frame
/// that is a bare background because the scene arrived one command late, a speaker name that was
/// never resolved. Each of those is invisible in one frame and obvious in twenty.
///
/// The frames are numbered in the order they were taken, and each one is named for the command it
/// was taken at, so a reader can put an image beside the line that produced it.
///
/// # Errors
///
/// Fails if no GPU adapter is available, or a file cannot be written.
pub(crate) fn capture_series(
    commands: &[vela_world::Command],
    dir: &str,
    args: &[String],
    out: &mut dyn Write,
    screens: &mut Screens,
    images: Vec<(String, u32, u32, Vec<u8>)>,
) -> Result<(), Error> {
    let dir = std::path::Path::new(dir);
    std::fs::create_dir_all(dir)
        .map_err(|error| Error::internal(format!("cannot create {}: {error}", dir.display())))?;

    let mut film = Film::new(args, screens, images)?;
    let steps = presenting(commands);
    let mut blank = 0usize;
    let mut still = 0usize;
    let mut previous: Option<Vec<u8>> = None;

    for (number, index) in steps.iter().enumerate() {
        let path = dir.join(format!("{:04}.png", number + 1));
        let pixels = film.write(commands, *index, &path)?;
        // What a person sees in a second and a PNG reports to nobody: a frame that is the bare
        // backdrop, and a frame that says exactly what the one before it said. Both are bugs a
        // single screenshot — and a whole suite of them, read one at a time — will not show.
        let empty = is_blank(&pixels);
        let unchanged = previous.as_ref() == Some(&pixels);
        blank += usize::from(empty);
        still += usize::from(unchanged);
        previous = Some(pixels);

        let mut notes = Vec::new();
        if empty {
            notes.push("nothing on screen but the backdrop");
        }
        if unchanged {
            notes.push("identical to the frame before it");
        }
        let note = match notes.is_empty() {
            true => String::new(),
            false => format!("  ⚠ {}", notes.join("; ")),
        };
        let _ = writeln!(
            out,
            "wrote {} — command {}/{}: {}{note}",
            path.display(),
            index + 1,
            commands.len(),
            commands[*index]
        );
    }
    let _ = writeln!(
        out,
        "{} frame(s) in {}: {blank} with nothing on screen, {still} unchanged from the one before",
        steps.len(),
        dir.display()
    );
    Ok(())
}

/// Whether a frame draws nothing at all — the bare backdrop, whatever colour the backdrop is.
///
/// Measured against the frame's *own* first pixel rather than against the clear colour, because what
/// the readback says a clear colour looks like depends on the target's colour space, and the question
/// here is not "which colour" but "did anything draw". A tenth of a percent, so the antialiased edge
/// of a nearly-empty frame is not content, and a dark scene is still a scene.
fn is_blank(pixels: &[u8]) -> bool {
    let Some(reference) = pixels.first_chunk::<4>() else {
        return true;
    };
    let total = pixels.len() / 4;
    let differing = pixels
        .chunks_exact(4)
        .filter(|pixel| {
            let delta = u32::from(pixel[0].abs_diff(reference[0]))
                + u32::from(pixel[1].abs_diff(reference[1]))
                + u32::from(pixel[2].abs_diff(reference[2]));
            delta > 12
        })
        .count();
    differing * 1000 < total
}

/// The commands worth a frame: the ones that change the picture, and the first command.
///
/// `apply` ignores a transition, an audio cue and a pause, so a frame taken at one of those is the
/// frame before it — a hundred images that differ in nothing. What changes the picture is what a
/// player *sees* change: a line of dialogue, a scene, a sprite, a menu.
fn presenting(commands: &[vela_world::Command]) -> Vec<usize> {
    let changes = |command: &vela_world::Command| {
        matches!(
            command,
            vela_world::Command::Say { .. }
                | vela_world::Command::Stage { .. }
                | vela_world::Command::Menu { .. }
        )
    };
    let mut steps = vec![0];
    steps.extend(
        commands
            .iter()
            .enumerate()
            .filter(|(_, command)| changes(command))
            .map(|(index, _)| index),
    );
    steps.dedup();
    steps
}

/// One rendering context, reused for every frame of a series.
///
/// A film strip is many frames of one story, and everything but the drawing is the same for all of
/// them: the size, the text engine, the pictures the platform uploaded, the renderer. Building it
/// once is also what makes the frames *comparable* — two captures that each built their own atlas
/// would differ in ways that have nothing to do with the story.
struct Film<'a> {
    /// The frame size, from `--size`.
    size: (u32, u32),
    /// The presentation state, at the command being written.
    presenter: vela_render::Presenter,
    /// The offscreen renderer every frame goes through.
    capture: vela_render::Capture,
    /// The project's screens, told where the pictures are.
    screens: &'a mut Screens,
    /// The screen `--screen` names, if one does.
    screen: Option<String>,
}

impl<'a> Film<'a> {
    /// Builds a context: size, font, pictures, and a renderer.
    fn new(
        args: &[String],
        screens: &'a mut Screens,
        images: Vec<(String, u32, u32, Vec<u8>)>,
    ) -> Result<Self, Error> {
        let size = parse_size(args).unwrap_or((1280, 720));
        let font = vela_text::Font::from_bytes(DEFAULT_FACE.to_vec(), 0)
            .ok_or_else(|| Error::internal("the bundled font failed to load".to_string()))?;
        let mut text = vela_text::TextEngine::new();
        text.add_font(FACE_NAME, font);

        let mut presenter = vela_render::Presenter::new(text, FACE_NAME, size);
        // The sizes are kept and the bytes handed over: a screen needs a picture's dimensions to lay
        // out and its texture to draw, and the texture only exists after the upload below.
        let sizes = stage_pictures(&mut presenter, images);

        let Some(mut capture) = vela_render::Capture::new(size.0, size.1) else {
            return Err(Error::internal(
                "no GPU adapter available: `--capture` renders with wgpu".to_string(),
            ));
        };
        let screen = named_screen(args, screens)?.map(str::to_string);

        presenter.upload_images(capture.renderer_mut());
        install_pictures(&sizes, &presenter, screens);
        Ok(Self {
            size,
            presenter,
            capture,
            screens,
            screen,
        })
    }

    /// Writes the frame the story is in at `chosen`.
    fn write(
        &mut self,
        commands: &[vela_world::Command],
        chosen: usize,
        path: &std::path::Path,
    ) -> Result<Vec<u8>, Error> {
        // Replayed from the story's first command for every frame, so a frame is a function of the
        // commands before it rather than of the frame before it — which is what makes a series a
        // series. The pictures and the glyph atlas are the platform's, and survive the reset.
        self.presenter.reset();
        for command in commands.iter().take(chosen + 1) {
            self.presenter.apply(command);
        }

        // The draw list first, the atlas second. Building is what *rasterises* glyphs, so an
        // upload before it sends an empty image and every glyph samples nothing — which renders
        // as a perfectly good dialogue box with no text in it.
        let mut draw = vela_render::DrawList::new();
        build_frame(
            &mut self.presenter,
            self.screens,
            commands,
            chosen,
            self.size,
            self.screen.as_deref(),
            &mut draw,
        );
        self.capture
            .renderer_mut()
            .upload_atlas(self.presenter.text().atlas());

        let mut graph = vela_render::RenderGraph::new();
        graph.push(Box::new(vela_render::ClearStage {
            color: self.presenter.style().background,
        }));
        graph.push(Box::new(vela_render::GeometryStage));
        self.capture
            .save(&graph, &draw, path)
            .map_err(|error| Error::internal(format!("cannot write {}: {error}", path.display())))
    }
}

/// Hands the pictures to the presenter, and keeps what a screen will need to lay one out.
///
/// A picture is two facts — how big it is, and which texture it became — and only the second is the
/// presenter's. The sizes come back rather than being asked for later, because the bytes are consumed
/// here and a screen measures before anything is uploaded.
fn stage_pictures(
    presenter: &mut vela_render::Presenter,
    images: Vec<(String, u32, u32, Vec<u8>)>,
) -> Vec<(String, u32, u32)> {
    let sizes = images
        .iter()
        .map(|(name, width, height, _)| (name.clone(), *width, *height))
        .collect();
    for (name, width, height, rgba) in images {
        presenter.stage_image(name, width, height, rgba);
    }
    sizes
}

/// Tells the screens where their pictures are, now that the platform has uploaded them.
///
/// The same table the windowed player builds, from the same two sources — so a capture and a window
/// agree about what a screen draws, which is the only reason a screenshot is worth taking.
fn install_pictures(
    sizes: &[(String, u32, u32)],
    presenter: &vela_render::Presenter,
    screens: &mut Screens,
) {
    screens.set_images(crate::commands::play::images::table(sizes, |name| {
        presenter.texture_of(name)
    }));
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
