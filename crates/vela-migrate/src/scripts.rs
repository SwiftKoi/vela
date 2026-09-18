//! Reading a Ren'Py project's `.rpy` files, and deciding what each one becomes.
//!
//! Split from `project.rs` by `REPO_LAYOUT.md §3.1`'s third recipe — reading files out of the work
//! of turning them into a project. The classifier is here because *this* is the file that knows a
//! `.rpy` file can hold story, configuration, the GUI's variables, or the screen language, and
//! which of those has a Vela counterpart.

use std::path::{Path, PathBuf};

use crate::Source;
use crate::assets::{Asset, media};
use crate::error::MigrateError;
use crate::gui;
use crate::report::Report;
use crate::rpy::{self, Kind};

/// Directories whose contents are never migrated.
///
/// `cache/` and `saves/` are Ren'Py's own artifacts; `tl/` is a translation tree, and Vela has no
/// catalogue to put it in yet.
const SKIPPED: &[&str] = &["cache", "saves", "tl"];

/// What one `.rpy` file became.
enum Outcome {
    /// A file with story in it.
    Story(Story),
    /// A file that becomes a Vela source of its own: the GUI's theme and styles (`gui.rs`), with
    /// the design frame it declared.
    Theme(Source, Option<(u32, u32)>),
    /// A file that is not translated, with its report entry already written.
    Nothing,
}

/// What reading every script produced.
#[derive(Default)]
pub(crate) struct Scripts {
    /// The files with story in them.
    pub(crate) stories: Vec<Story>,
    /// The sources a file becomes on its own: the GUI's theme and styles.
    pub(crate) sources: Vec<Source>,
    /// The design frame, from `gui.init(width, height)` (`SCREENS.md §2.6`).
    pub(crate) design: Option<(u32, u32)>,
}

/// Reads every script.
pub(crate) fn read_scripts(
    game: &Path,
    scripts: &[PathBuf],
    report: &mut Report,
) -> Result<Scripts, MigrateError> {
    let mut read = Scripts::default();
    for path in scripts {
        match script_file(game, path, report)? {
            Outcome::Story(story) => read.stories.push(story),
            Outcome::Theme(source, size) => {
                read.sources.push(source);
                // `gui.init(1280, 720)` is where a Ren'Py project declares the frame its screens
                // were laid out against, and `vela.toml` is where Vela keeps it (`SCREENS.md §2.6`).
                read.design = read.design.or(size);
            }
            Outcome::Nothing => {}
        }
    }
    Ok(read)
}

/// Everything under `dir`, split into scripts and media.
pub(crate) fn collect(
    base: &Path,
    dir: &Path,
    scripts: &mut Vec<PathBuf>,
    assets: &mut Vec<Asset>,
) -> Result<(), MigrateError> {
    let entries = std::fs::read_dir(dir).map_err(|source| MigrateError::Io {
        path: dir.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| MigrateError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            if SKIPPED.contains(&name.as_str()) {
                continue;
            }
            collect(base, &path, scripts, assets)?;
            continue;
        }

        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let relative = path
            .strip_prefix(base)
            .map(|relative| relative.to_string_lossy().replace('\\', "/"))
            .unwrap_or(name);

        match extension.as_str() {
            "rpy" => scripts.push(path),
            "rpyc" | "rpyb" | "txt" => {}
            other => {
                if let Some(kind) = media(other) {
                    assets.push(Asset {
                        path: relative,
                        kind,
                    });
                    continue;
                }
                // A file nothing recognises is not reported: it is not the migration's business,
                // and `vela check` is what decides whether the *result* can use it.
                let _ = other;
            }
        }
    }
    Ok(())
}

/// One story file, read and parsed.
pub(crate) struct Story {
    /// The path as the report names it.
    pub(crate) relative: String,
    /// The module name the file becomes.
    pub(crate) module: String,
    /// The tree.
    pub(crate) nodes: Vec<rpy::Node>,
}

/// One `.rpy` file, as a story, as a source of its own, or as nothing with a report entry.
fn script_file(game: &Path, path: &Path, report: &mut Report) -> Result<Outcome, MigrateError> {
    let text = std::fs::read_to_string(path).map_err(|source| MigrateError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let relative = path
        .strip_prefix(game)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let module = relative.trim_end_matches(".rpy").to_string();

    let nodes = rpy::read(&text);

    // The GUI's variables become the project's theme (`SCREENS.md §5`, `§2.6`). What a screen says
    // about them is what decides which of them are live, so the screens are read here — the one
    // place in the migration where a file's meaning depends on another file's text.
    // One theme per project, and Ren'Py names the file it usually lives in: `gui.rpy`. A project
    // that declares GUI variables somewhere else gets the ordinary "not translated" entry below,
    // rather than a second theme that the engine would quietly ignore.
    let screens = std::fs::read_to_string(game.join("screens.rpy")).unwrap_or_default();
    let is_gui = relative.rsplit('/').next() == Some("gui.rpy");
    let skin = is_gui
        .then(|| gui::skin(&relative, &nodes, &screens, report))
        .flatten();
    if let Some(skin) = skin {
        let source = Source {
            path: format!("src/{module}.vela"),
            text: skin.source,
        };
        return Ok(Outcome::Theme(
            formatted(source, &relative, report),
            skin.design,
        ));
    }

    if is_engine_configuration(&nodes) {
        report.push(
            &relative,
            1,
            &format!("{relative} ({} lines)", text.lines().count()),
            "engine configuration, the GUI's variables, or the screen language: Vela expresses \
             these differently, so this file is not translated — port what it configures by hand, \
             or leave it out and use Vela's own screens",
        );
        return Ok(Outcome::Nothing);
    }

    Ok(Outcome::Story(Story {
        relative,
        module,
        nodes,
    }))
}

/// The formatter's output for a generated source, or the source with a report entry saying it did
/// not parse — a bug in `vela migrate` rather than in the project it migrated.
fn formatted(source: Source, relative: &str, report: &mut Report) -> Source {
    match vela_syntax::format(vela_span::FileId::from_raw(0), &source.text) {
        Ok(text) => Source { text, ..source },
        Err(_) => {
            report.push(
                relative,
                1,
                relative,
                "the migrated file does not parse — a bug in `vela migrate` rather than in the \
                 project it migrated. The translation is written out unformatted",
            );
            source
        }
    }
}

/// Whether a file is configuration rather than story.
///
/// A *story* file is one that declares a label. That is the whole test, and it is the right one:
/// Ren'Py's story files are exactly the files with labels in them, and every other `.rpy` in a
/// project is `options`/`gui`/`screens` — the engine's configuration, which has no Vela
/// counterpart to translate into.
fn is_engine_configuration(nodes: &[rpy::Node]) -> bool {
    !nodes.iter().any(|node| matches!(node.kind, Kind::Label(_)))
}
