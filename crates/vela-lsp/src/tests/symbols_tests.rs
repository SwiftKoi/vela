//! The symbol index: what an offset names, and where else it is named.
//!
//! Two files, because that is the case the milestone names — "goto-definition, references … across
//! module boundaries" — and because a single-file fixture cannot tell a resolution rule from a name
//! lookup: with one module, every path resolves whether or not the qualifier is understood.

use vela_compile::Session;
use vela_hir::DefKind;

use crate::symbols::{Named, at, declaration, occurrences, rename};

/// Every kind of declaration, one per `DefKind`.
///
/// The names are chosen so that each one is a *prefix of something else later in the line* as often as
/// possible — `bg.room` before `@"art/room.png"`, `turns_word` before its parameter — because that is
/// what a search for the name would find first if it started anywhere but after the keyword.
const ALL_KINDS: &str = "\
const MAX_TRUST: int = 1

default trust: int = 0

struct Route:
    name: str

enum Ending:
    warm
    cold

character ren:
    name = \"Ren\"

image bg.room = @\"art/room.png\"

transform slide_in:
    x = 0.0

screen pause:
    layer ui
    text \"hi\"

style body:
    color = 0xffffff

theme dusk:
    color bg = 0x10121a

effect rand.int(low: int, high: int) -> int

fn rank(e: Ending) -> int:
    return 1

label start:
    return
";

/// One per `DefKind`, so a kind added without a fixture line fails the count.
const ALL_KIND_COUNT: usize = 13;

/// A two-module session, and the ids of its files.
///
/// `main` jumps into `chapters.forest` through an alias, and `forest` jumps back the qualified way, so
/// both directions of a cross-module transfer are in the fixture.
fn project() -> (Session, vela_span::FileId, vela_span::FileId) {
    let main = "\
use chapters.forest as forest

label start:
    \"Start.\"
    jump forest.clearing

label tally:
    return
";
    let forest = "\
use main

label clearing:
    \"Trees.\"
    call main.tally
    jump main.tally
";
    let mut session = Session::new();
    // Named the way the command line names them — relative to `src/`, which is what makes the module
    // `chapters.forest` rather than `src.chapters.forest`. A fixture that named them the other way
    // would pass while an editor could not resolve a single cross-module reference, which is exactly
    // what the first version of this test did.
    let main_file = session.set_file("main.vela", main);
    let forest_file = session.set_file("chapters/forest.vela", forest);
    session.set_entry("main.start");
    (session, main_file, forest_file)
}

/// The offset of `needle` in `text`, as a u32.
fn offset_of(text: &str, needle: &str) -> u32 {
    text.find(needle).expect("the fixture contains it") as u32
}

const MAIN: &str = "\
use chapters.forest as forest

label start:
    \"Start.\"
    jump forest.clearing

label tally:
    return
";

const FOREST: &str = "\
use main

label clearing:
    \"Trees.\"
    call main.tally
    jump main.tally
";

#[test]
fn a_transfer_names_the_label_it_reaches_across_a_module() {
    let (mut session, main_file, _) = project();
    let offset = offset_of(MAIN, "jump forest.clearing") + 5;

    let found = at(&mut session, main_file, offset).expect("a jump names a label");
    assert_eq!(
        found.named,
        Named::Definition {
            module: vela_hir::ModuleName::new("chapters.forest"),
            name: "clearing".to_string(),
        },
        "the alias resolves to the module it names"
    );
}

/// And the declaration of that name is in the *other* file — which is what makes this feature worth
/// having rather than a lookup in the file the cursor is already in.
#[test]
fn the_declaration_is_in_the_other_file() {
    let (mut session, main_file, forest_file) = project();
    let offset = offset_of(MAIN, "jump forest.clearing") + 5;

    let found = at(&mut session, main_file, offset).expect("a jump names a label");
    let (file, span) = declaration(&mut session, &found.named).expect("the label exists");

    assert_eq!(
        file, forest_file,
        "the label is declared in the forest module"
    );
    // The *name's* span, not the declaration's: a goto that landed on the keyword would put the caret
    // one word to the left of what the reader asked about, and a rename would delete the word `label`.
    assert_eq!(
        &FOREST[span.start() as usize..span.end() as usize],
        "clearing"
    );
}

/// Both directions at once: the unqualified `jump main.tally` in the other module resolves back into
/// `main`, and its declaration is the `label tally` there.
#[test]
fn an_unqualified_transfer_stays_in_its_own_module() {
    let (mut session, main_file, forest_file) = project();
    let offset = offset_of(FOREST, "call main.tally") + 5;

    let found = at(&mut session, forest_file, offset).expect("a call names a label");
    let (file, span) = declaration(&mut session, &found.named).expect("the label exists");

    assert_eq!(
        found.named,
        Named::Definition {
            module: vela_hir::ModuleName::new("main"),
            name: "tally".to_string(),
        }
    );
    assert_eq!(file, main_file);
    assert_eq!(&MAIN[span.start() as usize..span.end() as usize], "tally");
}

