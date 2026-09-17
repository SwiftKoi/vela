//! The bridge from a laid-out widget tree to a draw list.
//!
//! `vela-render` owns the pixels and knows nothing about widgets; `vela-ui` owns the widgets
//! and, until now, knew nothing about pixels. This is the seam between them (`ARCHITECTURE.md
//! §5`, the rank-8 exception that lets `vela-ui` consume the renderer). It is deliberately
//! plain data in and plain data out: a `Node` plus its `Frame` becomes `RectQuad`s and
//! `GlyphQuad`s, so the mapping from a screen to what it draws is testable with no GPU and no
//! display — which a screenshot cannot be.
//!
//! Rectangles first, then text, because that is the order the draw list keeps them in and the
//! order a frame paints them.
//!
//! It also resolves **what a style paints** — the colour, size and font a `style` sets in each
//! interaction state, through its inheritance chain (`SCREENS.md §5`). That arrived here from
//! `eval.rs` when variants made the evaluator's file exceed `REPO_LAYOUT.md §3.1`'s budget, and it
//! belongs here rather than there for the same reason it was in one file to begin with: it *produces*
//! a [`Paint`], which is the thing this module exists to fill in.

use vela_render::{Clip, Color, DrawList, GlyphQuad, ImageQuad, RectQuad};
use vela_syntax::{Expr, StyleDecl};
use vela_text::TextEngine;

use crate::eval::{Ctx, number};
use crate::images::ImageTable;
use crate::layout::Frame;
use crate::theme::string_literal;
use crate::tree::{Kind, Node, Paint, State};

/// The size text is drawn at when no `style` sets one.
const DEFAULT_SIZE: f32 = 24.0;

/// The colour of text with no `style`, matching the presenter's default body colour.
fn default_text() -> Color {
    Color::rgb(235, 235, 240)
}

/// The font a node's text is drawn with: what its `style` named, when the engine has it.
///
/// A style names a font the engine may not carry — `SCREENS.md §5` — and the honest answer to a
/// name nothing registered is the screen's own font, not an empty draw list. That is the same
/// silent-fallback rule a `style_prefix` follows (§5.2), and it is what lets a project write the
/// token before the face behind it is wired up.
///
/// Shared by measuring and drawing, because a node measured in one font and painted in another would
/// lay out around text it did not draw.
#[must_use]
pub(crate) fn resolve_font<'a>(
    text: &TextEngine,
    wanted: Option<&'a str>,
    default: &'a str,
) -> &'a str {
    match wanted {
        Some(name) if text.has_font(name) => name,
        _ => default,
    }
}

/// What a paint carries down the tree.
///
/// The text engine and the draw list travel together in every call, and so does the focus cursor —
/// which is why they are one value rather than three parameters threaded through every node.
struct Painter<'a> {
    text: &'a mut TextEngine,
    font: &'a str,
    draw: &'a mut DrawList,
    /// The pictures a name resolves to, so an `image` node has a texture to sample.
    images: &'a ImageTable,
    /// How many action-bearing nodes the walk has passed.
    seen: usize,
    /// Which of them the focus cursor is on, by the same numbering [`crate::focus::hotspots`] produces.
    focused: Option<usize>,
    /// Whether the walk is inside the focused control.
    ///
    /// A control's state belongs to everything it draws, not only to its own node: the words inside a
    /// focused button *are* that button, which is why Ren'Py's `hover_color` on a button colours its
    /// text. So the state is carried down the subtree rather than recomputed per node, and a text leaf
    /// inside the focused button draws `selected` although it has no action of its own.
    ///
    /// Only `selected` is reachable today: `hover` needs a pointer, which the host does not deliver
    /// (`SCREENS.md §11` — it resolves device events to semantic actions), and `insensitive` needs
    /// `enable_if` evaluated, which nothing does. Both are stored and resolvable; a state nothing can
    /// reach is a state nothing draws.
    selected: bool,
}

/// Paints `root` and everything under it into `draw`.
///
/// `frame` must be the frame `layout` produced for `root`; the two walk together, one child
/// per child, and a mismatch is a bug in the caller rather than something to recover from.
///
/// `focused` is the focused hotspot's index, counting action-bearing nodes in tree order — the same
/// numbering `focus::hotspots` produces, so a caller that has a focus cursor can pass it straight
/// through (`SCREENS.md §10`).
///
/// `images` is where a picture's name becomes a texture. It is supplied rather than reached for
/// because only the platform knows it (`images.rs`), and a name the table does not hold draws
/// nothing — the screen around it is still correct, which is what a build in progress looks like.
pub fn paint(
    root: &Node,
    frame: &Frame,
    text: &mut TextEngine,
    font: &str,
    draw: &mut DrawList,
    focused: Option<usize>,
    images: &ImageTable,
) {
    let mut painter = Painter {
        text,
        font,
        draw,
        images,
        seen: 0,
        focused,
        selected: false,
    };
    paint_node(root, frame, 0.0, 0.0, &mut painter);
}

