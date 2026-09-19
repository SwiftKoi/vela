//! Ren'Py's own globals as a migrated project reads them (`SCREENS.md §7`).
//!
//! A condition in a Ren'Py screen reads the *engine*: whether the main menu is up, whether a replay is
//! running, the dialogue log, whether this build has music. None of those is a thing a Vela project can
//! ask yet — they are the host systems §7 assigns to M12.2 and M12.3 — and a migrated screen that kept
//! the name would read nothing, silently, because a screen is where the checker does not resolve names.
//! So each name is answered *here*, once, with the value the engine would have given it today (the main
//! menu does not exist, there is no replay mode, there is no log, nothing plays), and the entry says
//! which system owns the answer. A name outside the table is answered `none` and reported the same way:
//! the line is kept, so the arm that does draw keeps drawing.
//!
//! A file of its own because this is a *vocabulary* rather than a walk: answering one more name is one
//! more row, and the table is what a reader comes here to see.

use super::*;

/// The globals, the value the migration answers for each, and the system that owns the real answer.
const GLOBALS: &[(&str, &str, &str)] = &[
    (
        "main_menu",
        "false",
        "whether the engine's main menu is up: Vela's is M12.2's, and no Vela game is in one",
    ),
    (
        "_in_replay",
        "false",
        "whether a replay is running, which is replay mode — the M12.3 side of M8's rollback",
    ),
    (
        "_history_list",
        "none",
        "the dialogue log: nothing records what was said yet (`SCREENS.md §7`, M12.2)",
    ),
    (
        "config.has_autosave",
        "false",
        "an autosave system Vela has not got",
    ),
    (
        "config.has_quicksave",
        "false",
        "the screens over save slots are M12.2's; the actions exist and the slots have no page",
    ),
    (
        "config.has_music",
        "false",
        "whether this build can play music, which is M12.3's audio: nothing plays yet",
    ),
    (
        "config.has_sound",
        "false",
        "sound effects, the second channel of M12.3's audio",
    ),
    (
        "config.has_voice",
        "false",
        "voice, the third channel of M12.3's audio",
    ),
    (
        "config.sample_sound",
        "false",
        "playing a sample when a volume is set, which is part of M12.3's audio",
    ),
    (
        "config.sample_voice",
        "false",
        "the same, for voice — M12.3's audio",
    ),
];

/// The condition with Ren'Py's globals answered, and an entry for each one answered.
pub(super) fn host_values(condition: &str, ctx: &mut Ctx) -> String {
    let mut out = condition.to_string();
    for (name, value, why) in GLOBALS {
        if !out.contains(name) {
            continue;
        }
        out = out.replace(name, value);
        ctx.gaps.unknown.push(format!(
            "`{name}`, which the migration answered `{value}`: {why}"
        ));
    }
    out
}
