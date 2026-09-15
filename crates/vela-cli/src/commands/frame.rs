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
    screens: &Screens,
    images: Vec<(String, u32, u32, Vec<u8>)>,
) -> Result<(), Error> {
    let size = parse_size(args).unwrap_or((1280, 720));
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
    for (name, width, height, rgba) in images {
        presenter.stage_image(name, width, height, rgba);
    }
    for command in commands.iter().take(chosen + 1) {
        presenter.apply(command);
    }

    let Some(mut capture) = vela_render::Capture::new(size.0, size.1) else {
        return Err(Error::internal(
            "no GPU adapter available: `--capture` renders with wgpu".to_string(),
        ));
    };
    let screen = named_screen(args, screens)?;

    // Images before the draw list, the atlas after it. Images are already pixels, so the
    // presenter needs their texture ids *while* it builds; glyphs are rasterised *by* building,
    // so their atlas cannot exist until it has.
    presenter.upload_images(capture.renderer_mut());

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
