//! What the pass could not translate, one entry per reason rather than per variable.
//!
//! Grouped because the sample has ninety-odd variables in these buckets, and ninety lines saying the
//! same thing is a report nobody reads: what a person needs is *which* styles are affected and
//! *why*, with the count as the evidence.

use std::collections::BTreeMap;

use super::theme::theme_name;
use super::values::Value;
use crate::report::Report;

/// What the pass could not translate, gathered by kind so that each kind gets its own reason.
#[derive(Default)]
pub(super) struct Leftover<'a> {
    /// Style name → the properties a Vela style does not carry.
    pub(super) placement: BTreeMap<String, Vec<String>>,
    /// Palette entries nothing in the project reads.
    pub(super) unused: Vec<&'a super::values::Variable>,
    /// Colours whose alpha a Vela theme cannot hold: the line, the name, and the alpha.
    pub(super) faded: Vec<(u32, String, u8)>,
    /// Variables whose name has no style group at all (`gui.language = "unicode"`).
    pub(super) loose: Vec<&'a super::values::Variable>,
    /// Variables holding a picture a screen places rather than a style paints.
    pub(super) pictures: Vec<&'a super::values::Variable>,
}

/// Writes every leftover into the migration's report.
pub(super) fn report_leftovers(
    project: &str,
    left: &Leftover<'_>,
    variables: &[super::values::Variable],
    report: &mut Report,
) {
    for (line, name, alpha) in &left.faded {
        report.push(
            project,
            *line,
            &format!("gui.rpy: `gui.{name}` is only 0x{alpha:02x} opaque"),
            "a Vela theme colour is opaque — `0xRRGGBB`, and the painter refuses a longer value. \
             The colour is kept; its alpha is not carried over, so it draws solid",
        );
    }
    for variable in &left.loose {
        report.push(
            project,
            variable.line,
            &format!(
                "gui.rpy: `gui.{}` = {}",
                variable.name,
                written(&variable.value)
            ),
            "this variable names no style and no screen reads it as a value, so there is nothing \
             in Vela for it to become",
        );
    }
    for variable in &left.pictures {
        report.push(
            project,
            variable.line,
            &format!("gui.rpy: `gui.{}`", variable.name),
            "this is a picture a screen places, not a style: a migrated screen adds it with `add` \
             or a widget's `background`, and the file it names is in the assets this migration \
             copies",
        );
    }
    for (group, keys) in &left.placement {
        report.push(
            project,
            line_of(variables, &format!("{group}_")),
            &format!(
                "gui.rpy: the `{group}` style has {} placement {}: {}",
                keys.len(),
                if keys.len() == 1 {
                    "property"
                } else {
                    "properties"
                },
                keys.join(", ")
            ),
            format!(
                "a Vela style paints, it does not place: these are reported rather than written \
                 into `{}`, and a migrated screen places the widgets that use the `{group}` style",
                theme_name(project)
            ),
        );
    }
    if let Some(first) = left.unused.first() {
        let names: Vec<&str> = left
            .unused
            .iter()
            .map(|variable| variable.name.as_str())
            .collect();
        report.push(
            project,
            first.line,
            &format!(
                "gui.rpy: {} value(s) nothing reads: {}",
                left.unused.len(),
                names.join(", ")
            ),
            "no screen in this project reads them and no `properties` splat covers them. They are \
             kept as theme entries, and nothing in the migrated project uses them yet",
        );
    }
}

/// A variable's value, as the report quotes it.
fn written(value: &Value) -> String {
    match value {
        Value::Colour { rgb, alpha } if *alpha != 255 => format!("\"#{rgb:06x}{alpha:02x}\""),
        Value::Colour { rgb, .. } => format!("\"#{rgb:06x}\""),
        Value::Number(number) => number.clone(),
        Value::Face(face) => format!("\"{face}\""),
        Value::Text(text) => format!("\"{text}\""),
        Value::Other => "an expression".to_string(),
    }
}

/// The first line of any variable whose name starts with a prefix.
fn line_of(variables: &[super::values::Variable], prefix: &str) -> u32 {
    variables
        .iter()
        .find(|variable| variable.name.starts_with(prefix))
        .map_or(1, |variable| variable.line)
}
