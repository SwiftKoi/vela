use crate::{Query, Session};

#[test]
fn parsing_twice_runs_once() {
    let mut session = Session::new();
    let file = session.set_file("main.vela", "label a:\n    \"x\"\n");

    session.parse(file);
    session.parse(file);

    assert_eq!(session.runs(Query::Parse), 1);
}

#[test]
fn editing_re_parses_only_the_edited_file() {
    // 200 labels across four modules — one file per module, which is the unit an editor
    // touches. Re-parsing everything would show up plainly in the counts.
    let mut session = Session::new();
    let mut ids = Vec::new();
    for module in ["ch1", "ch2", "ch3", "ch4"] {
        let mut source = String::new();
        for index in 0..50 {
            source.push_str(&format!("label {module}_scene_{index}:\n    \"line\"\n"));
        }
        ids.push(session.set_file(format!("{module}.vela"), source));
    }

    for &id in &ids {
        session.parse(id);
    }
    assert_eq!(session.runs(Query::Parse), 4);

    // Nothing changed, so nothing re-runs. This is the half of the property that makes
    // the other half worth having: a cache that always misses is not a cache.
    for &id in &ids {
        session.parse(id);
    }
    assert_eq!(
        session.runs(Query::Parse),
        4,
        "unchanged files must not be re-parsed"
    );

    // Edit one module. Exactly one file re-parses.
    session.set_file("ch2.vela", "label edited:\n    \"x\"\n");
    for &id in &ids {
        session.parse(id);
    }
    assert_eq!(
        session.runs(Query::Parse),
        5,
        "editing one file must re-parse exactly one file"
    );
}

#[test]
fn a_module_name_comes_from_its_path() {
    let mut session = Session::new();

    let nested = session.set_file("chapters/forest.vela", "");
    assert_eq!(
        session.module_of(nested).map(|name| name.as_str()),
        Some("chapters.forest")
    );

    let top = session.set_file("main.vela", "");
    assert_eq!(session.module_of(top).map(|n| n.as_str()), Some("main"));

    // A file that is not `.vela` is not a module, so a stray asset cannot shadow one.
    let asset = session.set_file("art/forest.png", "");
    assert!(session.module_of(asset).is_none());
}

#[test]
fn a_module_name_exposes_its_segments() {
    let mut session = Session::new();
    let file = session.set_file("a/b/c.vela", "");
    let module = session.module_of(file).expect("a module");

    assert_eq!(module.segments().collect::<Vec<_>>(), vec!["a", "b", "c"]);
    assert_eq!(module.last_segment(), "c");
    assert_eq!(module.to_string(), "a.b.c");
}

#[test]
fn editing_a_file_keeps_its_id() {
    let mut session = Session::new();

    let first = session.set_file("main.vela", "label a:\n");
    let second = session.set_file("main.vela", "label b:\n");

    // A fresh id per edit would silently drop the whole cache, which is the failure this
    // guards against: it looks correct and is merely slow.
    assert_eq!(first, second);
}

#[test]
fn diagnostics_are_collected_across_files_in_order() {
    let mut session = Session::new();
    session.set_file("a.vela", "label a:\n    var x = \n    return\n");
    session.set_file("b.vela", "!!!\n");

    let diagnostics = session.diagnostics();
    assert_eq!(diagnostics.len(), 2, "{diagnostics:#?}");

    // File order, then position: the order a reader sees them in.
    assert_eq!(diagnostics[0].code.as_str(), "E1003");
    assert_eq!(diagnostics[1].code.as_str(), "E1001");
}

#[test]
fn a_changed_file_reports_its_new_diagnostics() {
    let mut session = Session::new();
    session.set_file("main.vela", "label a:\n    \"ok\"\n    return\n");
    assert!(session.diagnostics().is_empty());

    session.set_file("main.vela", "label a:\n    var x = \n    return\n");
    let diagnostics = session.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    assert_eq!(diagnostics[0].code.as_str(), "E1003");

    // And fixing it clears them again, so the cache is not serving a stale tree.
    session.set_file("main.vela", "label a:\n    \"ok again\"\n    return\n");
    assert!(session.diagnostics().is_empty());
}

