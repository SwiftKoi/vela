//! The input a screen declares: what it answers, and when it acts on its own.
//!
//! `SCREENS.md §2.3`. A `key` binds a semantic action the host delivers, and a `timer` names a delay —
//! neither places a widget, so neither is part of the tree [`build`] produces. They are still part of
//! the *screen*, and they follow composition exactly as the tree does: a `use`d screen's bindings are
//! live while it is drawn, a block handed to a `use` binds what it wrote, and a binding inside a widget
//! or a conditional arm is as live as one at the top level.
//!
//! Collected separately rather than threaded through [`build`] for a dull reason with a real
//! consequence: the builder is already at the argument limit, and its output is a [`Node`]. A screen's
//! answer to an input is not a node.
//!
//! [`build`]: super::build::build
//! [`Node`]: crate::tree::Node

use vela_syntax::ScreenLine;

use super::compose::Compose;
use super::props::action_from;
use crate::eval::{Args, Ctx, number};
use crate::screens::{KeyBinding, Timer};

/// The bindings a screen body declares, in the order they are written.
///
/// An action nothing resolves — a call that is not in the registry, or a name that is not a parameter —
/// binds nothing rather than binding an empty action: the checker has already reported the name, and a
/// binding that fired nothing would look like it worked.
#[must_use]
pub fn bindings(body: &[ScreenLine], ctx: &Ctx<'_>, args: &Args) -> (Vec<KeyBinding>, Vec<Timer>) {
    let mut keys = Vec::new();
    let mut timers = Vec::new();
    collect(body, ctx, args, Compose::default(), &mut keys, &mut timers);
    (keys, timers)
}

/// Walks the body, following composition, and collects the bindings.
fn collect(
    lines: &[ScreenLine],
    ctx: &Ctx<'_>,
    args: &Args,
    compose: Compose<'_>,
    keys: &mut Vec<KeyBinding>,
    timers: &mut Vec<Timer>,
) {
    for line in lines {
        match line {
            ScreenLine::Key { name, action, .. } => {
                if let Some(action) = action_from(action, args) {
                    keys.push(KeyBinding {
                        name: name.clone(),
                        action,
                    });
                }
            }
            ScreenLine::Timer {
                seconds,
                action,
                repeat,
                ..
            } => {
                // Both halves have to resolve: a deadline nothing measures and an action nothing
                // names would be a timer that is never due and does nothing.
                let (Some(seconds), Some(action)) = (number(seconds), action_from(action, args))
                else {
                    continue;
                };
                timers.push(Timer {
                    seconds,
                    action,
                    repeat: *repeat,
                });
            }
            ScreenLine::If { .. } => {
                // The arm that *draws*, which is the arm whose input is live — the same decision the
                // tree makes, through the same function. A binding in an arm nobody took answers
                // nothing, exactly as the widgets in it draw nothing.
                if let Some(arm) = super::build::arm_of(line, args) {
                    collect(arm, ctx, args, compose, keys, timers);
                }
            }
            ScreenLine::Use {
                name,
                args: call,
                body,
                ..
            } => {
                // The block is this screen's own code, so its bindings are this screen's.
                collect(body, ctx, args, compose, keys, timers);
                if let Some((callee, bound, inner)) =
                    compose.expand(ctx.screens, name, call, args, body)
                {
                    collect(&callee.body, ctx, &bound, inner, keys, timers);
                }
            }
            ScreenLine::Transclude { .. } => {
                // Placed where the caller wrote it, and the caller's scope is what its names read.
                if let Some((lines, pane_args)) = compose.block() {
                    collect(lines, ctx, pane_args, Compose::default(), keys, timers);
                }
            }
            ScreenLine::Node(node) => collect(&node.children, ctx, args, compose, keys, timers),
            // Neither declares an input: a layer is where the screen draws, a prefix is how it looks.
            ScreenLine::Layer { .. } | ScreenLine::StylePrefix { .. } => {}
        }
    }
}