/// The spans a rename would replace cover *the name and nothing else*, which is the difference between
/// renaming a label and deleting the word `jump`.
#[test]
fn a_rename_replaces_every_name_exactly() {
    let (mut session, _, forest_file) = project();
    let offset = offset_of(FOREST, "call main.tally") + 5;
    let found = at(&mut session, forest_file, offset).expect("a call names a label");

    let spans = rename(&mut session, &found.named).expect("this name can be renamed");
    assert_eq!(
        spans.len(),
        3,
        "the declaration and both references: {spans:?}"
    );

    for (file, span) in spans {
        let source = session.sources().file(file).text();
        assert_eq!(
            &source[span.start() as usize..span.end() as usize],
            "tally",
            "a rename span that is not the name would rewrite something else"
        );
    }
}

/// A declaration's own name is recovered from its first line, for every kind of declaration there is.
///
/// This is the test that makes the recovery trustworthy rather than plausible: it is text position
/// arithmetic, and a kind whose keyword is longer than expected — `character`, `transform` — would be
/// off by the difference. A name span in the syntax tree is the better fix, and until then this is the
/// only thing standing between a rename and a file that no longer parses.
#[test]
fn a_declaration_name_is_recovered_for_every_kind() {
    let mut session = Session::new();
    let file = session.set_file("all.vela", ALL_KINDS);

    let names: Vec<(String, DefKind)> = session
        .symbols(file)
        .module
        .definitions
        .values()
        .map(|definition| (definition.name.clone(), definition.kind))
        .collect();

    assert_eq!(
        names.len(),
        ALL_KIND_COUNT,
        "the fixture declares every kind"
    );
    for (name, kind) in names {
        let named = Named::Definition {
            module: vela_hir::ModuleName::new("all"),
            name: name.clone(),
        };
        let (_, span) = occurrences(&mut session, &named)
            .first()
            .copied()
            .unwrap_or_else(|| panic!("no declaration found for {kind:?} `{name}`"));

        assert_eq!(
            &ALL_KINDS[span.start() as usize..span.end() as usize],
            name,
            "the recovered name span for a {kind:?} is wrong"
        );
    }
}

/// A rename refuses what it cannot place exactly, rather than editing part of it: a `use` names a
/// module, and a module is not a name inside one file.
#[test]
fn a_rename_of_something_that_is_not_a_name_refuses() {
    let (mut session, main_file, _) = project();
    let offset = offset_of(MAIN, "chapters.forest");
    let found = at(&mut session, main_file, offset).expect("a use names a module");

    assert_eq!(rename(&mut session, &found.named), None);
}

/// References are the union of both directions, and the declaration is among them: an editor that
/// renamed a label without its declaration would leave the file saying two different things.
#[test]
fn references_cover_both_modules_and_the_declaration() {
    let (mut session, main_file, forest_file) = project();
    let offset = offset_of(FOREST, "call main.tally") + 5;

    let found = at(&mut session, forest_file, offset).expect("a call names a label");
    let found = occurrences(&mut session, &found.named);

    let files: Vec<vela_span::FileId> = found.iter().map(|(file, _)| *file).collect();
    assert!(files.contains(&main_file), "the declaration: {files:?}");
    assert!(files.contains(&forest_file), "the call site: {files:?}");

    // The declaration, the `call`, and the `jump` — three places in two files.
    assert_eq!(found.len(), 3, "{found:?}");
}

/// An offset that names nothing answers nothing, rather than answering with the enclosing label: a
/// hover or a goto that guessed "you must mean the label around you" would be wrong wherever the
/// cursor is not on a name.
#[test]
fn an_offset_inside_a_body_names_nothing() {
    let (mut session, main_file, _) = project();
    let offset = offset_of(MAIN, "\"Start.\"");

    assert_eq!(at(&mut session, main_file, offset), None);
}

/// A definition's own name is nameable, so goto-definition on a label works from its declaration too.
#[test]
fn a_declaration_names_itself() {
    let (mut session, main_file, _) = project();
    let offset = offset_of(MAIN, "label start:") + 6;

    let found = at(&mut session, main_file, offset).expect("a label's own name");
    assert_eq!(
        found.named,
        Named::Definition {
            module: vela_hir::ModuleName::new("main"),
            name: "start".to_string(),
        }
    );
}

/// A `use` names a module, and goto-definition on it lands in that module's file.
#[test]
fn a_use_names_a_module() {
    let (mut session, main_file, forest_file) = project();
    let offset = offset_of(MAIN, "chapters.forest");

    let found = at(&mut session, main_file, offset).expect("a use names a module");
    assert_eq!(
        found.named,
        Named::Import {
            module: vela_hir::ModuleName::new("chapters.forest"),
        }
    );

    let (file, _) = declaration(&mut session, &found.named).expect("the module is in the program");
    assert_eq!(file, forest_file);
}