/// Paints one node, at the absolute origin its ancestors place it at.
///
/// The focus cursor is counted here rather than before the walk, so the numbering is the one
/// [`crate::focus::hotspots`] produces: action-bearing nodes, in tree order, parents before children.
/// A test asserts the two agree, because two walks that disagree would recolour the wrong button.
fn paint_node(node: &Node, frame: &Frame, parent_x: f32, parent_y: f32, painter: &mut Painter<'_>) {
    let left = parent_x + frame.rect.x;
    let top = parent_y + frame.rect.y;

    let enclosing = painter.selected;
    if node.action.is_some() {
        let index = painter.seen;
        painter.seen += 1;
        if painter.focused == Some(index) {
            painter.selected = true;
        }
    }
    let state = if painter.selected {
        State::Selected
    } else {
        State::Idle
    };
    let paint = node.paint.in_state(state);

    if let Some(background) = paint.background
        && frame.rect.width > 0.0
        && frame.rect.height > 0.0
    {
        painter.draw.push_rect(RectQuad {
            x: left,
            y: top,
            width: frame.rect.width,
            height: frame.rect.height,
            color: background,
        });
    }

    leaf_content(node, &paint, frame, left, top, painter);

    // A viewport shows a *window* of its content: the children are laid out whole and the part outside
    // the box is clipped rather than drawn over the neighbours below it. The clip is closed again after
    // the subtree, so a sibling is unclipped whatever it is.
    let clipping = matches!(node.kind, Kind::Viewport { .. })
        && frame.rect.width > 0.0
        && frame.rect.height > 0.0;
    if clipping {
        painter
            .draw
            .push_clip(Clip::rect(left, top, frame.rect.width, frame.rect.height));
    }

    for (child, child_frame) in node.children.iter().zip(&frame.children) {
        paint_node(child, child_frame, left, top, painter);
    }

    if clipping {
        painter.draw.pop_clip();
    }

    // The focused state is the *subtree's*, so a sibling of the focused control is not selected just
    // because the walk has been through it.
    painter.selected = enclosing;
}

/// Draws what a leaf holds: its text, or its picture.
///
/// One function because the two are the same question asked of different content — what does this
/// rectangle show — and because the alternative is two blocks in the middle of the tree walk.
fn leaf_content(
    node: &Node,
    paint: &Paint,
    frame: &Frame,
    left: f32,
    top: f32,
    painter: &mut Painter<'_>,
) {
    if let Kind::Text { text: content, .. } = &node.kind
        && !content.is_empty()
        && frame.rect.width > 0.0
    {
        paint_text(content, paint, left, top, frame.rect.width, painter);
        return;
    }

    // A picture, at the rectangle layout gave it. A name the platform says nothing about draws
    // nothing — the same silence the presenter keeps for a scene whose image was never built, and for
    // the same reason: there is no texture to sample, and a guess would be a picture nobody asked for.
    if let Kind::Image { name, .. } = &node.kind
        && frame.rect.width > 0.0
        && frame.rect.height > 0.0
        && let Some(picture) = painter.images.get(name)
    {
        painter.draw.push_image(ImageQuad {
            x: left,
            y: top,
            width: frame.rect.width,
            height: frame.rect.height,
            uv: [0.0, 0.0, 1.0, 1.0],
            image: picture.texture,
            color: Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
        });
    }
}

/// Shapes one run of text and emits its glyphs, wrapped to `max_width`.
///
/// The node's [`Paint`] arrives whole rather than as three values, because the font, the colour, and
/// the size are one answer to one question — and because resolving the font needs the engine, which the
/// painter already carries.
fn paint_text(
    content: &str,
    paint: &Paint,
    left: f32,
    top: f32,
    max_width: f32,
    painter: &mut Painter<'_>,
) {
    let font = resolve_font(painter.text, paint.font.as_deref(), painter.font);
    let size = paint.size.unwrap_or(DEFAULT_SIZE);
    let color = paint.color.unwrap_or_else(default_text);
    let Some(layout) = painter.text.layout(font, size, content, Some(max_width)) else {
        return;
    };
    let (atlas_width, atlas_height) = painter.text.atlas().size();

    for line in &layout.lines {
        let baseline = top + line.baseline;
        for glyph in &line.glyphs {
            let Some(rect) = painter.text.glyph(font, size, glyph.id) else {
                continue;
            };
            painter.draw.push_glyph(GlyphQuad {
                x: left + glyph.x + rect.left as f32,
                y: baseline - rect.top as f32,
                width: rect.width as f32,
                height: rect.height as f32,
                uv: [
                    rect.x as f32 / atlas_width as f32,
                    rect.y as f32 / atlas_height as f32,
                    (rect.x + rect.width) as f32 / atlas_width as f32,
                    (rect.y + rect.height) as f32 / atlas_height as f32,
                ],
                color,
            });
        }
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
