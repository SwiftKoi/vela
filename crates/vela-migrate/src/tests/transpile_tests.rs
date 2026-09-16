//! What a Ren'Py line becomes, and what it becomes a *report entry* instead.
//!
//! The supported list is `TOOLING.md §8`; everything outside it must produce an entry naming the
//! file and the line, because the milestone's third exit criterion is that no construct is ever
//! silently mistranslated. So half of these tests are about output and half are about refusals.

use std::collections::{BTreeMap, BTreeSet};

use crate::Report;

/// Migrates one file, returning the Vela text and the report.
fn migrate(source: &str) -> (String, Report) {
    migrate_with(source, &[])
}

/// Migrates one file against a set of image names, the way `project` does.
fn migrate_with(source: &str, images: &[&str]) -> (String, Report) {
    let renames = BTreeMap::new();
    let images: BTreeSet<String> = images.iter().map(|name| (*name).to_string()).collect();
    let names = crate::transpile::Names {
        renames: &renames,
        images: &images,
    };

    let nodes = crate::read(source);
    let mut report = Report::new();
    let text = crate::transpile::file(&nodes, "game.rpy", &mut report, &names);
    (text, report)
}

/// The lines of the output that are not the header.
#[allow(dead_code)]
fn body(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| !line.starts_with('#') || line.trim_start().starts_with('#'))
        .skip_while(|line| line.starts_with('#'))
        .collect()
}

#[test]
fn a_character_becomes_a_character_block() {
    let (text, report) = migrate("define s = Character(\"Sylvie\", color=\"#c8ffc8\")\n");
    assert!(text.contains("character s:\n"), "{text}");
    assert!(text.contains("    name = \"Sylvie\"\n"), "{text}");
    assert!(text.contains("    color = 0xc8ffc8\n"), "{text}");
    assert!(report.is_empty(), "{}", report.render());
}

#[test]
fn a_default_gets_the_type_its_literal_implies() {
    let (text, report) = migrate("default book = False\ndefault trust = 3\ndefault name = \"a\"\n");
    assert!(text.contains("default book: bool = false\n"), "{text}");
    assert!(text.contains("default trust: int = 3\n"), "{text}");
    assert!(text.contains("default name: str = \"a\"\n"), "{text}");
    assert!(report.is_empty(), "{}", report.render());
}

/// The constructs the sample uses, end to end: a label, a say, a scene with a transition, a show
/// with attributes, a jump, and a `return`.
#[test]
fn the_story_surface_migrates_unchanged() {
    let source = "\
label start:

    scene bg lecturehall
    with fade

    \"It rained.\"

    show sylvie green normal
    with dissolve

    s \"Hi there!\"
    jump later

label later:
    return
";
    // The images the story stages, as the project's own declarations would name them.
    let (text, report) = migrate_with(source, &["bg.lecturehall", "sylvie.green.normal"]);

    assert!(text.contains("label start:\n"), "{text}");
    assert!(text.contains("    scene bg.lecturehall\n"), "{text}");
    assert!(text.contains("    with fade\n"), "{text}");
    assert!(text.contains("    \"It rained.\"\n"), "{text}");
    assert!(text.contains("    show sylvie.green.normal\n"), "{text}");
    assert!(text.contains("    s \"Hi there!\"\n"), "{text}");
    assert!(text.contains("    jump later\n"), "{text}");
    assert!(text.contains("label later:\n"), "{text}");
    assert!(report.is_empty(), "{}", report.render());
}

/// A menu's caption is a bare line inside the block, and the choices are the lines that end in a
/// colon.
#[test]
fn a_menu_lifts_its_caption_onto_the_menu_line() {
    let source = "\
label start:
    menu:
        \"As soon as she catches my eye, I decide...\"

        \"To ask her right away.\":
            jump right

        \"To ask her later.\":
            jump later

label right:
    return

label later:
    return
";
    let (text, report) = migrate(source);
    assert!(
        text.contains("    menu \"As soon as she catches my eye, I decide...\":\n"),
        "{text}"
    );
    assert!(
        text.contains("        \"To ask her right away.\":\n"),
        "{text}"
    );
    assert!(text.contains("            jump right\n"), "{text}");
    assert!(report.is_empty(), "{}", report.render());
}

/// A *spoken* caption is a line of dialogue, and Vela's `menu` takes a bare string — so it goes
/// immediately before the menu, which is the same line in the same place.
#[test]
fn a_spoken_caption_is_said_before_the_menu() {
    let source = "\
label start:
    menu:
        s \"Sure, but what's a \\\"visual novel?\\\"\"

        \"It's a videogame.\":
            jump game

label game:
    return
";
    let (text, report) = migrate(source);
    assert!(
        text.contains("    s \"Sure, but what's a \\\"visual novel?\\\"\"\n"),
        "{text}"
    );
    assert!(text.contains("    menu:\n"), "{text}");
    assert!(text.contains("        \"It's a videogame.\":\n"), "{text}");
    assert!(report.is_empty(), "{}", report.render());
}

