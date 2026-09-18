//! What a `gui.rpy` becomes, and what it becomes a report entry instead.
//!
//! The naming convention is the whole translation (`crate::gui`), so most of these tests are about
//! the two halves of it: a variable a screen reads becomes a theme or a style, and one it does not
//! becomes an entry naming the file and the line. The boundary between them was measured rather
//! than guessed — a Vela style paints (`color`, `background`, `size`, `font`) and does not place —
//! and the test that pins it is the one about `xpos`.

use crate::Report;

/// Runs the GUI pass over one `gui.rpy`, against the screens that decide what is live.
fn skin(gui: &str, screens: &str) -> (String, Report) {
    let nodes = crate::read(gui);
    let mut report = Report::new();
    let skin = crate::gui::skin("gui.rpy", &nodes, screens, &mut report).expect("a theme");
    (skin.source, report)
}

/// The entry codes and lines of a report, for assertions about refusals.
fn entries(report: &Report) -> Vec<String> {
    report
        .entries()
        .iter()
        .map(|entry| format!("{}:{}: {}", entry.file, entry.line, entry.original))
        .collect()
}

/// A scope is a style, and a colour is a theme token the style reads.
///
/// The link between the two is the point of emitting a theme at all: `gui.name_text_color` is the
/// same colour as `gui.accent_color`, so the style says `theme.accent` rather than repeating the
/// number — change the palette and the styles follow.
#[test]
fn a_style_reads_the_theme_it_was_derived_from() {
    let (text, report) = skin(
        "define gui.name_text_color = '#cc6600'\ndefine gui.name_size = 30\n",
        "    style name:\n        properties gui.text_properties(\"name\")\n    text property:\n        color gui.name_text_color\n",
    );
    assert!(text.contains("color name_text = 0xcc6600"), "{text}");
    assert!(text.contains("style name:"), "{text}");
    assert!(text.contains("    size = 30"), "{text}");
    assert!(text.contains("    color = theme.name_text"), "{text}");
    assert!(report.is_empty(), "{:?}", entries(&report));
}

/// A colour two roles share stays a literal, because a link would have to choose one of them.
///
/// `#ffffff` is four different things in the sample's own `gui.rpy`. Resolving an `interface`
/// style's colour to whichever of them sorts first is a confident wrong answer, and a literal is
/// never wrong — so ambiguity is what decides, not order.
#[test]
fn an_ambiguous_colour_is_not_linked() {
    let (text, _) = skin(
        "define gui.interface_text_color = '#ffffff'\ndefine gui.choice_button_text_hover_color = '#ffffff'\n",
        "    style interface:\n        properties gui.text_properties(\"interface\")\n",
    );
    assert!(text.contains("    color = 0xffffff"), "{text}");
    assert!(!text.contains("theme.choice_button"), "{text}");
}

/// The splat decides which style a variable belongs to, not the underscores in its name.
///
/// `gui.button_text_hover_color` is `button`'s `hover_color`, because `gui.button_properties
/// ("button")` is what says `button` is a style — and Ren'Py puts the interaction state in the
/// identifier where `SCREENS.md §5.1` puts it in the key.
#[test]
fn the_splat_names_the_style_and_the_state_stays_in_the_key() {
    let (text, _) = skin(
        "define gui.button_text_hover_color = '#ffffff'\ndefine gui.button_text_size = 20\n",
        "    style button:\n        properties gui.button_properties(\"button\")\n",
    );
    assert!(text.contains("\nstyle button:\n"), "{text}");
    // The colour resolves to the palette entry that holds it — the same value, so the style reads
    // the theme rather than repeating the number.
    assert!(text.contains("    hover_color = theme."), "{text}");
    assert!(text.contains("    size = 20"), "{text}");
    // Not `button_text`, which is what an underscore-based split would have invented.
    assert!(!text.contains("style button_text"), "{text}");
}

/// A placement variable is reported rather than emitted, because a Vela style does not place.
///
/// Measured: a style setting `xpos` is accepted and ignored, so emitting it would produce a file
/// that looks migrated and draws in the wrong place.
#[test]
fn placement_is_reported_rather_than_emitted() {
    // A colour as well, because a file of nothing but placement produces no file at all — which
    // the last test in this file pins.
    let (text, report) = skin(
        "define gui.accent_color = '#cc6600'\ndefine gui.name_xpos = 240\ndefine gui.name_xalign = 0.0\n",
        "    style name:\n        properties gui.text_properties(\"name\")\n    text property:\n        color gui.accent_color\n",
    );
    assert!(!text.contains("xpos"), "{text}");
    assert!(!text.contains("xalign"), "{text}");
    let entries = entries(&report);
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(entries[0].contains("xpos, xalign"), "{entries:?}");
}

