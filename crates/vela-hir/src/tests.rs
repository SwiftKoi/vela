use vela_diag::Diagnostic;
use vela_span::FileId;
use vela_syntax::parse;

use crate::{Collected, Module, ModuleName, Modules, Transfer, collect, resolve, resolve_names};

fn collect_src(name: &str, source: &str) -> Collected {
    let file = FileId::from_raw(0);
    let parsed = parse(file, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse cleanly: {:?}",
        parsed.diagnostics
    );
    collect(ModuleName::new(name), file, &parsed.program)
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect()
}

/// The diagnostics one module's label references produce.
///
/// Resolution rather than name lookup: `E5003` is about the story graph, so a test for it
/// has to go through the same path the compiler does.
fn resolve_src(source: &str) -> Vec<Diagnostic> {
    let collected = collect_src("main", source);
    let table = Table(vec![collected.module]);
    resolve(&table.0[0], &table)
}

/// A fixed set of modules, for resolution tests.
struct Table(Vec<Module>);

impl Modules for Table {
    fn get(&self, name: &ModuleName) -> Option<&Module> {
        self.0.iter().find(|module| &module.name == name)
    }
}

#[test]
fn definitions_are_collected_with_their_kinds() {
    let collected = collect_src(
        "main",
        "const MAX = 1\n\
         default trust: int = 0\n\
         struct Route:\n    name: str\n\
         enum Ending:\n    good\n\
         character eileen:\n    name = \"E\"\n\
         image bg.forest = @\"a.png\"\n\
         theme dusk:\n    color = 0x10121a\n\
         label start:\n    return\n",
    );

    let definitions = &collected.module.definitions;
    assert_eq!(definitions["MAX"].kind, crate::DefKind::Constant);
    assert_eq!(definitions["trust"].kind, crate::DefKind::Default);
    assert_eq!(definitions["Route"].kind, crate::DefKind::Struct);
    assert_eq!(definitions["Ending"].kind, crate::DefKind::Enum);
    assert_eq!(definitions["eileen"].kind, crate::DefKind::Character);
    // An image's name is its dotted path, so `bg.forest` defines exactly that.
    assert_eq!(definitions["bg.forest"].kind, crate::DefKind::Image);
    assert_eq!(definitions["dusk"].kind, crate::DefKind::Theme);
    assert_eq!(definitions["start"].kind, crate::DefKind::Label);
    assert!(collected.diagnostics.is_empty());
}

#[test]
fn a_name_defined_twice_is_reported_e2003() {
    let collected = collect_src(
        "main",
        "label start:\n    return\n\nfn start():\n    return\n",
    );

    assert_eq!(codes(&collected.diagnostics), vec!["E2003"]);
    let diagnostic = &collected.diagnostics[0];
    assert!(diagnostic.message.contains("start"));
    // The second definition is the primary span; the first is the note, so a reader can
    // see both without hunting.
    assert_eq!(diagnostic.secondary.len(), 1);
}

#[test]
fn the_label_graph_records_jumps_and_calls_from_nested_blocks() {
    let collected = collect_src(
        "main",
        "label start:\n\
         \x20   jump other\n\
         \x20   menu \"pick\":\n\
         \x20       \"a\":\n\
         \x20           call inner\n\
         \x20       \"b\":\n\
         \x20           return\n\
         \n\
         label other:\n\
         \x20   if x:\n\
         \x20       jump start\n\
         \x20   else:\n\
         \x20       jump inner\n\
         \n\
         label inner:\n\
         \x20   return\n",
    );

    let story = &collected.module.story;
    let start: Vec<&str> = story
        .targets_of("start")
        .iter()
        .map(|target| target.label())
        .collect();
    // The `call` is inside a menu choice, so a walk that did not descend would miss it.
    assert_eq!(start, vec!["other", "inner"]);

    let other: Vec<&str> = story
        .targets_of("other")
        .iter()
        .map(|target| target.label())
        .collect();
    assert_eq!(other, vec!["start", "inner"]);

    assert!(story.targets_of("inner").is_empty());
}

#[test]
fn a_call_and_a_jump_are_distinguished() {
    let collected = collect_src(
        "main",
        "label a:\n    call b\n    jump c\nlabel b:\n    return\nlabel c:\n    return\n",
    );

    let transfers: Vec<Transfer> = collected
        .module
        .story
        .targets_of("a")
        .iter()
        .map(|target| target.transfer)
        .collect();
    assert_eq!(transfers, vec![Transfer::Call, Transfer::Jump]);
}

