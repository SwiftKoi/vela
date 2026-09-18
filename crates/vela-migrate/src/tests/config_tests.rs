//! What an `options.rpy` becomes: a work list rather than a file.
//!
//! Nothing here is translated except the project's name, which `vela.toml` carries — so the pass's
//! whole product is the *report*, and the report is what these tests are about. One entry per kind of
//! knob, naming every declaration of that kind: "engine configuration, 220 lines, port by hand" was
//! one entry that told a person nothing they could start on.

use crate::Report;

/// Reports one `options.rpy`, and returns the entries as `names: reason`.
fn report(text: &str) -> Vec<String> {
    let nodes = crate::read(text);
    let mut report = Report::new();
    assert!(
        crate::config::report("options.rpy", &nodes, &mut report),
        "the fixture is configuration"
    );
    report
        .entries()
        .iter()
        .map(|entry| format!("{}: {}", entry.original, entry.reason))
        .collect()
}

/// The name becomes `vela.toml`'s, so it is *not* reported: one question, one answer.
#[test]
fn the_project_name_is_translated_rather_than_reported() {
    let entries = report("define config.name = _(\"The Question\")\n");
    assert!(entries.is_empty(), "{entries:?}");
}

/// A kind of knob is one entry naming all of its declarations, and the reason is the kind's.
#[test]
fn a_kind_of_knob_is_one_entry_naming_its_declarations() {
    let entries = report(
        "define config.enter_transition = dissolve\n\
         define config.exit_transition = dissolve\n\
         define config.window_icon = \"gui/window_icon.png\"\n\
         define config.console = True\n",
    );
    assert_eq!(entries.len(), 2, "{entries:?}");
    let transitions = entries
        .iter()
        .find(|entry| entry.contains("enter_transition"))
        .expect("the transitions are one entry");
    assert!(transitions.contains("exit_transition"), "{transitions}");
    assert!(!transitions.contains("window_icon"), "{transitions}");
    assert!(transitions.contains("animation"), "{transitions}");
    let window = entries
        .iter()
        .find(|entry| entry.contains("window_icon"))
        .expect("the window's knobs are one entry");
    assert!(window.contains("console"), "{window}");
}

/// A `gui.` variable outside `gui.rpy` is reported, because the theme is read from that one file.
#[test]
fn a_gui_variable_outside_gui_rpy_is_reported() {
    let entries = report("define gui.about = \"Nobody\"\n");
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(entries[0].contains("gui.about"), "{entries:?}");
    assert!(entries[0].contains("`gui.rpy`"), "{entries:?}");
}

/// A preference default is the settings store's, which is M12.2's, and the entry says so.
#[test]
fn a_preference_default_names_the_store_that_owns_it() {
    let entries = report("default preferences.text_cps = 0\n");
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(entries[0].contains("preferences.text_cps"), "{entries:?}");
    assert!(entries[0].contains("M12.2"), "{entries:?}");
}

/// A block of Python is named by its own line: "a Python block" is not something to start on.
#[test]
fn a_python_block_is_named_by_its_line() {
    let entries = report("init python:\n    build.classify('**~', None)\n");
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(entries[0].contains("`init python`"), "{entries:?}");
}

/// A file with nothing of the sort in it is not configuration, so the pass leaves it alone.
#[test]
fn a_file_with_no_configuration_is_not_one() {
    let nodes = crate::read("label start:\n    \"One.\"\n    return\n");
    let mut report = Report::new();
    assert!(!crate::config::report("script.rpy", &nodes, &mut report));
    assert!(report.is_empty());
}
