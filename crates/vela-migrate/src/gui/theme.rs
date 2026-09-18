//! Building the output: the theme, its styles, and the report of what had nowhere to go.

use std::collections::{BTreeMap, BTreeSet};

use super::names::{Fate, classify, font_token, token};
use super::report::{Leftover, report_leftovers};
use super::values::{Value, Variable, colours, fonts};
use crate::report::Report;

/// The theme being built.
pub(super) struct Theme<'a> {
    /// The theme's own lines: its colours and font tokens.
    tokens: String,
    /// Style name → the settings under it.
    styles: BTreeMap<String, Vec<(String, String)>>,
    /// What could not be translated.
    leftover: Leftover<'a>,
}

impl<'a> Theme<'a> {
    /// Sorts every variable into the theme, a style, or the report.
    pub(super) fn build(
        variables: &'a [Variable],
        groups: &BTreeSet<String>,
        live: &BTreeSet<String>,
    ) -> Self {
        let colour_of = colours(variables);
        let font_of = fonts(variables);
        let mut theme = Self {
            tokens: String::new(),
            styles: BTreeMap::new(),
            leftover: Leftover::default(),
        };
        for variable in variables {
            theme.add(variable, groups, live, &colour_of, &font_of);
        }
        theme
    }

    /// Files one variable.
    fn add(
        &mut self,
        variable: &'a Variable,
        groups: &BTreeSet<String>,
        live: &BTreeSet<String>,
        colour_of: &BTreeMap<String, u32>,
        font_of: &BTreeMap<String, String>,
    ) {
        // Every colour is a palette entry and every face a font token, whether or not a style also
        // carries it: a theme is where a look's colours live, and a style that repeats one says
        // `theme.<token>` rather than the number.
        if let Value::Colour { rgb, alpha } = variable.value {
            self.tokens.push_str(&format!(
                "    color {} = 0x{rgb:06x}\n",
                token(&variable.name, "_color")
            ));
            if alpha != 255 {
                self.leftover
                    .faded
                    .push((variable.line, variable.name.clone(), alpha));
            }
        }
        if let Value::Face(face) = &variable.value {
            self.tokens.push_str(&format!(
                "    font {} = \"{face}\"\n",
                font_token(&variable.name, groups)
            ));
        }

        match classify(variable, groups, live, colour_of, font_of) {
            Fate::Setting(group, key, setting) => {
                self.styles.entry(group).or_default().push((key, setting));
            }
            Fate::Palette { read } if !read => self.leftover.unused.push(variable),
            Fate::Palette { .. } => {}
            Fate::Picture => self.leftover.pictures.push(variable),
            Fate::Placement(group, key) => {
                self.leftover.placement.entry(group).or_default().push(key);
            }
            Fate::Loose => self.leftover.loose.push(variable),
        }
    }

    /// The finished file, or `None` when there is nothing to write.
    ///
    /// A `theme` with an empty body is a parse error, and a file that holds only placement is
    /// better reported than written: it would look like the look came across when none of it did.
    pub(super) fn finish(
        mut self,
        project: &str,
        variables: &[Variable],
        report: &mut Report,
    ) -> Option<String> {
        if self.tokens.is_empty() && self.styles.is_empty() {
            report_leftovers(project, &self.leftover, variables, report);
            report.push(
                project,
                1,
                "gui.rpy",
                "the GUI variables in this file are all placement, which a Vela style does not \
                 carry, so this migration writes no theme rather than an empty one",
            );
            return None;
        }
        let source = self.render(project);
        report_leftovers(project, &self.leftover, variables, report);
        Some(source)
    }

    /// The theme and its styles, as canonical Vela.
    fn render(&mut self, project: &str) -> String {
        let mut out = format!(
            "# The look, migrated from `gui.rpy` by `vela migrate`.\n\
             #\n\
             # A Vela style paints — `color`, `background`, `size` and `font` are what it resolves —\n\
             # while placement belongs to each widget's own props. The GUI's placement variables are\n\
             # therefore not here, and `MIGRATION.md` names what was left out and why.\n\n\
             theme {}:\n",
            theme_name(project)
        );
        if self.tokens.is_empty() {
            out.push_str(
                "    # No theme tokens: every variable a screen reads is a property of a\n",
            );
            out.push_str("    # style, and those are below.\n");
        }
        out.push_str(&self.tokens);
        for (group, settings) in &self.styles {
            out.push_str(&format!("\nstyle {group}:\n"));
            let mut settings = settings.clone();
            settings.sort();
            for (key, value) in settings {
                out.push_str(&format!("    {key} = {value}\n"));
            }
        }
        out
    }
}

/// The theme's name: the project's, so a migrated theme is recognisable in a project's own files.
pub(super) fn theme_name(project: &str) -> String {
    let base = project
        .trim_end_matches(".rpy")
        .rsplit('/')
        .next()
        .unwrap_or("migrated");
    if base.is_empty() || base == "gui" {
        "migrated".to_string()
    } else {
        base.to_string()
    }
}
