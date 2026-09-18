//! Reading a Ren'Py project's `.rpy` files, and deciding what each one becomes.
//!
//! Split from `project.rs` by `REPO_LAYOUT.md §3.1`'s third recipe — reading files out of the work
//! of turning them into a project. The classifier is here because *this* is the file that knows a
//! `.rpy` file can hold story, configuration, the GUI's variables, or the screen language, and
//! which of those has a Vela counterpart.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use crate::Source;
use crate::assets::{Asset, media};
use crate::error::MigrateError;
use crate::gui::{self, Names};
use crate::report::Report;
use crate::rpy::{self, Kind};
use crate::screens;

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
    /// A file of screens and styles (`screens.rs`): the module it belongs in, its text, and the styles
    /// a screen took the name of — which the merge has to apply to the theme as well, since the two end
    /// up in one file.
    Screens {
        /// The module the file becomes.
        module: String,
        /// The screens and styles, as Vela.
        text: String,
        /// `style X` → the name it kept.
        renames: BTreeMap<String, String>,
    },
    /// A file that is not translated, with its report entry already written.
    Nothing,
}

/// What reading every script produced.
#[derive(Default)]
pub(crate) struct Scripts {
    /// The files with story in them.
    pub(crate) stories: Vec<Story>,
    /// The GUI's theme, from `gui.rpy` (`gui.rs`), and the names it declares.
    theme: Option<(String, Names)>,
    /// The screens and their styles, from `screens.rpy` (`screens.rs`).
    screens: Option<(String, String, BTreeMap<String, String>)>,
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
    // `gui.rpy` is read before `screens.rpy`, which is the order the paths sort in and the order the
    // passes need: a screen refers to the theme's styles and tokens by the names the theme chose.
    let mut names = Names::default();
    for path in scripts {
        match script_file(game, path, &mut names, report)? {
            Outcome::Story(story) => read.stories.push(story),
            Outcome::Screens {
                module,
                text,
                renames,
            } => {
                read.screens = Some((module, text, renames));
            }
            Outcome::Theme(source, size) => {
                read.theme = Some((source.text, names.clone()));
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
fn script_file(
    game: &Path,
    path: &Path,
    names: &mut Names,
    report: &mut Report,
) -> Result<Outcome, MigrateError> {
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
        *names = skin.names;
        let source = Source {
            path: format!("src/{module}.vela"),
            text: skin.source,
        };
        return Ok(Outcome::Theme(
            formatted(source, &relative, report),
            skin.design,
        ));
    }

    // A file of screens becomes a Vela screen module (`screens.rs`). Checked before the
    // engine-configuration test below, because a screen file declares no label and would otherwise
    // be reported whole.
    if let Some(lowered) = screens::lower(&relative, &nodes, names, report) {
        let source = formatted(
            Source {
                path: String::new(),
                text: lowered.text,
            },
            &relative,
            report,
        );
        return Ok(Outcome::Screens {
            module,
            text: source.text,
            renames: lowered.renames,
        });
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
    let mut queue: VecDeque<&rpy::Node> = nodes.iter().collect();
    while let Some(node) = queue.pop_front() {
        if matches!(node.kind, Kind::Label(_)) {
            return false;
        }
        queue.extend(node.children.iter());
    }
    true
}

/// The files a project's reading produced, in the order they are written.
pub(crate) fn files(read: &mut Scripts) -> Vec<Source> {
    let theme = read.theme.take();
    let screens = read.screens.take();
    let mut files = Vec::new();
    match (theme, screens) {
        // One module for both, the theme first: a style resolves within the file that declares it,
        // so a screen that reads `theme.accent` has to be in the file that holds it.
        (Some((theme, _names)), Some((module, screens, renames))) => {
            let theme = unshadow(theme, &renames);
            files.push(Source {
                path: format!("src/{module}.vela"),
                text: format!("{theme}\n{screens}"),
            });
        }
        (Some((theme, _)), None) => files.push(Source {
            path: "src/gui.vela".to_string(),
            text: theme,
        }),
        (None, Some((module, screens, _))) => files.push(Source {
            path: format!("src/{module}.vela"),
            text: screens,
        }),
        (None, None) => {}
    }
    files
}

/// The theme's text, with the styles a screen took the name of renamed to match.
///
/// The rename itself is the screens pass's decision — it is the pass that knows both names (see
/// `screens::collisions`) — and this is where the *other* half of it lands: Vela keeps screens and
/// styles in one namespace, so the theme's `style main_menu` has to become `style main_menu_style`
/// the moment a screen called `main_menu` exists beside it. Renaming by line and not by substring,
/// because `style main_menu_frame:` contains `style main_menu` and is a different style.
fn unshadow(theme: String, renames: &BTreeMap<String, String>) -> String {
    if renames.is_empty() {
        return theme;
    }
    let mut out = String::new();
    for line in theme.lines() {
        let header = line.strip_suffix(':').map(str::trim_end);
        match header.and_then(|header| header.strip_prefix("style ")) {
            Some(name) if renames.contains_key(name) => {
                let renamed = &renames[name];
                out.push_str(
                    &line.replace(&format!("style {name}:"), &format!("style {renamed}:")),
                );
            }
            _ => out.push_str(line),
        }
        out.push('\n');
    }
    out
}