/// A font file becomes a face a project registers; a string that is not a font is not one.
///
/// `gui.unscrollable = "hide"` is a setting, not a face, and a rule that treated every string as a
/// font emitted `font unscrollable = "hide"` — which is why the value decides, not just the name.
#[test]
fn a_font_is_a_file_and_a_setting_is_not() {
    let (text, report) = skin(
        "define gui.name_text_font = \"DejaVuSans.ttf\"\ndefine gui.unscrollable = \"hide\"\n",
        "    style name:\n        properties gui.text_properties(\"name\")\n",
    );
    assert!(text.contains("font name = \"DejaVuSans\""), "{text}");
    assert!(!text.contains("hide"), "{text}");
    let entries = entries(&report);
    assert!(
        entries.iter().any(|entry| entry.contains("unscrollable")),
        "{entries:?}"
    );
}

/// An alpha is kept as a colour and reported, because a Vela theme colour is opaque.
///
/// Ren'Py writes `'#5555557f'`; Vela's painter refuses anything longer than `0xRRGGBB`, so the
/// colour migrates and the translucency is a thing to port by hand rather than a silent change.
#[test]
fn an_alpha_is_reported() {
    let (text, report) = skin(
        "define gui.insensitive_color = '#5555557f'\n",
        "    style insensitive:\n        properties gui.text_properties(\"insensitive\")\n",
    );
    assert!(text.contains("color insensitive = 0x555555"), "{text}");
    assert!(!text.contains("7f"), "{text}");
    let entries = entries(&report);
    assert!(entries[0].contains("insensitive"), "{entries:?}");
}

/// A palette entry nothing reads is kept and reported, rather than dropped.
///
/// `gui.hover_color` is Ren'Py's own base palette: Ren'Py's styles read it, and nothing in a Vela
/// project does yet. It stays in the theme — a palette is a palette — and the report says so.
#[test]
fn a_palette_entry_nothing_reads_is_reported() {
    let (text, report) = skin(
        "define gui.accent_color = '#cc6600'\ndefine gui.hover_color = '#e0a366'\n",
        "    text property:\n        color gui.accent_color\n",
    );
    assert!(text.contains("color accent = 0xcc6600"), "{text}");
    assert!(text.contains("color hover = 0xe0a366"), "{text}");
    let entries = entries(&report);
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(entries[0].contains("hover_color"), "{entries:?}");
    assert!(!entries[0].contains("accent_color"), "{entries:?}");
}

/// A variable that belongs to no style at all is reported as such rather than as a style's.
#[test]
fn a_name_with_no_group_is_reported_as_itself() {
    let (_, report) = skin(
        "define gui.accent_color = '#cc6600'\ndefine gui.language = \"unicode\"\n",
        "",
    );
    let entries = entries(&report);
    assert!(
        entries.iter().any(|entry| entry.contains("gui.language")),
        "{entries:?}"
    );
    assert!(
        !entries
            .iter()
            .any(|entry| entry.contains("`language` style")),
        "a value with no group is not a style: {entries:?}"
    );
}

/// A file that is all placement produces nothing rather than a `theme` with an empty body.
///
/// That is a parse error in Vela, and an empty declaration is worse than no declaration: it looks
/// like the look came across when none of it did.
#[test]
fn placement_only_produces_no_theme_at_all() {
    let nodes = crate::read("define gui.name_xpos = 240\n");
    let mut report = Report::new();
    assert!(crate::gui::skin("gui.rpy", &nodes, "", &mut report).is_none());
    assert!(!report.is_empty(), "the refusal is reported");
}

/// `gui.init` is where a project says which frame it was designed for.
#[test]
fn the_design_size_comes_from_gui_init() {
    let nodes = crate::read(
        "init python:\n    gui.init(1280, 720)\n\ndefine gui.accent_color = '#cc6600'\n",
    );
    let mut report = Report::new();
    let skin = crate::gui::skin("gui.rpy", &nodes, "", &mut report).expect("a theme");
    assert_eq!(skin.design, Some((1280, 720)));
}
