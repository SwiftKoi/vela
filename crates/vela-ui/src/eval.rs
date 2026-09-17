//! Reading a screen's arguments and the expressions in its body.
//!
//! A `screen` is a function of its arguments (`SCREENS.md §2`), and this is the tiny evaluator
//! that decides what those arguments mean. It understands a conditional, a value, and a string
//! — exactly what a screen body needs to choose its *shape* — and nothing else. Arithmetic,
//! calls, and bindings into `World` state belong to the reactivity work; growing this into a
//! second VM would be the wrong answer to "the screen reads state".
//!
//! Kept apart from [`crate::instantiate`] so that the decision "what does this expression
//! mean" does not live in the same file as "what node does this line build".

use vela_render::Color;
use vela_syntax::{BinOp, Expr, ScreenDecl, StrPart, StyleDecl, UnOp};

use crate::actions::Action;
use crate::images::ImageTable;
use crate::props::Anchor;
use crate::theme::{Fonts, Palette, string_literal};
use crate::tree::{Paint, State};
use crate::widgets::WidgetRegistry;

// Re-exported because this is the module that *reads* them: every caller of the evaluator already
// imports `Args` and `Value` from here, and moving the types without moving the path would be churn in
// a dozen files for no reader's benefit.
pub use crate::value::{Args, ScreenState, Value};

/// What a screen body reads when it is evaluated.
pub struct Ctx<'a> {
    /// The widget vocabulary, for telling a widget from a prop.
    pub registry: &'a WidgetRegistry,
    /// The active theme's colours.
    pub palette: &'a Palette,
    /// The active theme's font tokens (`SCREENS.md §5`).
    pub fonts: &'a Fonts,
    /// The pictures a name resolves to (`SCREENS.md §3`).
    ///
    /// Here rather than at paint time alone because a picture's *size* is a layout input: a screen that
    /// measured it later would lay out around nothing and then draw over its neighbours.
    pub images: &'a ImageTable,
    /// The declared styles, for `style = ...`.
    pub styles: &'a [StyleDecl],
    /// The file's screens, so a `use` can find the one it names (`SCREENS.md §2`).
    ///
    /// Screens resolve within the file that declares them, like styles — so this is one file's worth
    /// and not a project-wide table.
    pub screens: &'a [&'a ScreenDecl],
}

/// A numeric literal.
pub(crate) fn number(expr: &Expr) -> Option<f32> {
    match expr {
        Expr::Int { value, .. } => Some(*value as f32),
        Expr::Float { value, .. } => Some(*value as f32),
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => number(operand).map(|number| -number),
        Expr::Paren { inner, .. } => number(inner),
        _ => None,
    }
}

/// An anchor named by an expression, when that expression is a bare word.
pub(crate) fn anchor_of(expr: &Expr) -> Option<Anchor> {
    let Expr::Name { name, .. } = expr else {
        return None;
    };
    anchor_word(name)
}

/// An anchor named by a bare word.
pub(crate) fn anchor_word(name: &str) -> Option<Anchor> {
    Some(match name {
        "top_left" => Anchor::TopLeft,
        "top" => Anchor::Top,
        "top_right" => Anchor::TopRight,
        "left" => Anchor::Left,
        "center" | "centre" => Anchor::Center,
        "right" => Anchor::Right,
        "bottom_left" => Anchor::BottomLeft,
        "bottom" => Anchor::Bottom,
        "bottom_right" => Anchor::BottomRight,
        "stretch" => Anchor::Stretch,
        _ => return None,
    })
}

/// A bare name used as a value, e.g. `style = body`.
pub(crate) fn name_of(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Name { name, .. } => Some(name),
        _ => None,
    }
}

/// An action call written as a value, e.g. `open_screen(settings)`.
pub(crate) fn action_of(expr: &Expr, values: &Args) -> Option<Action> {
    let Expr::Call { callee, args, .. } = expr else {
        return None;
    };
    let Expr::Name { name, .. } = callee.as_ref() else {
        return None;
    };
    Some(Action::new(
        name.clone(),
        args.iter().map(|arg| action_arg(arg, values)).collect(),
    ))
}

