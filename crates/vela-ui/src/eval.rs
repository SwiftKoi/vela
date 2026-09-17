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
use crate::props::Anchor;
use crate::theme::{Fonts, Palette, string_literal};
use crate::tree::{Paint, State};
use crate::widgets::WidgetRegistry;

/// A value a screen argument can hold.
///
/// The small set a screen body can branch on. A `screen` is called by the runtime with
/// arguments it already has as `World` values, and this is the shape they arrive in.
#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    /// A string.
    Str(String),
    /// A number.
    Num(f64),
    /// A boolean.
    Bool(bool),
    /// An action: what a widget does when it is activated (`SCREENS.md §7`).
    ///
    /// An action is a *value* and not only a syntax, which is what lets a screen take one as a
    /// parameter and hand it to a widget. Without that, `confirm(message, yes_action, no_action)`
    /// cannot be written at all: the caller's answer is the content.
    Action(Action),
    /// No value.
    None,
}

impl Value {
    /// The value as text, for `text <name>`.
    #[must_use]
    pub fn as_text(&self) -> String {
        match self {
            Self::Str(text) => text.clone(),
            Self::Bool(true) => "true".to_string(),
            Self::Bool(false) => "false".to_string(),
            // An action as text is the call it was written as. Nothing draws one today, but a screen
            // that interpolates an action should say what it is rather than render as blank.
            Self::Action(action) => action.to_string(),
            Self::None => String::new(),
            // Not `{}`: a float's default formatting is locale-adjacent enough that the
            // determinism rules ban it, and a fixed precision is what a screen wants anyway.
            Self::Num(number) if number.fract() == 0.0 => format!("{number:.0}"),
            Self::Num(number) => format!("{number:.2}"),
        }
    }

    /// Whether the value counts as true in a screen condition.
    #[must_use]
    pub fn truthy(&self) -> bool {
        match self {
            Self::None => false,
            Self::Bool(value) => *value,
            Self::Str(text) => !text.is_empty(),
            Self::Num(number) => *number != 0.0,
            // An action is something, so it is true — the same answer every non-`none` value gets.
            Self::Action(_) => true,
        }
    }
}

/// The arguments a screen was called with, by parameter name, in call order.
#[derive(Clone, Debug, Default)]
pub struct Args {
    values: Vec<(String, Value)>,
}

impl Args {
    /// No arguments.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Binds a parameter.
    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        self.values.push((name.into(), value));
    }

    /// The value bound to a parameter, if any.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }
}

/// What a screen body reads when it is evaluated.
pub struct Ctx<'a> {
    /// The widget vocabulary, for telling a widget from a prop.
    pub registry: &'a WidgetRegistry,
    /// The active theme's colours.
    pub palette: &'a Palette,
    /// The active theme's font tokens (`SCREENS.md §5`).
    pub fonts: &'a Fonts,
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
pub(crate) fn action_of(expr: &Expr) -> Option<Action> {
    let Expr::Call { callee, args, .. } = expr else {
        return None;
    };
    let Expr::Name { name, .. } = callee.as_ref() else {
        return None;
    };
    Some(Action::new(
        name.clone(),
        args.iter().map(render_arg).collect(),
    ))
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

/// A value as displayable text.
pub(crate) fn text_of(expr: &Expr, values: &Args) -> String {
    match expr {
        Expr::Str { parts, .. } => parts_text(parts, values),
        Expr::Int { value, .. } => value.to_string(),
        Expr::Bool { value, .. } => value.to_string(),
        Expr::Name { name, .. } => values.get(name).map(Value::as_text).unwrap_or_default(),
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
        Expr::Binary { op, lhs, rhs, .. } => match op {
            BinOp::Is | BinOp::IsNot => {
                let equal = value_of(lhs, values) == value_of(rhs, values);
                if *op == BinOp::Is { equal } else { !equal }
            }
            _ => false,
        },
        _ => false,
    }
}

/// The value of an expression. An unbound name is `none`.
///
/// Used for a comparison, and for the arguments of a `use`: what a screen call passes is a value, and
/// this is the small set of expressions this evaluator can produce one from. Anything else is `none`
/// rather than a crash — the same answer an unrecognised condition gets, and for the same reason.
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
        Expr::Call { .. } => action_of(expr).map_or(Value::None, Value::Action),
        _ => Value::None,
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
