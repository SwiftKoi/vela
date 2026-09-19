//! `vela migrate` on a whole Ren'Py project.
//!
//! The milestone's first exit criterion is that a sample project *transpiles, compiles with zero
//! errors, and runs*, and the third is that nothing is silently mistranslated. Both are properties
//! of the command rather than of the transpiler, so they are tested here: a Ren'Py project in a
//! temporary directory, migrated, and then **checked** — because "the output compiles" is the only
//! claim worth making about a migration, and it is not one the migrator can make about itself.

use std::path::{Path, PathBuf};

use super::support::cli;

/// A one-pixel PNG, which is all an image has to be to be an image.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc, 0xcf, 0xc0, 0x50,
    0x0f, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xa9, 0x8c, 0x21, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

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
        "define config.name = _(\"The Question\")\ndefine config.version = \"7.0\"\n",
    )
    .expect("write the configuration");

    // The pictures the story stages, because a `scene` naming an image nothing declares is `E5019` and
    // the migrated project has to check — a project with a story and no images is not one.
    let images = base.join("game").join("images");
    std::fs::create_dir_all(&images).expect("create the image directory");
    for name in ["bg lecturehall.png", "sylvie green normal.png"] {
        std::fs::write(images.join(name), PNG).expect("write an image");
    }

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

    // The story and its images translated, and the engine configuration dispositioned declaration
    // by declaration: the name becomes `vela.toml`'s, and a knob with no counterpart is reported.
    assert!(out.contains("migrated 2 source file(s)"), "{out}");

    let out_dir = migrated(&source);
    let manifest = std::fs::read_to_string(out_dir.join("vela.toml")).expect("the manifest");
    assert!(manifest.contains("name = \"The Question\""), "{manifest}");
    let report = std::fs::read_to_string(out_dir.join("MIGRATION.md")).expect("the report");
    assert!(report.contains("config.version"), "{report}");
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

/// A testcase migrates when every step of it is one Vela has, and is reported when it is not.
///
/// A test is a *sequence*: `advance until …` then `expect …` asserts what the wait left behind, so a
/// testcase with a step missing is a different test rather than a smaller one — it would run to a
/// different conclusion and pass for reasons that have nothing to do with the story. The rule is all
/// of it or none of it, and the entry says which step stopped it.
#[test]
fn a_testcase_migrates_whole_or_is_reported() {
    let base = std::env::temp_dir().join(format!("vela-migrate-tests-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("game")).expect("create project");
    std::fs::write(
        base.join("game").join("script.rpy"),
        "label start:\n    menu:\n        \"Take the left.\":\n            \"Left it is.\"\n        \"Take the right.\":\n            \"Right it is.\"\n    return\n",
    )
    .expect("write the story");
    // `click "take the left"` names a menu option — the option is spelled with a capital and a full
    // stop, which is Ren'Py's own containment rule and the reason this is a rule rather than a guess.
    // `click "Start"` names a *control*, which is the `click` step's own subject and not a report
    // entry; what stops the second testcase is the line after it, a condition about which *screen* is
    // up — a subject Vela's `advance until shown` does not have (it waits for text).
    std::fs::write(
        base.join("game").join("testcases.rpy"),
        "testcase the left way:\n    click \"take the left\"\n    assert \"Left it is.\"\n\n\
         testcase by the menu:\n    click \"Start\"\n    advance until screen \"main_menu\"\n",
    )
    .expect("write the testcases");

    let (code, out) = cli(&["migrate", &base.to_string_lossy()]);
    assert_eq!(code, 0, "the migration failed:\n{out}");

    let out_dir = migrated(&base);
    let tests = std::fs::read_to_string(out_dir.join("src/testcases.vela")).expect("test items");
    assert!(tests.contains("test \"the left way\":"), "{tests}");
    assert!(tests.contains("    choose \"Take the left.\"\n"), "{tests}");
    assert!(
        tests.contains("    expect shown \"Left it is.\"\n"),
        "{tests}"
    );
    // The one that waits on a screen's *name* is not written at all, and says so.
    assert!(!tests.contains("by the menu"), "{tests}");
    let report = std::fs::read_to_string(out_dir.join("MIGRATION.md")).expect("the report");
    assert!(
        !report.contains("click \"Start\""),
        "a click on a control is a step Vela has: {report}"
    );
    assert!(report.contains("main_menu"), "{report}");

    // And the translated test runs, which is the only claim worth making about it.
    let (code, tested) = cli(&["test", &out_dir.to_string_lossy()]);
    assert_eq!(code, 0, "the migrated test does not pass:\n{tested}");

    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&out_dir);
}

/// A click that names a control migrates to `click`, and the words it names are the ones Ren'Py
/// matched on — the control's own text.
///
/// The test is written, not *run*, and the reason is a gap this milestone names rather than hides: a
/// click presses a control on a screen the run has open, and what opens the first one — the game's own
/// menu (`M12.2`) or a key binding (`M12.3`) — is not on this milestone's list
/// (`docs/roadmap/M12.1-screen-language.md`, item 18).
#[test]
fn a_click_on_a_control_becomes_a_click_step() {
    let base = std::env::temp_dir().join(format!("vela-migrate-click-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("game")).expect("create project");
    std::fs::write(
        base.join("game").join("script.rpy"),
        "label start:\n    \"One.\"\n    return\n",
    )
    .expect("write the story");
    std::fs::write(
        base.join("game").join("testcases.rpy"),
        "testcase the menu:\n    click \"Start\"\n    assert \"One.\"\n",
    )
    .expect("write the testcases");

    let (code, out) = cli(&["migrate", &base.to_string_lossy()]);
    assert_eq!(code, 0, "the migration failed:\n{out}");

    let out_dir = migrated(&base);
    let tests = std::fs::read_to_string(out_dir.join("src/testcases.vela")).expect("test items");
    assert!(tests.contains("    click \"Start\"\n"), "{tests}");
    assert!(tests.contains("    expect shown \"One.\"\n"), "{tests}");

    // And the migrator's own invariant holds: what it wrote compiles.
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