/// One action argument: the value the screen holds, or the name it wrote.
///
/// A name that is *in scope* is the value bound to it, and everything else is what was written — a
/// string, a number, a dotted path. That one rule is what makes an action mean the same thing however
/// its argument was spelled: `open_screen(settings)` passes the name `settings` for the runtime to
/// resolve, `set_screen_variable(device, "mouse")` passes the string `mouse`, and
/// `set_screen_variable(device, item.kind)` passes whatever that element's field holds rather than the
/// four words `item.kind`. The head of a path is what tells the two apart, which is the question
/// `image_of` asks and answers the same way.
fn action_arg(expr: &Expr, values: &Args) -> Value {
    let bound = match expr {
        Expr::Name { name, .. } => values.get(name).is_some(),
        Expr::Field { base, .. } => {
            matches!(base.as_ref(), Expr::Name { name, .. } if values.get(name).is_some())
        }
        _ => false,
    };
    if bound {
        return value_of(expr, values);
    }
    match expr {
        Expr::Int { value, .. } => Value::Num(*value as f64),
        Expr::Float { value, .. } => Value::Num(*value),
        Expr::Bool { value, .. } => Value::Bool(*value),
        Expr::None { .. } => Value::None,
        _ => Value::Str(render_arg(expr)),
    }
}

/// One action argument, rendered as written.
///
/// A path is joined with dots (`forest.confession`), which is how a label or an asset is
/// named; the runtime resolves the text, this only preserves what the screen said.
fn render_arg(expr: &Expr) -> String {
    match expr {
        Expr::Name { name, .. } => name.clone(),
        Expr::Field { base, name, .. } => format!("{}.{}", render_arg(base), name),
        Expr::Int { value, .. } => value.to_string(),
        Expr::Bool { value, .. } => value.to_string(),
        Expr::Str { parts, .. } => parts_text(parts, &Args::new()),
        _ => String::new(),
    }
}

/// The picture an `image` line refers to: a path written here, or a value that holds one.
///
/// Two shapes, and the head of a path is what tells them apart — the same question `theme.bg` asks, and
/// the same one `color_of` answers. A head that is not a name in scope is the picture's own name
/// (`bg.room`); one the screen was *given* is a lookup (`option.icon`). So a screen draws a picture
/// chosen by data and names a fixed one with the same syntax, which is what keeps an image from being
/// the one thing in a screen that cannot come from a value.
///
/// A string is a name too: `image "bg.room"` is how the prop schema writes an asset (`schema.rs`).
pub(crate) fn image_of(expr: &Expr, values: &Args) -> Option<String> {
    match expr {
        Expr::Field { base, .. } => {
            let bound =
                matches!(base.as_ref(), Expr::Name { name, .. } if values.get(name).is_some());
            if bound {
                Some(value_of(expr, values).as_text())
            } else {
                Some(render_arg(expr))
            }
        }
        Expr::Name { name, .. } => Some(match values.get(name) {
            Some(value) => value.as_text(),
            None => name.clone(),
        }),
        Expr::Str { .. } => Some(text_of(expr, values)),
        Expr::Paren { inner, .. } => image_of(inner, values),
        _ => None,
    }
}

/// A value as displayable text.
pub(crate) fn text_of(expr: &Expr, values: &Args) -> String {
    match expr {
        Expr::Str { parts, .. } => parts_text(parts, values),
        Expr::Int { value, .. } => value.to_string(),
        Expr::Bool { value, .. } => value.to_string(),
        Expr::Name { name, .. } => values.get(name).map(Value::as_text).unwrap_or_default(),
        // A field reads as the value it names: `text option.caption` is the caption, and
        // `text d.who` is whoever said it.
        Expr::Field { .. } => value_of(expr, values).as_text(),
        Expr::None { .. } => String::new(),
        _ => String::new(),
    }
}

/// A string literal's parts as text.
pub(crate) fn parts_text(parts: &[StrPart], values: &Args) -> String {
    parts
        .iter()
        .map(|part| match part {
            StrPart::Literal { text, .. } => text.clone(),
            StrPart::Interpolation { expr, .. } => text_of(expr, values),
        })
        .collect()
}

/// Whether a screen condition holds.
///
/// An expression this evaluator does not understand is *false* rather than a crash: a branch
/// whose condition it cannot read draws nothing, which is visible, instead of taking the
/// process down over a screen an author is midway through writing.
pub(crate) fn eval(expr: &Expr, values: &Args) -> bool {
    match expr {
        Expr::Bool { value, .. } => *value,
        Expr::Name { name, .. } => values.get(name).is_some_and(Value::truthy),
        Expr::Paren { inner, .. } => eval(inner, values),
        // Both negations evaluate the same way: the tree already says where each binds, so all
        // that is left here is the truth value.
        Expr::Unary {
            op: UnOp::Not | UnOp::Bang,
            operand,
            ..
        } => !eval(operand, values),
        Expr::Binary { op, lhs, rhs, .. } => compare(*op, lhs, rhs, values),
        _ => false,
    }
}

