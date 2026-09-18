//! `vela migrate` on a whole Ren'Py project.
//!
//! The milestone's first exit criterion is that a sample project *transpiles, compiles with zero
//! errors, and runs*, and the third is that nothing is silently mistranslated. Both are properties
//! of the command rather than of the transpiler, so they are tested here: a Ren'Py project in a
//! temporary directory, migrated, and then **checked** — because "the output compiles" is the only
//! claim worth making about a migration, and it is not one the migrator can make about itself.

use std::path::{Path, PathBuf};

use super::support::cli;

/// The story, which is indentation-significant — so it is written as it will be read, in a raw
/// string, rather than with `\` continuations that would eat the indentation.
const STORY: &str = r##"# The story.
define s = Character(_("Sylvie"), color="#c8ffc8")
default book = False

label start:

    play music "illurock.opus"

    scene bg lecturehall
    with fade

    "It rained all evening."

    show sylvie green normal
    with dissolve

    s "Hi there! How was class?"

    menu:
        "I could ask her now."

        "Ask her right away.":
            jump rightaway

        "Ask her later.":
            jump later

label rightaway:

    $ book = True
    if book:
        "{b}Good Ending{/b}."
    return

label later:
    "{i}Bad Ending{/i}."
    return
"##;

/// A Ren'Py project with the constructs the sample uses.
fn renpy_project(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("vela-migrate-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("game")).expect("create project");

    // A byte-order mark, because Ren'Py's own scripts carry one.
    std::fs::write(
        base.join("game").join("script.rpy"),
        format!("\u{feff}{STORY}"),
    )
    .expect("write the story");
    std::fs::write(
        base.join("game").join("options.rpy"),
        "define config.name = _(\"The Question\")\n",
    )
    .expect("write the configuration");

    base
}

/// The migrated project's directory, beside the source it came from.
fn migrated(source: &Path) -> PathBuf {
    source.with_file_name(format!(
        "{}_migrated",
        source.file_name().unwrap_or_default().to_string_lossy()
    ))
}

/// The whole journey: a Ren'Py project in, a Vela project out that checks with no problems.
#[test]
fn a_renpy_project_migrates_into_one_that_checks() {
    let source = renpy_project("question");
    let (code, out) = cli(&["migrate", &source.to_string_lossy(), "--report"]);
    assert_eq!(code, 0, "the migration failed:\n{out}");

    // One story file translated, and the engine configuration reported rather than guessed at.
    assert!(out.contains("migrated 1 source file(s)"), "{out}");
    assert!(out.contains("options.rpy"), "{out}");

    let out_dir = migrated(&source);
    assert!(out_dir.join("vela.toml").exists(), "{out}");
    assert!(out_dir.join("src/script.vela").exists(), "{out}");
    assert!(out_dir.join("MIGRATION.md").exists(), "{out}");

    // The claim that matters: the migrated project compiles.
    let (code, checked) = cli(&["check", &out_dir.to_string_lossy()]);
    assert_eq!(code, 0, "the migrated project does not check:\n{checked}");
    assert!(checked.contains("no problems"), "{checked}");

    // And it runs: every command the story presents, in order.
    let (code, ran) = cli(&["run", &out_dir.to_string_lossy(), "--headless"]);
    assert_eq!(code, 0, "the migrated project does not run:\n{ran}");
    assert!(ran.contains("say \"It rained all evening.\""), "{ran}");
    assert!(
        ran.contains("menu \"I could ask her now.\""),
        "the menu caption is on the menu line:\n{ran}"
    );
    assert!(ran.contains("say \"{b}Good Ending{/b}.\""), "{ran}");

    let _ = std::fs::remove_dir_all(&source);
    let _ = std::fs::remove_dir_all(&out_dir);
}

/// A story that fades to black migrates to a black *picture*, not to a name nothing declares.
///
/// Ren'Py's `00definitions.rpy` defines `image black = Solid("#000")` as a built-in, so `scene black`
/// is ordinary Ren'Py with no file behind it. Vela has no built-in, and a `scene` naming something
/// nothing declares used to draw a placeholder box labelled `black` — which a capture of the
/// migrated ending showed.
#[test]
fn renpys_built_in_black_becomes_a_solid() {
    let base = std::env::temp_dir().join(format!("vela-migrate-solid-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("game")).expect("create project");
    std::fs::write(
        base.join("game").join("script.rpy"),
        "label start:\n    scene black\n    \"We get married shortly after that.\"\n    return\n",
    )
    .expect("write the story");

    let (code, out) = cli(&["migrate", &base.to_string_lossy()]);
    assert_eq!(code, 0, "the migration failed:\n{out}");

    let out_dir = migrated(&base);
    let images = std::fs::read_to_string(out_dir.join("src/images.vela")).expect("declarations");
    assert!(images.contains("image black = 0x0"), "{images}");
    // And it is a translation rather than a refusal: nothing is left for a person (two files, the
    // story and the declarations).
    assert!(out.contains("migrated 2 source file(s)"), "{out}");

    let (code, checked) = cli(&["check", &out_dir.to_string_lossy()]);
    assert_eq!(code, 0, "the migrated project does not check:\n{checked}");

    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&out_dir);
}

/// A project that is not Ren'Py is a usage error, not a crash and not a silent empty migration.
#[test]
fn a_directory_that_is_not_a_renpy_project_is_refused() {
    let base = std::env::temp_dir().join(format!("vela-migrate-not-a-game-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create the directory");

    let (code, out) = cli(&["migrate", &base.to_string_lossy()]);
    assert_eq!(code, 2, "expected a usage error:\n{out}");
    assert!(out.contains("no `game/` directory"), "{out}");

    let _ = std::fs::remove_dir_all(&base);
}

/// `--strict` turns the number of things to port into an exit code, which is what lets a team
/// track migration progress as a number that goes to zero.
#[test]
fn strict_fails_when_something_needs_a_person() {
    let source = renpy_project("strict");
    let (code, out) = cli(&["migrate", &source.to_string_lossy(), "--strict"]);
    assert_eq!(code, 1, "expected diagnostics:\n{out}");
    assert!(
        out.contains("to port by hand") || out.contains("thing(s) to port"),
        "{out}"
    );

    let _ = std::fs::remove_dir_all(&source);
    let _ = std::fs::remove_dir_all(migrated(&source));
}
