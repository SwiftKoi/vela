//! What Ren'Py's words become: the widget, prop, action and system tables.
//!
//! Every table here is a list of *names*, and the code that reads them is elsewhere: a widget name
//! goes through `widget_name`, a prop through `prop_line`, and a system through the walk. They live
//! together because they answer one question — what does this word become? — and a reader checking
//! whether a Ren'Py construct is handled starts here.

/// Widgets whose Vela name differs.
///
/// `fixed` is a `stack` rather than an `absolute`: a Ren'Py `fixed` holds as many children as it
/// likes and places them by `xpos`/`ypos`, and Vela's `absolute` holds *one* (`single_child` in the
/// registry, and `E5004` when given two). `stack` is the container that holds many, which is the
/// half of `fixed` a migration can keep — the coordinates are reported either way.
pub(super) const WIDGETS: &[(&str, &str)] = &[
    ("hbox", "row"),
    ("vbox", "column"),
    ("add", "image"),
    ("null", "spacer"),
    ("fixed", "stack"),
    ("frame", "box"),
    ("window", "box"),
    ("label", "text"),
    // `vpgrid` is a grid inside a viewport, and the grid is what scrolls (`SCREENS.md §3.2`).
    ("vpgrid", "viewport"),
];

/// Widgets that are the same word in both languages.
pub(super) const SAME: &[&str] = &[
    "text", "button", "bar", "input", "grid", "viewport", "image", "spacer",
];

/// Widgets that take no leading content, so every token on their line is a prop.
///
/// The list is Vela's names — the *lowered* ones, since a Ren'Py `vbox` is a `column` by the time
/// anything asks.
pub(super) const CAPTIONLESS: &[&str] = &[
    "row", "column", "box", "stack", "absolute", "grid", "flow", "viewport", "vpgrid", "button",
    "bar", "input", "slider", "spacer",
];

/// Widgets that are nothing without their content, so one whose picture was reported goes too.
pub(super) const NEEDS_CONTENT: &[&str] = &["text", "image"];

/// Props whose Vela name differs.
pub(super) const PROPS: &[(&str, &str)] = &[
    ("spacing", "gap"),
    ("padding", "pad"),
    ("cols", "columns"),
    ("yinitial", "initial"),
];

/// Props a Vela layout does not have: a pixel coordinate, or one axis of a position.
pub(super) const PLACEMENT: &[&str] = &[
    "xpos",
    "ypos",
    "xoffset",
    "yoffset",
    "xsize",
    "ysize",
    "xminimum",
    "yminimum",
    "xmaximum",
    "ymaximum",
    "xanchor",
    "yanchor",
    "xcenter",
    "ycenter",
    "xpadding",
    "ypadding",
    "left_margin",
    "right_margin",
    "top_margin",
    "bottom_margin",
    "xspacing",
    "yspacing",
    "min_width",
    "min_height",
    "xfill",
    "yfill",
    "textalign",
    "text_xalign",
    "text_yalign",
    "xalign",
    "yalign",
];

/// Props a widget takes that are neither painted nor placed.
pub(super) const WIDGET_PROPS: &[&str] = &[
    "value",
    "initial",
    "enable_if",
    "id",
    "text",
    "mouse",
    "style",
];

/// Actions whose Vela name differs. Every other action is Ren'Py's name in lower snake case, which
/// is what `crates/vela-ui/src/actions/builtin.rs` registered — the two vocabularies were built
/// from the same screens.
pub(super) const ACTIONS: &[(&str, &str)] =
    &[("ShowMenu", "open_screen"), ("Return", "close_screen")];

/// Every action Vela registers, by Vela's name.
pub(super) const KNOWN_ACTIONS: &[&str] = &[
    "call",
    "close_screen",
    "end_replay",
    "file_delete",
    "file_page",
    "file_page_next",
    "file_page_previous",
    "gamepad_calibrate",
    "hide",
    "jump",
    "language",
    "open_screen",
    "play",
    "preference",
    "quick_load",
    "quick_save",
    "quit",
    "rollback",
    "set",
    "set_screen_variable",
    "skip",
    "stop",
];

/// Statements that name a system Vela has no counterpart for, and which one.
pub(super) const SYSTEMS: &[(&str, &str)] = &[
    (
        "tag",
        "the screen stack (`tag`: one screen with a tag replaces another)",
    ),
    ("zorder", "the screen stack's draw order"),
    ("modal", "input capture, which the input map owns"),
    (
        "key",
        "the input map, which binds a *semantic action* rather than a keycode (`SCREENS.md §11`)",
    ),
    ("transform", "animation (`SCREENS.md §6` is not yet built)"),
    ("on", "animation, in a transform"),
    ("python", "a Python block"),
    ("init", "a Python block"),
    ("$", "a Python statement"),
];
