//! Reading a Ren'Py project's `.rpy` files, and deciding what each one becomes.
//!
//! Split from `project.rs` by `REPO_LAYOUT.md §3.1`'s third recipe — reading files out of the work
//! of turning them into a project. The classifier is here because *this* is the file that knows a
//! `.rpy` file can hold story, configuration, the GUI's variables, or the screen language, and
//! which of those has a Vela counterpart.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use crate::Node;
use crate::Source;
use crate::assets::{Asset, media};
use crate::config;
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
    /// A file of `testsuite`/`testcase` blocks (`testcases.rs`): the module it becomes, and the
    /// nodes. The *translation* needs every story's menu options — a `click` is a `choose` only when
    /// the text names an option — so it happens after the whole project has been read.
    Tests {
        /// The module the file becomes.
        module: String,
        /// The file as it was read.
        nodes: Vec<Node>,
        /// The path as the report names it.
        relative: String,
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
    /// The tests, from `testcases.rpy` (`testcases.rs`).
    tests: Option<(String, Vec<Node>, String)>,
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
            Outcome::Tests {
                module,
                nodes,
                relative,
            } => {
                read.tests = Some((module, nodes, relative));
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

/// A file's bytes, or the error the caller reports.
fn read(path: &Path) -> Result<String, MigrateError> {
    std::fs::read_to_string(path).map_err(|source| MigrateError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// A file's path as the report names it, and the module it becomes.
///
/// The module is the path without its extension, which is how a project's directory layout becomes its
/// namespace: `chapters/forest.rpy` is `chapters/forest` (`LANGUAGE.md §6.1`).
fn naming(game: &Path, path: &Path) -> (String, String) {
    let relative = path
        .strip_prefix(game)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let module = relative.trim_end_matches(".rpy").to_string();
    (relative, module)
}

/// The entry a file of engine configuration gets, once, rather than per line.
///
/// A file that declares no label is `options.rpy`, `testcases.rpy` (before item 18 read it) or one of
/// the other files a project configures the engine in, and Vela expresses each of them differently.
/// One entry for the file is what keeps the report readable: a 220-line `options.rpy` reported line by
/// line is a report nobody reads.
fn report_engine_configuration(relative: &str, text: &str, report: &mut Report) {
    report.push(
        relative,
        1,
        &format!("{relative} ({} lines)", text.lines().count()),
        "engine configuration, the GUI's variables, or the screen language: Vela expresses these \
         differently, so this file is not translated — port what it configures by hand, or leave it \
         out and use Vela's own screens",
    );
}

/// Whether a file is Ren'Py's testcases rather than a story.
fn is_testcases(nodes: &[Node]) -> bool {
    nodes
        .iter()
        .any(|node| matches!(head(&node.text), "testsuite" | "testcase"))
}

/// The first word of a line.
fn head(text: &str) -> &str {
    text.split_whitespace().next().unwrap_or("")
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
    let text = read(path)?;
    let (relative, module) = naming(game, path);
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

    // A file of testcases becomes `test` items (`testcases.rs`), for the same reason the screens do:
    // a testcase declares no label either, so the story test below would report the whole file.
    if is_testcases(&nodes) {
        return Ok(Outcome::Tests {
            module,
            nodes,
            relative,
        });
    }

    // The engine's configuration, dispositioned one declaration at a time (`config.rs`): the names
    // that map are read, and the ones that do not are reported by kind rather than by file.
    if config::report(&relative, &nodes, report) {
        return Ok(Outcome::Nothing);
    }

    if is_engine_configuration(&nodes) {
        report_engine_configuration(&relative, &text, report);
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
pub(crate) fn files(read: &mut Scripts, report: &mut Report) -> Vec<Source> {
    let theme = read.theme.take();
    let screens = read.screens.take();
    let mut files = Vec::new();

    if let Some((module, nodes, relative)) = read.tests.take() {
        // The options come from the stories, which are read by now: a `click` is a `choose` when the
        // text names a menu option, and only the stories can say which texts those are.
        let menus: std::collections::BTreeSet<String> = read
            .stories
            .iter()
            .flat_map(|story| crate::testcases::menu_options(&story.nodes))
            .collect();
        if let Some(translated) = crate::testcases::lower(&relative, &nodes, &menus, report) {
            let source = formatted(
                Source {
                    path: format!("src/{module}.vela"),
                    text: translated.text,
                },
                &relative,
                report,
            );
            files.push(source);
        }
    }
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
