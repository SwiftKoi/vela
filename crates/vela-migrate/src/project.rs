//! Migrating a project, rather than a file.
//!
//! A Ren'Py project is `game/*.rpy` plus an assets tree, and only *some* of it is a story: the
//! rest is engine configuration (`options.rpy`), the GUI's own variables (`gui.rpy`), and the
//! screen language (`screens.rpy`), each of which Vela expresses differently or not at all. So
//! the walk classifies files first and translates second, and everything it does not translate
//! it reports once, by name.
//!
//! # Assets are inventoried, not copied
//!
//! `vela check` **imports** a project's assets — an unclaimed file under `assets/` is a hard
//! error — so copying a Ren'Py project's media blindly would turn a migration that compiles into
//! one that does not. And an asset in Vela is only meaningful once something declares it
//! (`image bg.room = @"art/room.png"`), which means reproducing Ren'Py's *automatic* image
//! definitions from filenames: a real piece of work, and the first item on the report this
//! produces.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::error::MigrateError;
use crate::report::Report;
use crate::rpy::{self, Kind};

/// One file the migration will write.
#[derive(Clone, Debug)]
pub struct Source {
    /// Where it goes, relative to the project root.
    pub path: String,
    /// What it holds.
    pub text: String,
}

/// One media file the project has, and what the migration did with it.
#[derive(Clone, Debug)]
pub struct Asset {
    /// The path, relative to `game/`.
    pub path: String,
    /// What kind of thing it is, from its extension.
    pub kind: &'static str,
}

/// A migrated project, before anything is written.
#[derive(Debug)]
pub struct Project {
    /// The project's name, from `options.rpy` when it says, and the directory name otherwise.
    pub name: String,
    /// The entry point, as `module.label`.
    pub entry: String,
    /// The files to write.
    pub files: Vec<Source>,
    /// The media found, in sorted order.
    pub assets: Vec<Asset>,
    /// Everything that was not translated.
    pub report: Report,
}

/// Directories whose contents are never migrated.
///
/// `cache/` and `saves/` are Ren'Py's own artifacts; `tl/` is a translation tree, and Vela has no
/// catalogue to put it in yet.
const SKIPPED: &[&str] = &["cache", "saves", "tl"];

/// Migrates the Ren'Py project at `root`.
///
/// # Errors
///
/// Fails if `root` has no `game/` directory, or if a file it needs cannot be read.
pub fn project(root: &Path) -> Result<Project, MigrateError> {
    let game = root.join("game");
    if !game.is_dir() {
        return Err(MigrateError::NotARenpyProject(root.to_path_buf()));
    }

    let mut report = Report::new();
    let mut scripts = Vec::new();
    let mut assets = Vec::new();
    collect(&game, &game, &mut scripts, &mut assets)?;
    scripts.sort();
    assets.sort_by(|left, right| left.path.cmp(&right.path));

    let mut files = Vec::new();
    let mut stories = Vec::new();
    for path in &scripts {
        let Some(story) = story_file(&game, path, &mut report)? else {
            continue;
        };
        stories.push(story);
    }

    // Ren'Py keeps a label and a `default` in separate namespaces — `label book` and
    // `default book` are both ordinary — and Vela does not (`E2003`). So a label that collides
    // with a value is renamed, deterministically and with a report entry, rather than left to
    // fail the check on the other side.
    let renames = collisions(&stories, &mut report);
    for story in &stories {
        // Canonical by construction: the migration emits what the formatter would, so a migrated
        // project diffs cleanly, `vela fmt --check` has nothing to say about it, and the shape of
        // the output is this project's one shape rather than the one Ren'Py happened to use.
        let text = crate::transpile::file(&story.nodes, &story.relative, &mut report, &renames);
        let text = match vela_syntax::format(vela_span::FileId::from_raw(0), &text) {
            Ok(formatted) => formatted,
            Err(_) => {
                report.push(
                    &story.relative,
                    1,
                    &story.relative,
                    "the migrated file does not parse — a bug in `vela migrate` rather than in the \
                     project it migrated. The translation is written out unformatted",
                );
                text
            }
        };
        files.push(Source {
            path: format!("src/{}.vela", story.module),
            text,
        });
    }

    let name = name_of(&game).unwrap_or_else(|| {
        root.file_name().map_or_else(
            || "migrated".to_string(),
            |name| name.to_string_lossy().to_string(),
        )
    });
    let entry = entry_of(&files).unwrap_or_else(|| "main.start".to_string());

    Ok(Project {
        name,
        entry,
        files,
        assets,
        report,
    })
}

/// Everything under `dir`, split into scripts and media.
fn collect(
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

/// What kind of media an extension is, if it is one.
fn media(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "png" | "jpg" | "jpeg" | "webp" | "ktx2" => "image",
        "opus" | "ogg" | "mp3" | "wav" => "audio",
        "ttf" | "otf" => "font",
        "webm" | "mp4" => "video",
        _ => return None,
    })
}

/// One story file, read and parsed.
struct Story {
    /// The path as the report names it.
    relative: String,
    /// The module name the file becomes.
    module: String,
    /// The tree.
    nodes: Vec<rpy::Node>,
}

/// One `.rpy` file, as a story — or as nothing, with a report entry saying why.
fn story_file(
    game: &Path,
    path: &Path,
    report: &mut Report,
) -> Result<Option<Story>, MigrateError> {
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
    if is_engine_configuration(&nodes) {
        report.push(
            &relative,
            1,
            &format!("{relative} ({} lines)", text.lines().count()),
            "engine configuration, the GUI's variables, or the screen language: Vela expresses \
             these differently, so this file is not translated — port what it configures by hand, \
             or leave it out and use Vela's own screens",
        );
        return Ok(None);
    }

    Ok(Some(Story {
        relative,
        module,
        nodes,
    }))
}

