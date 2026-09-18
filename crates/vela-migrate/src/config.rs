//! `options.rpy` → the report an engine's configuration needs (`TOOLING.md §8`).
//!
//! A configuration file is where a project says what *engine* it wants: its name and version, its
//! audio capabilities, the transitions between its screens, where saves live, and what the build
//! system should package. Almost none of it has a Vela counterpart, and the ones that do are kept
//! elsewhere — `config.name` is read as `vela.toml`'s `[project] name`.
//!
//! So this pass produces no file. What it produces is the *work list*: one entry per kind of knob,
//! naming every declaration of that kind, where the whole file used to be one entry saying "220 lines,
//! port by hand". The difference is the same one item 17 made for `screens.rpy` — a report a person
//! can act on has to say what is left, not how much.

use crate::report::Report;
use crate::rpy::{Kind, Node};

/// What a configuration file declares, grouped so that one kind of work is one entry.
const KINDS: &[(&[&str], &str)] = &[
    (
        &[
            "enter_transition",
            "exit_transition",
            "after_load_transition",
            "end_game_transition",
            "window_show_transition",
            "window_hide_transition",
        ],
        "a transition is animation, which `SCREENS.md §6` states as not yet built — `with` is in the \
         language and the fade it names is not (M13)",
    ),
    (
        &["has_music", "has_sound", "has_voice"],
        "a capability flag for a system Vela has not got: audio is M12.3's, and `has_voice` is one of \
         the knobs `SCREENS.md §8` names as never migrating",
    ),
    (
        &["window", "window_icon", "console"],
        "a knob for Ren'Py's own window: Vela draws its own, so there is no icon, no console, and no \
         say-window policy to configure",
    ),
    (
        &["version"],
        "Vela has no project version. `vela.toml` carries the name, the entry and the design size, \
         and a field nothing reads would be inert data — the bundle descriptor and an about screen \
         are where Ren'Py's version is read, and both are later work",
    ),
    (
        &["save_directory"],
        "saves live where the engine puts them, under the project's own name (`RUNTIME.md §5`), and \
         a project cannot choose the path",
    ),
    (
        &[
            "name",
            "itch_project",
            "classify",
            "classify_renpy",
            "documentation",
        ],
        "Ren'Py's build system: `vela build` packages by target and `vela.toml` is the descriptor \
         (`BUILD_AND_ASSETS.md §6`)",
    ),
    (
        &["text_cps", "afm_time"],
        "the settings store, which M12.2 owns: no spec describes one, and nothing in the engine keeps \
         a preference (`SCREENS.md §7`)",
    ),
];

/// Reports every declaration in a configuration file, or `false` when the file is not one.
///
/// The file is configuration when it declares a `config.`, `build.`, `gui.` or `preferences.` name —
/// which is the whole of what an `options.rpy` is, and what tells it from a story that happens to
/// declare nothing (the story test is a label, and this is the other half of that pair).
pub fn report(relative: &str, nodes: &[Node], report: &mut Report) -> bool {
    let mut groups: Vec<(&str, Vec<String>)> = Vec::new();
    let mut outside = Vec::new();
    let mut python = Vec::new();
    let mut found = false;

    for node in nodes {
        match &node.kind {
            Kind::Define { name, .. } | Kind::Default { name, .. } => {
                let name = name.clone();
                let Some(fate) = fate(&name) else {
                    continue;
                };
                found = true;
                // The one name that *is* translated: `vela.toml`'s `[project] name`, read by
                // `project::name_of`. Translating it and then reporting it would be two answers to
                // one question.
                if name == "config.name" {
                    continue;
                }
                match fate {
                    Fate::Kind(reason) => match groups.iter_mut().find(|(seen, _)| *seen == reason)
                    {
                        Some((_, names)) => names.push(name),
                        None => groups.push((reason, vec![name])),
                    },
                    Fate::Gui => outside.push(name),
                }
            }
            // `init python:`, `init -1400:` — where `build.classify`, `renpy.image_size` and the
            // rest of the engine's configuration *do* something rather than declare it. Named by its
            // own line, because "a Python block" is not a work item.
            _ if matches!(head(&node.text), "init" | "python" | "$") => {
                found = true;
                python.push(format!("`{}`", node.text.trim().trim_end_matches(':')));
            }
            _ => {}
        }
    }

    for (reason, mut names) in groups {
        names.sort();
        report.push(
            relative,
            1,
            &format!("{relative}: {}", names.join(", ")),
            reason,
        );
    }
    if !outside.is_empty() {
        outside.sort();
        report.push(
            relative,
            1,
            &format!("{relative}: {}", outside.join(", ")),
            "a `gui.*` variable outside `gui.rpy` is not in the theme — the theme is read from that \
             one file (`gui.rs`) — so a screen that reads it gets nothing. Move it into `gui.rpy` or \
             write it as a theme token by hand",
        );
    }
    if !python.is_empty() {
        python.sort();
        report.push(
            relative,
            1,
            &format!("{relative}: {}", python.join(", ")),
            "a block of Python: `build.classify` and `build.documentation` are Ren'Py's packaging \
             rules (`BUILD_AND_ASSETS.md §6`), and anything else in one is Python the engine cannot \
             run",
        );
    }
    found
}

/// The first word of a line.
fn head(text: &str) -> &str {
    text.split_whitespace().next().unwrap_or("")
}

/// What one declaration is.
enum Fate {
    /// A kind of knob, and the reason it has no counterpart.
    Kind(&'static str),
    /// A `gui.` variable outside `gui.rpy`.
    Gui,
}

/// The kind a name belongs to.
fn fate(name: &str) -> Option<Fate> {
    if name.starts_with("gui.") {
        return Some(Fate::Gui);
    }
    if !name.starts_with("config.")
        && !name.starts_with("build.")
        && !name.starts_with("preferences.")
    {
        return None;
    }
    let leaf = name.split('.').nth(1).unwrap_or_default();
    KINDS
        .iter()
        .find(|(leaves, _)| leaves.contains(&leaf))
        .map(|(_, reason)| Fate::Kind(reason))
}
