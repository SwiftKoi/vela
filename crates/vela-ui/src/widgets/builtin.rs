//! The default widget set.
//!
//! `SCREENS.md §3` calls this the *default* set rather than the only one, and the difference
//! is load-bearing: every entry here is an ordinary registry entry, so a plugin's widget is
//! registered the same way and the layout pass cannot tell them apart.
//!
//! A table rather than one file per widget, which `CONVENTIONS.md §4.2` also allows — that
//! row exists for widgets with behaviour of their own. These declare a name, the props they
//! accept, and how the layout treats them; the *behaviour* is the solver's, shared and
//! already tested. A widget that grows real behaviour of its own earns its own file.

use crate::widgets::schema::{COMMON, CONTAINER, INTERACTIVE, PropDecl, PropType};
use crate::widgets::{Category, Widget};

/// Props that only a linear container accepts.
const LINEAR: &[PropDecl] = &[PropDecl {
    name: "gap",
    ty: PropType::Number,
    required: false,
    doc: "Space between children.",
}];

/// Props a grid accepts.
const GRID: &[PropDecl] = &[PropDecl {
    name: "columns",
    ty: PropType::Number,
    required: true,
    doc: "How many columns before wrapping to the next row.",
}];

/// Props a viewport accepts.
const VIEWPORT: &[PropDecl] = &[PropDecl {
    name: "initial",
    ty: PropType::Number,
    required: false,
    doc: "Where the window starts, as a fraction of the travel: `0` the top, `1` the bottom.",
}];

/// The widgets every project starts with.
pub const ALL: &[Widget] = &[
    Widget {
        name: "box",
        category: Category::Container,
        single_child: true,
        common: COMMON,
        own: &[CONTAINER],
    },
    Widget {
        name: "row",
        category: Category::Container,
        single_child: false,
        common: COMMON,
        own: &[CONTAINER, LINEAR],
    },
    Widget {
        name: "column",
        category: Category::Container,
        single_child: false,
        common: COMMON,
        own: &[CONTAINER, LINEAR],
    },
    Widget {
        name: "stack",
        category: Category::Container,
        single_child: false,
        common: COMMON,
        own: &[],
    },
    Widget {
        name: "flow",
        category: Category::Container,
        single_child: false,
        common: COMMON,
        own: &[CONTAINER, LINEAR],
    },
    Widget {
        name: "grid",
        category: Category::Container,
        single_child: false,
        common: COMMON,
        // `LINEAR` because the solver reads a grid's `gap` (`layout/containers.rs`'s `grid_tracks`)
        // and `SCREENS.md §4.2` lists `grid` among the containers that take one. The registry was the
        // one of the three that disagreed, which a migration's `grid ... spacing 10` is what found.
        own: &[CONTAINER, GRID, LINEAR],
    },
    Widget {
        name: "absolute",
        category: Category::Container,
        single_child: true,
        common: COMMON,
        own: &[],
    },
    Widget {
        name: "viewport",
        category: Category::Container,
        single_child: true,
        common: COMMON,
        own: &[CONTAINER, VIEWPORT],
    },
    Widget {
        name: "text",
        category: Category::Leaf,
        single_child: false,
        common: COMMON,
        own: &[],
    },
    Widget {
        name: "image",
        category: Category::Leaf,
        single_child: false,
        common: COMMON,
        own: &[],
    },
    Widget {
        name: "spacer",
        category: Category::Leaf,
        single_child: false,
        common: COMMON,
        own: &[],
    },
    Widget {
        name: "button",
        category: Category::Interactive,
        single_child: true,
        common: COMMON,
        own: &[CONTAINER, INTERACTIVE],
    },
    Widget {
        name: "bar",
        category: Category::Interactive,
        single_child: false,
        common: COMMON,
        own: &[INTERACTIVE],
    },
    Widget {
        name: "input",
        category: Category::Interactive,
        single_child: false,
        common: COMMON,
        own: &[INTERACTIVE],
    },
];
