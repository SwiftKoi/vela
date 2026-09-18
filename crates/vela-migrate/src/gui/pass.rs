//! The entry point: parse, decide, build, report.

use super::{names, theme, values};
use crate::report::Report;
use crate::rpy::Node;

/// What the GUI pass produced.
pub struct Skin {
    /// The theme and its styles, as a Vela source file.
    pub source: String,
    /// The frame `gui.init(width, height)` declared, when it declares one.
    pub design: Option<(u32, u32)>,
}

/// Translates a `gui.rpy` into a theme, or `None` when the file declares no `gui.` variables.
///
/// `screens` is the project's screen-language source, and it is what decides whether a variable is
/// *live*: one a screen reads, or that a `gui.<x>_properties("group")` splat covers, is
/// translated; one nothing reads is reported rather than dropped quietly.
pub fn skin(project: &str, nodes: &[Node], screens: &str, report: &mut Report) -> Option<Skin> {
    let variables = values::variables(nodes);
    if variables.is_empty() {
        return None;
    }
    let groups = names::splat_groups(screens);
    let live = names::live_names(screens, &groups, &variables);
    let built = theme::Theme::build(&variables, &groups, &live);

    Some(Skin {
        source: built.finish(project, &variables, report)?,
        design: values::design(nodes),
    })
}