/// A binary operator, over the values its two sides have.
///
/// The comparison half of what a screen condition needs (`SCREENS.md §2.2`). A binding, a parameter and
/// a literal are all the screen *has*, and comparing two of them is not a read of anything outside it —
/// which is why this is here rather than in §8's work: `if device == "keyboard"` is a decision about a
/// value the screen holds, and it draws the wrong arm if the comparison is missing.
///
/// Comparisons do not coerce. `1 == "1"` is false because a number and a string are different values,
/// `1 < "a"` is false because there is no order between them, and `none == none` is true — the same
/// answer `is` gives, which is the whole relation for values that have no order.
///
/// `and` and `or` are the language's short-circuit operators (`&&`/`||`, `LANGUAGE.md §3`). Nothing a
/// screen can write today has an effect, so the two spellings agree on every expression this evaluator
/// accepts; they are written the way the VM writes them because that is what the operators *mean*.
fn compare(op: BinOp, lhs: &Expr, rhs: &Expr, values: &Args) -> bool {
    match op {
        BinOp::And => eval(lhs, values) && eval(rhs, values),
        BinOp::Or => eval(lhs, values) || eval(rhs, values),
        BinOp::Eq | BinOp::Ne => {
            let equal = value_of(lhs, values) == value_of(rhs, values);
            if op == BinOp::Eq { equal } else { !equal }
        }
        // `is` is equality too, and it is here rather than beside the comparisons because the two are
        // spelled apart: `is` is the identity a reader means by "is nothing there", and a screen that
        // writes it should keep working when the value on one side is a word rather than a reference.
        BinOp::Is | BinOp::IsNot => {
            let equal = value_of(lhs, values) == value_of(rhs, values);
            if op == BinOp::Is { equal } else { !equal }
        }
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            // Two numbers or two strings, and never one of each: a screen comparing `1 < "a"` has a
            // mistake rather than an order to compute, and false is the answer every other comparison
            // that cannot be made gets. A `NaN` on either side is the same answer, because there is no
            // order to report — which is what `partial_cmp` is for.
            let ordering = match (value_of(lhs, values), value_of(rhs, values)) {
                (Value::Num(a), Value::Num(b)) => a.partial_cmp(&b),
                (Value::Str(a), Value::Str(b)) => Some(a.cmp(&b)),
                _ => None,
            };
            let Some(ordering) = ordering else {
                return false;
            };
            match op {
                BinOp::Lt => ordering.is_lt(),
                BinOp::Le => ordering.is_le(),
                BinOp::Gt => ordering.is_gt(),
                _ => ordering.is_ge(),
            }
        }
        // Arithmetic is a *value* question rather than a condition one, and it is not implemented: this
        // is the same answer an unrecognised condition gets, and the checker reports the shape that
        // needs it (`W4013`).
        _ => false,
    }
}

/// The value of an expression. An unbound name is `none`.
///
/// Used for a comparison, for the arguments of a `use`, and for what a `for` iterates: what a screen
/// passes or walks is a value, and this is the small set of expressions this evaluator can produce one
/// from. Anything else is `none` rather than a crash — the same answer an unrecognised condition gets,
/// and for the same reason.
pub(crate) fn value_of(expr: &Expr, values: &Args) -> Value {
    match expr {
        Expr::Name { name, .. } => values.get(name).cloned().unwrap_or(Value::None),
        Expr::None { .. } => Value::None,
        Expr::Bool { value, .. } => Value::Bool(*value),
        Expr::Int { value, .. } => Value::Num(*value as f64),
        Expr::Str { parts, .. } => Value::Str(parts_text(parts, values)),
        // A call in a screen argument is an action (`SCREENS.md §7`). The vocabulary is words like
        // `quit` and `open_screen`, and a call is how one becomes a value a screen can pass on — as
        // the argument of a `use`, or into a parameter a widget then holds.
        Expr::Call { .. } => action_of(expr, values).map_or(Value::None, Value::Action),
        // A list literal, and a record literal. These are how a screen makes its own data: nothing
        // outside it has to hand one over for `for` to have something to walk, which is what keeps a
        // loop testable before the systems that feed it exist (`SCREENS.md §2.4`).
        Expr::List { items, .. } => {
            Value::List(items.iter().map(|item| value_of(item, values)).collect())
        }
        Expr::Map { entries, .. } => Value::Record(
            entries
                .iter()
                .map(|(key, value)| (key_text(key, values), value_of(value, values)))
                .collect(),
        ),
        // A field of a record — `option.caption`, where `option` is whatever the loop bound or the
        // caller passed. `theme.bg` arrives here too and resolves to `none`, which is right: a theme
        // token is read by `color_of`, not by this.
        Expr::Field { base, name, .. } => value_of(base, values)
            .field(name)
            .cloned()
            .unwrap_or(Value::None),
        Expr::Paren { inner, .. } => value_of(inner, values),
        _ => Value::None,
    }
}

