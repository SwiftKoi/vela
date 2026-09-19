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

use vela_syntax::{BinOp, Expr, ScreenDecl, StrPart, StyleDecl, UnOp};

use crate::actions::Action;
use crate::images::ImageTable;
use crate::props::Anchor;
use crate::theme::{Fonts, Palette};
use crate::variants::{self, Variant};
use crate::widgets::WidgetRegistry;

mod setting;
pub(crate) use setting::is_question as is_setting_question;

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

/// Whether a call's *name* is one of the questions a screen may ask the host (`SCREENS.md §2.6`).
///
/// The names are the whole of it, which is why this is a predicate over a string: the checker walks a
/// body by name (`check/actions.rs`'s visitor) and the evaluator has the callee expression, and both
/// must agree about which two calls are questions rather than actions.
#[must_use]
pub(crate) fn is_question(name: &str) -> bool {
    variants::is_question(name) || setting::is_question(name)
}

/// Whether a callee is one of the questions a screen may ask the host (`SCREENS.md §2.6`).
///
/// Two of them, and they are one shape: `variant("pc")` asks *where this is running* and answers a truth
/// value, and `setting("text_speed")` asks *what the player chose* and answers that setting
/// (`RUNTIME.md §2.1`). Neither is an action: an action is a call a screen stores and a widget performs,
/// and a question is a value it draws or decides from.
pub(crate) fn is_question_call(callee: &Expr) -> bool {
    matches!(callee, Expr::Name { name, .. } if is_question(name))
}

/// The answer a call gives, when the call is a question.
///
/// One place, because a question can be written in three positions — a value, a condition, and a text —
/// and all three must agree about what it answers. `None` for every other call, which is how the one
/// `Call` arm in [`value_of`] tells the two apart without a second pattern for it.
fn question_of(callee: &Expr, args: &[Expr], values: &Args) -> Option<Value> {
    let Expr::Name { name, .. } = callee else {
        return None;
    };
    if variants::is_question(name) {
        return Some(Value::Bool(variant_answer(args, values)));
    }
    setting::is_question(name).then(|| setting::answer(args, values))
}

/// The answer to a `variant("name")` call.
///
/// A name the engine does not know, an argument that is not a string literal, and a call with no
/// argument all answer `false` — which is the same answer an unknown variant gets, and the checker is
/// what tells the author that the call cannot mean anything (`E5018`). Answering false here rather than
/// refusing keeps the evaluator total: every expression a screen body can hold has a value, which is
/// what lets a screen be laid out while it is being written.
fn variant_answer(args: &[Expr], values: &Args) -> bool {
    let [name, ..] = args else {
        return false;
    };
    let Value::Str(name) = value_of(name, values) else {
        return false;
    };
    Variant::named(&name).is_some_and(|variant| values.variants().has(variant))
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
        // A question the host answers reads as its answer (`SCREENS.md §2.6`), because a question is a
        // value: `text variant("pc")` says `true` or `false`, and `text setting("text_speed")` says the
        // number the player chose. Any *other* call in a text position is an action in the wrong place,
        // and stays empty rather than drawing the name of something that is not a string.
        Expr::Call { callee, args, .. } => question_of(callee, args, values)
            .unwrap_or(Value::None)
            .as_text(),
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
        // A question the host answers, in a condition: `if variant("pc")` and `if setting("skip_unseen")`
        // are decided by the answer rather than by `value_of` having found an action, which is the other
        // half of `§2.6`.
        Expr::Call { callee, args, .. } => question_of(callee, args, values)
            .unwrap_or(Value::None)
            .truthy(),
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
        // A call is one of two things, and which one is [`is_question_call`]'s answer rather than a second
        // arm: a *question* the host answers (`SCREENS.md §2.6`), which is a value like any other —
        // `variant("pc")` can be a condition, a text, or the argument of a `set` — or an *action* (§7),
        // whose vocabulary is words like `quit` and `open_screen`, and which a screen passes on as the
        // argument of a `use` or into a parameter a widget then holds.
        Expr::Call { callee, args, .. } => question_of(callee, args, values)
            .or_else(|| action_of(expr, values).map(Value::Action))
            .unwrap_or(Value::None),
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
