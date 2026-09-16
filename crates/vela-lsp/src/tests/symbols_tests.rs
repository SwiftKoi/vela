//! The symbol index: what an offset names, and where else it is named.
//!
//! Two files, because that is the case the milestone names — "goto-definition, references … across
//! module boundaries" — and because a single-file fixture cannot tell a resolution rule from a name
//! lookup: with one module, every path resolves whether or not the qualifier is understood.

use vela_compile::Session;

use crate::symbols::{Named, at, declaration, occurrences};

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
    let text = &FOREST[span.start() as usize..span.end() as usize];
    assert!(text.starts_with("label clearing:"), "{text:?}");
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
    let text = &MAIN[span.start() as usize..span.end() as usize];
    assert!(text.starts_with("label tally:"), "{text:?}");
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
