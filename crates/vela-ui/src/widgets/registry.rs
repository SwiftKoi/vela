//! The widget registry.
//!
//! `CONVENTIONS.md §4.2`: adding a widget touches no core file. A registry rather than a
//! `match` on a name, because a match is a file every new widget has to edit — which is the
//! thing the registry exists to avoid, and which `check-registries` would flag anyway.
//!
//! Order is the registration order, not a hash order. The project forbids hash iteration for
//! a reason that applies here as much as anywhere: a completion list, a documentation page,
//! and a golden test all read this registry, and all three should be the same list twice.

use crate::widgets::builtin;
use crate::widgets::schema::PropDecl;

/// What a widget is for, which decides what it may contain and how it is checked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    /// Holds other widgets.
    Container,
    /// Draws something and holds nothing.
    Leaf,
    /// Can be focused and activated.
    Interactive,
}

impl Category {
    /// Whether children may appear inside one of these.
    #[must_use]
    pub fn accepts_children(self) -> bool {
        !matches!(self, Self::Leaf)
    }
}

/// A registered widget.
///
/// No `measure`/`arrange` here, unlike the trait sketch in `SCREENS.md §9`. That sketch allows
/// the passes to be traits "if that keeps files small"; here they are the solver's, shared by
/// every container and already tested as one thing. Thirteen copies of the same measurement
/// would be thirteen chances for them to disagree.
///
/// What is per-widget is the *declared surface*: the name, the props, and what it is for —
/// which is what the checker, the completion list, and the docs actually read.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Widget {
    /// The name a screen writes.
    pub name: &'static str,
    /// What it is for.
    pub category: Category,
    /// Whether it takes exactly one child.
    pub single_child: bool,
    /// The props every widget accepts.
    pub common: &'static [PropDecl],
    /// The groups of props this widget adds.
    ///
    /// A list of groups rather than one flat slice, because a linear container's props are a
    /// *superset* of a plain container's, not a replacement: `row` takes `pad` and `align` as
    /// well as `gap`. Flattening them into one slice per widget would mean copying the
    /// container props into every linear container, which is how two lists come to disagree.
    pub own: &'static [&'static [PropDecl]],
}

impl Widget {
    /// Whether this widget accepts a prop.
    ///
    /// This is the question `E5006` ("wrong prop") answers, and it is answered in one place so
    /// that the checker and the completion list cannot disagree about it.
    #[must_use]
    pub fn accepts(&self, prop: &str) -> bool {
        self.props().iter().any(|decl| decl.name == prop)
    }

    /// The prop names, for a suggestion.
    #[must_use]
    pub fn prop_names(&self) -> Vec<&'static str> {
        self.props().iter().map(|decl| decl.name).collect()
    }

    /// Every prop, shared first, in declaration order.
    #[must_use]
    pub fn props(&self) -> Vec<&PropDecl> {
        self.common
            .iter()
            .chain(self.own.iter().flat_map(|group| group.iter()))
            .collect()
    }

    /// One sentence: what this widget is, and what it takes.
    ///
    /// The same sentence `vela doc` prints and a hover shows, so the reference and the editor cannot
    /// describe a widget differently. It lives here rather than in either of them because the schema
    /// is what knows the answer, and the docs' promise is that there is one place a prop or a widget
    /// is defined.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut sentence = match self.category {
            Category::Container => String::from("It is a container"),
            Category::Leaf => String::from("It draws, and holds nothing"),
            Category::Interactive => String::from("It can be focused"),
        };
        if self.single_child {
            sentence.push_str(", and takes exactly one child");
        } else if self.category.accepts_children() {
            sentence.push_str(", and takes children");
        }
        sentence.push('.');
        sentence
    }
}

/// The widgets a project can write.
#[derive(Clone, Debug)]
pub struct WidgetRegistry {
    widgets: Vec<Widget>,
}

impl Default for WidgetRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

impl WidgetRegistry {
    /// The default set, with no plugins.
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            widgets: builtin::ALL.to_vec(),
        }
    }

    /// An empty registry, for a project that wants to start from nothing.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            widgets: Vec::new(),
        }
    }

    /// Adds a widget, replacing any of the same name.
    ///
    /// Replacement rather than an error: a project overriding `text` with its own is a
    /// legitimate thing to want, and a duplicate is the *only* way to express it. Registration
    /// order is preserved for a replacement so a style override does not reorder the list.
    pub fn register(&mut self, widget: Widget) {
        if let Some(existing) = self.widgets.iter_mut().find(|w| w.name == widget.name) {
            *existing = widget;
            return;
        }
        self.widgets.push(widget);
    }

    /// Looks a widget up by the name a screen writes.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Widget> {
        self.widgets.iter().find(|widget| widget.name == name)
    }

    /// Every name, in registration order.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.widgets.iter().map(|widget| widget.name).collect()
    }

    /// Every widget, in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &Widget> {
        self.widgets.iter()
    }

    /// How many are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.widgets.len()
    }

    /// Whether none are.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.widgets.is_empty()
    }

    /// The closest registered name to `name`, for a `did you mean`.
    ///
    /// The same fuzzy match the label resolver uses, and for the same reason: a misspelled
    /// widget is the first thing anyone writes, and knowing *where* it is wrong without
    /// knowing what it should be turns a fix into a hunt.
    #[must_use]
    pub fn closest(&self, name: &str) -> Option<&'static str> {
        self.names()
            .into_iter()
            .map(|candidate| (vela_diag::edit_distance(name, candidate), candidate))
            .filter(|(distance, _)| *distance <= (name.chars().count() / 3).clamp(1, 3))
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, candidate)| candidate)
    }
}
