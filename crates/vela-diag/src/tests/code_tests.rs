use crate::{Code, Severity};

/// Every code seeded into the registry, with its declared severity.
///
/// This test is what `check-diag-codes` looks for: it is the reason a code cannot be
/// registered without being exercised.
const SEEDED: &[(&str, Severity)] = &[
    ("E0001", Severity::Error),
    ("E0003", Severity::Error),
    ("E5003", Severity::Error),
    ("E7301", Severity::Error),
    ("W4002", Severity::Warning),
];

#[test]
fn registered_codes_resolve_with_their_declared_severity() {
    for (name, severity) in SEEDED {
        let code = Code::new(name).unwrap_or_else(|| panic!("{name} should be registered"));
        assert_eq!(code.as_str(), *name);
        assert_eq!(code.severity(), *severity, "{name} severity");
        assert!(!code.title().is_empty(), "{name} needs a title");
    }
}

#[test]
fn severity_follows_the_code_letter_not_the_call_site() {
    // A warning code stays a warning wherever it is used; there is no way to construct
    // an error-typed `W4002`.
    let warning = Code::new("W4002").unwrap();
    assert_eq!(warning.severity(), Severity::Warning);
    assert_eq!(warning.severity().as_str(), "warning");
}

#[test]
fn unregistered_and_malformed_codes_are_rejected() {
    // Not in the registry, even though it is well-formed.
    assert!(Code::new("E9999").is_none());
    // Wrong letter.
    assert!(Code::new("X0001").is_none());
    // Too short.
    assert!(Code::new("E001").is_none());
    // Not digits.
    assert!(Code::new("E00AB").is_none());
    // Empty.
    assert!(Code::new("").is_none());
}

#[test]
fn display_and_debug_agree_with_as_str() {
    let code = Code::new("E5003").unwrap();
    assert_eq!(format!("{code}"), "E5003");
    assert_eq!(format!("{code:?}"), "E5003");
}