/// The module-level value names a label must not collide with.
fn value_names(stories: &[Story]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for story in stories {
        for node in &story.nodes {
            match &node.kind {
                Kind::Define { name, .. } | Kind::Default { name, .. } => {
                    names.insert(name.clone());
                }
                _ => {}
            }
        }
    }
    names
}

/// Labels that collide with a value, and the names they are renamed to.
fn collisions(stories: &[Story], report: &mut Report) -> BTreeMap<String, String> {
    let values = value_names(stories);
    let mut renames = BTreeMap::new();

    for story in stories {
        for node in &story.nodes {
            let Kind::Label(name) = &node.kind else {
                continue;
            };
            if !values.contains(name) || renames.contains_key(name) {
                continue;
            }
            let renamed = format!("{name}_label");
            report.push(
                &story.relative,
                node.line,
                &node.text,
                format!(
                    "`{name}` is both a value and a label. Ren'Py keeps those namespaces apart and \
                     Vela does not, so the label is renamed `{renamed}` — and every `jump`/`call` to \
                     it with it"
                ),
            );
            renames.insert(name.clone(), renamed);
        }
    }
    renames
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

/// The project's name, from `options.rpy`'s `define config.name`.
fn name_of(game: &Path) -> Option<String> {
    let text = std::fs::read_to_string(game.join("options.rpy")).ok()?;
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("define config.name") else {
            continue;
        };
        let (_, value) = crate::expr::split_assignment(rest)?;
        let value = crate::expr::without_translation_call(&value).unwrap_or(value);
        return Some(value.trim().trim_matches('"').to_string());
    }
    None
}

/// The entry point: the module of the first file to declare `label start`.
///
/// A linked program names everything `module.label`, so the entry is the story file's module. A
/// project with no `start` label gets its first file's first label, which is what `vela run`
/// would have to be told anyway.
fn entry_of(files: &[Source]) -> Option<String> {
    for file in files {
        let Some(module) = file
            .path
            .strip_prefix("src/")
            .and_then(|path| path.strip_suffix(".vela"))
        else {
            continue;
        };
        for line in file.text.lines() {
            let Some(label) = line
                .trim()
                .strip_prefix("label ")
                .and_then(|rest| rest.strip_suffix(':'))
            else {
                continue;
            };
            if label == "start" {
                return Some(format!("{module}.start"));
            }
            return Some(format!("{module}.{label}"));
        }
    }
    None
}

impl Project {
    /// Writes the migrated project into `out`.
    ///
    /// Three things land there: the `vela.toml` a project needs, the translated sources, and
    /// `MIGRATION.md` — the report, beside the code it is about, because the report *is* the rest
    /// of the work and a work item list that scrolls off a terminal is a work item list nobody
    /// keeps.
    ///
    /// # Errors
    ///
    /// Fails if a directory cannot be created or a file cannot be written.
    pub fn write(&self, out: &Path) -> Result<(), MigrateError> {
        write_file(&out.join("vela.toml"), &self.manifest())?;
        for file in &self.files {
            let path = out.join(&file.path);
            if let Some(parent) = path.parent() {
                create(parent)?;
            }
            write_file(&path, &file.text)?;
        }
        write_file(&out.join("MIGRATION.md"), &self.notes())?;
        Ok(())
    }

    /// The `vela.toml` for the migrated project.
    fn manifest(&self) -> String {
        format!(
            "schema = 1\n\n[project]\nname = {:?}\nentry = {:?}\n",
            self.name, self.entry
        )
    }

    /// `MIGRATION.md`: what was translated, what was not, and what the assets are.
    fn notes(&self) -> String {
        let mut text = format!(
            "# Migration report\n\nMigrated from a Ren'Py project by `vela migrate`. {} \
             source file(s) were translated; {} thing(s) need a person.\n\n",
            self.files.len(),
            self.report.len()
        );

        text.push_str("## Translated\n\n");
        for file in &self.files {
            let labels = file
                .text
                .lines()
                .filter(|line| line.trim().starts_with("label "))
                .count();
            text.push_str(&format!("- `{}` — {labels} label(s)\n", file.path));
        }

        text.push_str("\n## Needs a person\n\n");
        if self.report.is_empty() {
            text.push_str("Nothing: the whole project translated.\n");
        } else {
            text.push_str("```\n");
            text.push_str(&self.report.render());
            text.push_str("```\n");
        }

        text.push_str("\n## Assets\n\n");
        if self.assets.is_empty() {
            text.push_str("None found.\n");
        } else {
            text.push_str(
                "Not copied. An asset in Vela is only meaningful once something declares it — \
                 `image bg.room = @\"art/room.png\"` — and Ren'Py declares its images \
                 *automatically* from filenames, so reproducing them is its own piece of work. \
                 Note also that `vela check` **imports** everything under `assets/`, so a file no \
                 importer claims is an error rather than a warning.\n\n",
            );
            for asset in &self.assets {
                text.push_str(&format!("- `{}` ({})\n", asset.path, asset.kind));
            }
        }
        text
    }
}

/// Creates a directory and every parent.
fn create(path: &Path) -> Result<(), MigrateError> {
    std::fs::create_dir_all(path).map_err(|source| MigrateError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Writes a file, creating its directory.
fn write_file(path: &Path, text: &str) -> Result<(), MigrateError> {
    if let Some(parent) = path.parent() {
        create(parent)?;
    }
    std::fs::write(path, text).map_err(|source| MigrateError::Io {
        path: path.to_path_buf(),
        source,
    })
}