#[test]
fn an_empty_session_has_nothing_to_report() {
    let mut session = Session::new();
    assert!(session.diagnostics().is_empty());
    assert_eq!(session.runs(Query::Parse), 0);
}

fn codes(diagnostics: &[vela_diag::Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect()
}

#[test]
fn an_undefined_label_is_reported() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "label a:\n    jump nope\nlabel b:\n    return\n",
    );

    assert_eq!(codes(&session.diagnostics()), vec!["E5003"]);
}

#[test]
fn a_clean_project_reports_nothing() {
    let mut session = Session::new();
    session.set_file("main.vela", "label a:\n    jump b\nlabel b:\n    return\n");
    session.set_file("lib.vela", "label helper:\n    return\n");

    assert!(session.diagnostics().is_empty());
}

#[test]
fn checking_is_cached_across_passes() {
    let mut session = Session::new();
    session.set_file("main.vela", "label a:\n    jump b\nlabel b:\n    return\n");

    session.diagnostics();
    assert_eq!(session.runs(Query::Check), 1);

    session.diagnostics();
    assert_eq!(
        session.runs(Query::Check),
        1,
        "a second pass must not re-check"
    );
}

#[test]
fn editing_one_module_re_checks_only_that_module() {
    // The M2 exit criterion, now at check granularity rather than parse granularity: 200
    // labels across four modules, and an edit re-checks one.
    let mut session = Session::new();
    let mut ids = Vec::new();
    for module in ["ch1", "ch2", "ch3", "ch4"] {
        let mut source = String::new();
        for index in 0..50 {
            source.push_str(&format!("label {module}_scene_{index}:\n    \"line\"\n"));
        }
        ids.push(session.set_file(format!("{module}.vela"), source));
    }

    session.diagnostics();
    assert_eq!(session.runs(Query::Check), 4);

    session.set_file("ch2.vela", "label edited:\n    \"x\"\n");
    session.diagnostics();

    assert_eq!(
        session.runs(Query::Check),
        5,
        "editing one module must re-check exactly one module"
    );
}

#[test]
fn editing_an_imported_module_re_checks_the_importer() {
    let mut session = Session::new();
    session.set_file("lib.vela", "label helper:\n    return\n");
    session.set_file("main.vela", "use lib\nlabel a:\n    jump lib.helper\n");

    assert!(session.diagnostics().is_empty());
    assert_eq!(session.runs(Query::Check), 2);

    // Renaming the label in `lib` has to invalidate `main`, because `main` read it. This
    // is the case a naive "invalidate the edited file" cache gets wrong.
    session.set_file("lib.vela", "label renamed:\n    return\n");
    let diagnostics = session.diagnostics();

    assert_eq!(
        session.runs(Query::Check),
        4,
        "the importer re-checks when what it imports changes"
    );
    assert_eq!(codes(&diagnostics), vec!["E5003"]);
}

#[test]
fn a_module_nothing_imports_is_unaffected_by_edits_elsewhere() {
    let mut session = Session::new();
    session.set_file("lib.vela", "label helper:\n    return\n");
    session.set_file("main.vela", "use lib\nlabel a:\n    jump lib.helper\n");
    session.diagnostics();
    assert_eq!(session.runs(Query::Check), 2);

    session.set_file("standalone.vela", "label alone:\n    return\n");
    session.diagnostics();

    assert_eq!(
        session.runs(Query::Check),
        3,
        "only the new module is checked; neither existing one is re-checked"
    );
}

/// An entry point named as a manifest writes it.
fn entry(dotted: &str) -> vela_hir::Entry {
    vela_hir::Entry::parse(dotted).unwrap_or_else(|| panic!("{dotted} should parse"))
}

#[test]
fn a_label_no_path_reaches_is_reported() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "label start:\n    jump reached\n\nlabel reached:\n    return\n\nlabel orphan:\n    return\n",
    );
    session.set_entries(vec![entry("main.start")]);

    let diagnostics = session.diagnostics();
    assert_eq!(codes(&diagnostics), vec!["W4002"], "{diagnostics:#?}");
    assert!(diagnostics[0].message.contains("orphan"));
}