/// A record literal's key, as text.
///
/// A key is written either as a string or as a bare name (`{ caption: "Yes" }`), and both mean the
/// same field. Anything else is a key nothing can look up, so it is written as it reads rather than
/// dropped — a record whose field vanished would be a loop that silently drew one thing fewer.
fn key_text(key: &Expr, values: &Args) -> String {
    match key {
        Expr::Name { name, .. } => name.clone(),
        other => text_of(other, values),
    }
}

/// A colour from a `theme.<token>` reference or a literal.
pub(crate) fn color_of(expr: &Expr, ctx: &Ctx) -> Option<Color> {
    match expr {
        Expr::Field { base, name, .. } => {
            let Expr::Name { name: base, .. } = base.as_ref() else {
                return None;
            };
            if base == "theme" {
                ctx.palette.color(name)
            } else {
                None
            }
        }
        Expr::Name { name, .. } => ctx.palette.color(name),
        Expr::Int { value, .. } => hex_color(*value),
        Expr::Paren { inner, .. } => color_of(inner, ctx),
        _ => None,
    }
}

/// A font name from a `theme.<token>` reference, or a literal.
///
/// Mirrors [`color_of`]: the value a style writes is a token of the active theme, and resolving it
/// here is what keeps a style independent of which theme is selected (`SCREENS.md §5`).
pub(crate) fn font_of(expr: &Expr, ctx: &Ctx) -> Option<String> {
    match expr {
        Expr::Field { base, name, .. } => {
            let Expr::Name { name: base, .. } = base.as_ref() else {
                return None;
            };
            if base == "theme" {
                ctx.fonts.get(name).map(str::to_string)
            } else {
                None
            }
        }
        Expr::Str { .. } => string_literal(expr),
        Expr::Paren { inner, .. } => font_of(inner, ctx),
        _ => None,
    }
}

/// A `0xRRGGBB` integer as a colour.
fn hex_color(value: i64) -> Option<Color> {
    let value = u32::try_from(value).ok()?;
    if value > 0xFF_FFFF {
        return None;
    }
    Some(Color::rgb(
        ((value >> 16) & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        (value & 0xFF) as u8,
    ))
}

/// What a style draws with, in every state it names, through its inheritance chain.
///
/// A setting's key is a state applied to a property (`State::split`): `color` and `idle_color` are the
/// values themselves, `hover_color` is what `hover` overrides. An unknown key is ignored rather than
/// reported — a setting body is `key = value` (`LANGUAGE.md §7`) and the language does not fix which
/// keys exist, which is also why a style can name a property the painter does not read yet.
pub(crate) fn style_paint(name: &str, ctx: &Ctx) -> Paint {
    let mut paint = Paint::default();
    for style in chain(name, ctx.styles) {
        for setting in &style.settings {
            let (state, key) = State::split(&setting.key);
            let target = paint.state_mut(state);
            match key {
                "color" => set_colour(&mut target.color, &setting.value, ctx),
                "background" => set_colour(&mut target.background, &setting.value, ctx),
                "size" => set_size(&mut target.size, &setting.value),
                "font" => set_font(&mut target.font, &setting.value, ctx),
                _ => {}
            }
        }
    }
    paint
}

/// Resolves a colour expression into a slot, leaving it alone when it does not resolve.
fn set_colour(slot: &mut Option<Color>, expr: &Expr, ctx: &Ctx) {
    if let Some(colour) = color_of(expr, ctx) {
        *slot = Some(colour);
    }
}

/// Resolves a numeric expression into a slot, leaving it alone when it does not resolve.
fn set_size(slot: &mut Option<f32>, expr: &Expr) {
    if let Some(size) = number(expr) {
        *slot = Some(size);
    }
}

/// Resolves a font expression into a slot, leaving it alone when it does not resolve.
fn set_font(slot: &mut Option<String>, expr: &Expr, ctx: &Ctx) {
    if let Some(font) = font_of(expr, ctx) {
        *slot = Some(font);
    }
}

/// A style and its ancestors, base first.
///
/// A `from` chain is linear, and the walk is bounded by the declaration count: a longer walk
/// has visited a style twice, which is a cycle the checker already rejects — the bound is a
/// guard against hanging on one that slipped through, not an expected outcome.
fn chain<'a>(name: &str, styles: &'a [StyleDecl]) -> Vec<&'a StyleDecl> {
    let mut collected: Vec<&StyleDecl> = Vec::new();
    let mut current = name.to_string();
    for _ in 0..=styles.len() {
        let Some(style) = styles.iter().find(|style| style.name == current) else {
            break;
        };
        collected.push(style);
        match &style.from {
            Some(base) => current = base.clone(),
            None => break,
        }
    }
    collected.reverse();
    collected
}
