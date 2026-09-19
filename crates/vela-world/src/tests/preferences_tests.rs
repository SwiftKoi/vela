//! The settings store: what it holds, and in what order.

use crate::Value;
use crate::preferences::Preferences;

/// Settings are walked in name order, whatever order they were set in.
///
/// The order is observable — the settings file is written from this walk — so it has to be the
/// store's rather than the caller's (`CONVENTIONS.md §2.2`, enforced by `xtask check-determinism`).
#[test]
fn settings_are_walked_in_name_order() {
    let mut preferences = Preferences::new();
    preferences.set("volume", Value::Int(80));
    preferences.set("text_speed", Value::Int(30));
    preferences.set("auto_forward", Value::Bool(true));

    let names: Vec<&str> = preferences.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["auto_forward", "text_speed", "volume"]);
    assert_eq!(preferences.len(), 3);
}

/// A setting is what was set, of whatever type, and setting one again replaces it.
///
/// The type is the value's rather than the store's: what a *name* means is the interface's business
/// (`SCREENS.md §7`), and a build that has never met a name must still be able to hold it.
#[test]
fn a_setting_is_what_was_set() {
    let mut preferences = Preferences::new();
    assert!(preferences.is_empty(), "a player who has chosen nothing");
    assert_eq!(preferences.get("text_speed"), None);

    preferences.set("text_speed", Value::Int(30));
    assert_eq!(preferences.get("text_speed"), Some(&Value::Int(30)));

    preferences.set("text_speed", Value::Int(45));
    assert_eq!(
        preferences.len(),
        1,
        "a second set replaces rather than adds"
    );
    assert_eq!(preferences.get("text_speed"), Some(&Value::Int(45)));
}