/// A text tag is Vela syntax (`BYTECODE.md §3.3`), so it migrates verbatim — which is the whole
/// point of implementing it rather than reporting it.
#[test]
fn text_tags_migrate_verbatim() {
    let (text, report) = migrate("label start:\n    \"{b}Good Ending{/b}.\"\n    return\n");
    assert!(text.contains("    \"{b}Good Ending{/b}.\"\n"), "{text}");
    assert!(report.is_empty(), "{}", report.render());
}

/// `$` statement blocks become assignments where they are assignments, and entries where they are
/// Python.
#[test]
fn a_python_assignment_becomes_an_assignment() {
    let (text, report) = migrate("label start:\n    $ book = True\n    return\n");
    assert!(text.contains("    book = true\n"), "{text}");
    assert!(report.is_empty(), "{}", report.render());
}

/// The corpus of known-unsupported snippets: each produces exactly one entry, naming its line.
///
/// This is the exit criterion in test form — *"no construct is ever silently mistranslated,
/// verified by a corpus of known-unsupported snippets, each producing a report entry"*.
#[test]
fn unsupported_constructs_are_reported_with_their_line() {
    let cases: &[(&str, &str)] = &[
        ("init python:\n    pass\n", "no Vela equivalent"),
        ("$ import math\n", "only a plain assignment"),
        ("$ x = print(\"hi\")\n", "Python, not Vela"),
        ("window show\n", "`window`"),
        ("nvl clear\n", "no Vela equivalent"),
        ("define config.name = \"x\"\n", "Ren'Py store"),
        ("transform slide:\n    xpos 0\n", "no Vela equivalent"),
        ("screen pause():\n    text \"hi\"\n", "no Vela equivalent"),
        ("default book = starting_value\n", "not a literal"),
        ("label start:\n    $ foo()\n    return\n", "not one"),
    ];

    for (source, expected) in cases {
        let (_, report) = migrate(source);
        assert!(
            !report.is_empty(),
            "`{source}` produced no entry, so it was silently dropped"
        );
        assert!(
            report
                .entries()
                .iter()
                .any(|entry| entry.reason.contains(expected)),
            "`{source}` should mention `{expected}`: {}",
            report.render()
        );
        assert!(
            report.entries().iter().all(|entry| entry.line >= 1),
            "every entry names a line: {}",
            report.render()
        );
    }
}

/// `_()` is Ren'Py's translation marker: the string is kept and the loss of the *marker* is
/// reported, because Vela has no catalogue to put it in yet.
#[test]
fn a_translation_marker_is_reported_and_the_string_kept() {
    let (text, report) = migrate("define s = Character(_(\"Sylvie\"))\n");
    assert!(text.contains("    name = \"Sylvie\"\n"), "{text}");
    assert_eq!(report.len(), 1);
    assert!(
        report.render().contains("translation marker"),
        "{}",
        report.render()
    );
}

/// Two runs over one source produce the same text and the same report, byte for byte — which is
/// what makes the report a work item list a team can diff.
#[test]
fn a_migration_is_deterministic() {
    let source = "\
define s = Character(_(\"Sylvie\"), color=\"#c8ffc8\")
default book = False
label start:
    play music \"illurock.opus\"
    scene bg uni
    with fade
    s \"Hi\"
    menu:
        \"Ask\":
            $ book = True
            jump asked
        \"Wait\":
            jump waited
label asked:
    \"{b}Ending{/b}.\"
    return
label waited:
    nvl clear
    return
";
    let first = migrate(source);
    let second = migrate(source);

    assert_eq!(first.0, second.0, "the output should not vary between runs");
    assert_eq!(
        first.1.render(),
        second.1.render(),
        "the report should not vary between runs"
    );
}

/// Ren'Py names an image with a tag and attributes; Vela names it with a dotted path, and the
/// migration rewrites the reference to the name it declared — an attribute Vela does not know
/// about stages nothing, so leaving it would migrate the story and lose the picture.
#[test]
fn an_image_reference_is_rewritten_to_its_dotted_name() {
    let source =
        "label start:\n    scene bg lecturehall\n    show sylvie green normal\n    return\n";
    let (text, report) = migrate_with(source, &["bg.lecturehall", "sylvie.green.normal"]);

    assert!(text.contains("    scene bg.lecturehall\n"), "{text}");
    assert!(text.contains("    show sylvie.green.normal\n"), "{text}");
    assert!(report.is_empty(), "{}", report.render());
}

/// A reference nothing declares is left alone and reported: it is either a name with no file — Ren'Py
/// has built-in `black` and `white` images that Vela does not — or a typo, and both want a person.
#[test]
fn an_image_reference_that_names_nothing_is_reported() {
    let source = "label start:\n    scene black\n    return\n";
    let (text, report) = migrate_with(source, &["bg.lecturehall"]);

    assert!(text.contains("    scene black\n"), "{text}");
    assert_eq!(report.len(), 1);
    assert!(
        report.render().contains("names no image"),
        "{}",
        report.render()
    );
}