#[test]
fn a_local_label_resolves() {
    let collected = collect_src("main", "label a:\n    jump b\nlabel b:\n    return\n");
    let table = Table(vec![collected.module]);

    assert!(resolve(&table.0[0], &table).is_empty());
}

#[test]
fn a_jump_to_a_label_that_does_not_exist_is_reported_e5003() {
    // The flagship: a typo'd label is caught before the game runs, not in front of a
    // playtester.
    let collected = collect_src(
        "main",
        "label a:\n    jump clearring\nlabel b:\n    return\n",
    );
    let table = Table(vec![collected.module]);
    let diagnostics = resolve(&table.0[0], &table);

    assert_eq!(codes(&diagnostics), vec!["E5003"]);
    assert!(diagnostics[0].message.contains("clearring"));
}

#[test]
fn a_qualified_reference_resolves_through_an_alias() {
    let here = collect_src(
        "main",
        "use chapters.forest as forest\nlabel a:\n    jump forest.clearing\n",
    );
    let there = collect_src("chapters.forest", "label clearing:\n    return\n");
    let table = Table(vec![there.module, here.module]);

    let main = table.0.iter().find(|m| m.name.as_str() == "main").unwrap();
    assert!(resolve(main, &table).is_empty(), "the alias should resolve");
}

#[test]
fn a_qualified_reference_to_a_module_that_was_not_imported_is_reported_e2002() {
    let here = collect_src("main", "label a:\n    jump forest.clearing\n");
    let there = collect_src("chapters.forest", "label clearing:\n    return\n");
    let table = Table(vec![there.module, here.module]);

    let main = table.0.iter().find(|m| m.name.as_str() == "main").unwrap();
    assert_eq!(codes(&resolve(main, &table)), vec!["E2002"]);
}

#[test]
fn a_qualified_reference_to_a_missing_label_names_the_module_it_looked_in() {
    let here = collect_src(
        "main",
        "use chapters.forest as forest\nlabel a:\n    jump forest.nope\n",
    );
    let there = collect_src("chapters.forest", "label clearing:\n    return\n");
    let table = Table(vec![there.module, here.module]);

    let main = table.0.iter().find(|m| m.name.as_str() == "main").unwrap();
    let diagnostics = resolve(main, &table);

    assert_eq!(codes(&diagnostics), vec!["E5003"]);
    // Naming the module matters: with `forest.nope` the reader needs to know it looked
    // in `chapters.forest` and not somewhere else.
    assert!(diagnostics[0].message.contains("forest.nope"));
    assert!(diagnostics[0].primary.message.contains("chapters.forest"));
}

#[test]
fn an_import_without_an_alias_is_usable_by_its_full_path() {
    let here = collect_src(
        "main",
        "use chapters.forest\nlabel a:\n    jump chapters.forest.clearing\n",
    );
    let there = collect_src("chapters.forest", "label clearing:\n    return\n");
    let table = Table(vec![there.module, here.module]);

    let main = table.0.iter().find(|m| m.name.as_str() == "main").unwrap();
    assert!(resolve(main, &table).is_empty());
}

/// The name diagnostics for a single module.
fn name_codes(source: &str) -> Vec<String> {
    let file = FileId::from_raw(0);
    let parsed = parse(file, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse cleanly: {:?}",
        parsed.diagnostics
    );
    let collected = collect(ModuleName::new("main"), file, &parsed.program);
    codes(&resolve_names(&collected.module, &parsed.program))
}

#[test]
fn a_name_that_means_nothing_is_reported_e2001() {
    assert_eq!(
        name_codes("label a:\n    var x = nope\n    return\n"),
        vec!["E2001"]
    );
}

#[test]
fn a_module_definition_resolves() {
    assert!(
        name_codes("default trust: int = 0\nlabel a:\n    var x = trust\n    return\n").is_empty()
    );
}

#[test]
fn a_local_resolves_from_where_it_is_declared() {
    assert!(name_codes("label a:\n    var x = 1\n    var y = x\n    return\n").is_empty());
}

#[test]
fn a_parameter_resolves() {
    assert!(name_codes("fn f(n: int) -> int:\n    return n\n").is_empty());
}

#[test]
fn a_builtin_resolves() {
    assert!(name_codes("label a:\n    var s = str(1)\n    return\n").is_empty());
}

#[test]
fn only_the_base_of_a_field_is_a_name() {
    // `route.name` looks up `route`. The field itself is not in scope, and reporting it
    // would make every field access an error.
    assert!(
        name_codes(
            "struct Route:\n    name: str\nlabel a:\n    var route = Route\n    var n = route.name\n    return\n"
        )
        .is_empty()
    );
}