#[test]
fn a_reachable_label_is_not_reported() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "label start:\n    jump middle\n\nlabel middle:\n    jump end\n\nlabel end:\n    return\n",
    );
    session.set_entries(vec![entry("main.start")]);

    assert!(session.diagnostics().is_empty());
}

#[test]
fn reachability_follows_transfers_out_of_nested_blocks() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "label start:\n\
         \x20   menu \"pick\":\n\
         \x20       \"a\":\n\
         \x20           call helper\n\
         \x20       \"b\":\n\
         \x20           return\n\
         \n\
         label helper:\n\
         \x20   return\n",
    );
    session.set_entries(vec![entry("main.start")]);

    // `helper` is only reachable through a menu choice. A walk that did not descend would
    // call it dead.
    assert!(session.diagnostics().is_empty());
}

#[test]
fn reachability_crosses_module_boundaries() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "use forest\n\nlabel start:\n    jump forest.clearing\n",
    );
    session.set_file(
        "forest.vela",
        "label clearing:\n    return\n\nlabel stranded:\n    return\n",
    );
    session.set_entries(vec![entry("main.start")]);

    let diagnostics = session.diagnostics();
    assert_eq!(codes(&diagnostics), vec!["W4002"], "{diagnostics:#?}");
    assert!(diagnostics[0].message.contains("stranded"));
}

#[test]
fn without_an_entry_point_nothing_is_reported_as_unreachable() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "label a:\n    return\n\nlabel b:\n    return\n",
    );

    // Every label looks unreachable without a root. Reporting all of them would be noise,
    // and noise is what teaches people to ignore warnings.
    assert!(session.diagnostics().is_empty());
}

#[test]
fn a_label_that_can_fall_off_its_end_is_reported() {
    let mut session = Session::new();
    session.set_file("main.vela", "label start:\n    \"hello\"\n");

    assert!(
        codes(&session.diagnostics()).contains(&"W4003".to_string()),
        "expected W4003"
    );
}

#[test]
fn an_if_without_an_else_does_not_terminate() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "label start:\n    if x:\n        return\n\nlabel end:\n    return\n",
    );

    let reported = codes(&session.diagnostics());
    assert!(reported.contains(&"W4003".to_string()), "{reported:?}");
}

#[test]
fn an_if_with_an_else_that_both_terminate_does() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "label start:\n    if x:\n        return\n    else:\n        jump end\n\nlabel end:\n    return\n",
    );

    let reported = codes(&session.diagnostics());
    assert!(!reported.contains(&"W4003".to_string()), "{reported:?}");
}

#[test]
fn a_variable_typo_is_reported_before_the_game_runs() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "default trust: int = 0\n\nlabel start:\n    var x = tru\n    return\n",
    );

    let diagnostics = session.diagnostics();
    assert_eq!(codes(&diagnostics), vec!["E2001"], "{diagnostics:#?}");
    assert!(diagnostics[0].message.contains("tru"));
}

#[test]
fn an_import_of_a_module_that_does_not_exist_is_reported() {
    let mut session = Session::new();
    session.set_file("main.vela", "use nonexistent\n\nlabel a:\n    return\n");

    assert_eq!(codes(&session.diagnostics()), vec!["E2001"]);
}

#[test]
fn a_type_error_is_reported() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "label start:\n    var x: int = \"one\"\n    return\n",
    );

    let diagnostics = session.diagnostics();
    assert_eq!(codes(&diagnostics), vec!["E3007"], "{diagnostics:#?}");
}

#[test]
fn mixing_int_and_float_is_reported() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "label start:\n    var x = 1 + 1.0\n    return\n",
    );

    assert_eq!(codes(&session.diagnostics()), vec!["E3006"]);
}

#[test]
fn a_well_typed_project_reports_nothing() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "use lib\n\ndefault score: int = 0\n\nlabel start:\n    var x = score + 1\n    \"score: {x}\"\n    jump lib.end\n",
    );
    session.set_file("lib.vela", "label end:\n    return\n");

    assert!(
        session.diagnostics().is_empty(),
        "{:?}",
        codes(&session.diagnostics())
    );
}