#[test]
fn a_for_binding_is_in_scope_in_the_body() {
    assert!(
        name_codes("label a:\n    for item in [1, 2]:\n        var x = item\n    return\n")
            .is_empty()
    );
}

#[test]
fn a_match_binding_is_in_scope_in_its_arm() {
    assert!(
        name_codes(
            "enum E:\n    bad(reason: str)\n\ndefault x: E = E.bad\n\nlabel a:\n    match x:\n        when E.bad(reason):\n            var y = reason\n        else:\n            return\n    return\n"
        )
        .is_empty()
    );
}

#[test]
fn a_lambda_parameter_resolves_inside_the_body() {
    assert!(name_codes("label a:\n    var f = fn(v: int) -> v + 1\n    return\n").is_empty());
}

#[test]
fn a_name_inside_an_interpolation_is_resolved() {
    assert_eq!(
        name_codes("label a:\n    var s = \"score: {nope}\"\n    return\n"),
        vec!["E2001"]
    );
}

#[test]
fn an_import_of_a_module_that_does_not_exist_is_reported_e2001() {
    let here = collect_src("main", "use nonexistent\n\nlabel a:\n    return\n");
    let table = Table(vec![here.module]);
    let diagnostics = resolve(&table.0[0], &table);

    assert_eq!(codes(&diagnostics), vec!["E2001"]);
}

#[test]
fn a_speaker_that_is_not_a_character_is_reported_e5001() {
    assert_eq!(
        name_codes("label a:\n    eileen \"Hi.\"\n    return\n"),
        vec!["E5001"]
    );
}

#[test]
fn a_declared_character_can_speak() {
    assert!(
        name_codes(
            "character eileen:\n    name = \"E\"\n\nlabel a:\n    eileen \"Hi.\"\n    return\n"
        )
        .is_empty()
    );
}

#[test]
fn a_misspelled_label_is_suggested_e5003() {
    let diagnostics = resolve_src(
        "label start:\n    jump clearring\n    return\n\nlabel clearing:\n    return\n",
    );

    let diagnostic = diagnostics.first().expect("no diagnostic");
    assert_eq!(diagnostic.code.as_str(), "E5003");
    assert!(
        diagnostic
            .help
            .as_deref()
            .is_some_and(|help| help.contains("clearing")),
        "no suggestion: {:?}",
        diagnostic.help
    );
}

/// A qualified jump suggests the *qualified* correction — a bare `clearing` would send the
/// author to a second mistake.
#[test]
fn a_suggestion_keeps_the_qualifier() {
    // Two modules, because a qualified jump has to resolve through a `use` first — without
    // one the reference is `E2002` and never reaches the label check.
    let here = collect_src(
        "main",
        "use forest\n\nlabel start:\n    jump forest.clearring\n    return\n",
    );
    let there = collect_src("forest", "label clearing:\n    return\n");
    let table = Table(vec![here.module, there.module]);
    let diagnostics = resolve(&table.0[0], &table);

    let diagnostic = diagnostics.first().expect("no diagnostic");
    assert!(
        diagnostic
            .help
            .as_deref()
            .is_some_and(|help| help.contains("forest.clearing")),
        "the suggestion dropped its qualifier: {:?}",
        diagnostic.help
    );
}

/// A name that is nowhere near any label gets no suggestion, because a wrong one sends
/// someone looking in the wrong place.
#[test]
fn a_name_with_no_near_neighbour_gets_no_suggestion() {
    let diagnostics =
        resolve_src("label start:\n    jump qqqqqqqq\n    return\n\nlabel clearing:\n    return\n");

    let diagnostic = diagnostics.first().expect("no diagnostic");
    assert_eq!(diagnostic.code.as_str(), "E5003");
    assert!(
        diagnostic.help.is_none(),
        "an unrelated name was offered: {:?}",
        diagnostic.help
    );
}

/// The note lists what the module has, so an author can see the whole namespace at once.
#[test]
fn the_note_lists_the_modules_labels() {
    let diagnostics = resolve_src(
        "label start:\n    jump nope\n    return\n\nlabel clearing:\n    return\n\nlabel river:\n    return\n",
    );

    let diagnostic = diagnostics.first().expect("no diagnostic");
    let note = diagnostic.notes.first().expect("no note");
    assert!(note.contains("clearing"), "{note}");
    assert!(note.contains("river"), "{note}");
}
