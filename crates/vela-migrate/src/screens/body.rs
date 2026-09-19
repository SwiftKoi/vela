//! The walk: a screen body, a widget and its block, and the styles a file declares.
//!
//! One rule runs through it — **a line is written only if it still says something**. A widget whose
//! picture was reported, a block whose every line was, a style with nothing left: each goes rather
//! than being emitted empty, because `image` with an anchor after it is a parse error, and one of
//! those hides every entry below it in the report.

use super::props::*;
use super::tables::*;
use super::words::*;
use super::*;

/// A conditional or a loop: its condition decides whether the block draws at all.
///
/// A condition is read in two steps, and the order is the rule. First the theme: a `gui.<name>` is
/// resolved the way a prop's is, and a name no value holds is *reported* and its arm dropped — the
/// migrated `main_menu` guarded its title with `gui.show_name`, which `options.rpy` declares and the
/// theme does not hold. Then the question: a call nothing answers cannot decide an arm, and the engine
/// reports the same thing as `W4013`, so the arm is reported and left out rather than written to warn.
fn condition(head: &str, rest: &str, node: &Node, depth: usize, out: &mut String, ctx: &mut Ctx) {
    // `rest` still carries the header's colon, and a name with a colon on the end is not a name —
    // `value` is what strips it, so the two are asked in one order.
    let Some(condition) = ctx.resolved(&value(rest)) else {
        return;
    };
    let condition = host_values(&condition, ctx);
    match ungrounded(&condition) {
        Some(call) if head != "for" => {
            ctx.gaps.unknown.push(format!(
                "`{call}(…)` in a condition, which is the host's to answer and Vela's host does not \
                 (`SCREENS.md §11`)"
            ));
            return;
        }
        // A loop's iterable is a *value* rather than an action (`SCREENS.md §2.4`), and `range(n)` is
        // Python's: nothing in a Vela screen produces a sequence yet, so the loop is written and walks
        // nothing — which is what §2.4's **Not yet** says.
        Some(call) => ctx.gaps.unknown.push(format!(
            "the `{call}(…)` a loop iterates, which no value in a screen produces"
        )),
        None => {}
    }
    // `else:` has nothing after the word, and an unconditional space before the colon is `else :` —
    // which the parser reads as a name and then a colon with a gap in it.
    let header = match condition.trim() {
        "" => head.to_string(),
        condition => format!("{head} {condition}"),
    };
    block(&header, "", &node.children, depth, out, ctx);
}

/// Ren'Py's own globals as a migrated project reads them (`SCREENS.md §7`).
///
/// A condition in a Ren'Py screen reads the *engine*: whether the main menu is up, whether a replay
/// is running, the dialogue log, whether the build has autosave. None of those is a thing a Vela
/// project can ask — they are the host systems §7 assigns to M12.2 and M12.3 — and a migrated screen
/// that kept the name would read nothing, silently, because a screen is where the checker does not
/// resolve names. So each name is answered *here*, once, with the value the engine would have given
/// it today (the main menu does not exist, there is no replay mode, there is no log), and the entry
/// says which system owns the answer. A name outside the table is answered `none` and reported the
/// same way: the line is kept, so the arm that does draw keeps drawing.
const GLOBALS: &[(&str, &str, &str)] = &[
    (
        "main_menu",
        "false",
        "whether the engine's main menu is up: Vela's is M12.2's, and no Vela game is in one",
    ),
    (
        "_in_replay",
        "false",
        "whether a replay is running, which is replay mode — the M12.3 side of M8's rollback",
    ),
    (
        "_history_list",
        "none",
        "the dialogue log: nothing records what was said yet (`SCREENS.md §7`, M12.2)",
    ),
    (
        "config.has_autosave",
        "false",
        "an autosave system Vela has not got",
    ),
    (
        "config.has_quicksave",
        "false",
        "the screens over save slots are M12.2's; the actions exist and the slots have no page",
    ),
];

/// The condition with Ren'Py's globals answered, and an entry for each one answered.
fn host_values(condition: &str, ctx: &mut Ctx) -> String {
    let mut out = condition.to_string();
    for (name, value, why) in GLOBALS {
        if !out.contains(name) {
            continue;
        }
        out = out.replace(name, value);
        ctx.gaps.unknown.push(format!(
            "`{name}`, which the migration answered `{value}`: {why}"
        ));
    }
    out
}

/// A `use`: a call with no arguments and no block, or one that hands over a block.
///
/// `use navigation` is the first; `use game_menu("About"):` is the second. Both are the same line and
/// the colon is what tells them apart (`SCREENS.md §2.1`).
fn use_line_step(rest: &str, node: &Node, depth: usize, out: &mut String, ctx: &mut Ctx) {
    let line = format!("use {}", use_line(rest, ctx));
    if !block(&line, "", &node.children, depth, out, ctx) {
        emit(out, depth, &line);
    }
}

/// One block of a screen body.
pub(super) fn body(nodes: &[Node], depth: usize, out: &mut String, ctx: &mut Ctx) {
    // Ren'Py writes a position under a widget as often as on it, and the two spellings mean the
    // same thing: `xalign 0.5` and `yalign 0.5` as lines are the `anchor center` a pair on the
    // widget's own line would be. Either half alone is pixel placement and is reported.
    let anchor = anchor_lines(nodes);
    for (index, node) in nodes.iter().enumerate() {
        if let Some((horizontal, vertical, line)) = &anchor {
            if index == *horizontal {
                emit(out, depth, line);
                continue;
            }
            if index == *vertical {
                continue;
            }
        }
        let marked = unmarked(node.text.trim());
        if marked.contains("{#") {
            // `{#auto_page}` is a *translation id*: Ren'Py keys its catalogue on it and draws
            // nothing, so the words come across and the identifier is reported — a catalogue is
            // M12.3's, and the strings are its value (`SCREENS.md §7`).
            ctx.gaps.systems.push(
                "a Ren'Py translation id (`{#…}`), which a catalogue keys on and Vela has none for",
            );
        }
        let text = markers(&marked);
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        let (head, rest) = head_of(&text);

        if let Some((_, system)) = SYSTEMS.iter().find(|(name, _)| *name == head) {
            ctx.gaps.systems.push(system);
            continue;
        }
        line(&head, rest, node, depth, out, ctx);
    }
}

/// One line of a screen body, once its head and the text after it are known.
fn line(head: &str, rest: &str, node: &Node, depth: usize, out: &mut String, ctx: &mut Ctx) {
    match head {
        // `has vbox:` names a container the block *is*, so the block becomes the widget.
        "has" => {
            let (inner, inner_rest) = head_of(rest);
            if let Some(widget) = widget_name(inner) {
                let props = props_of(&pairs(inner_rest), depth + 1, ctx);
                block(&widget, &props, &node.children, depth, out, ctx);
            }
        }
        "if" | "elif" | "else" | "for" | "while" => condition(head, rest, node, depth, out, ctx),
        "use" => use_line_step(rest, node, depth, out, ctx),
        "transclude" => emit(out, depth, "transclude"),
        "default" => {
            if python_call(rest) {
                ctx.gaps
                    .unknown
                    .push(format!("`default {}`, a host value", first_word(rest)));
            } else {
                emit(out, depth, &format!("default {}", value(rest)));
            }
        }
        "style_prefix" => emit(
            out,
            depth,
            &format!("style_prefix {}", unquote(&value(rest))),
        ),
        "timer" => emit(out, depth, &timer(rest, ctx)),
        "variant" => ctx.gaps.systems.push(
            "`variant` inside a screen, which selects a whole body per variant rather than a \
             condition within one",
        ),
        "textbutton" | "imagebutton" | "button" => {
            lower_button(head, rest, depth, node, out, ctx);
        }
        _ => {
            if let Some(widget) = widget_name(head.to_string()) {
                lower_widget(&widget, head, rest, depth, node, out, ctx);
            } else {
                lower_prop(head, rest, depth, out, ctx);
            }
        }
    }
}

/// Writes a header, its colon and its body — or nothing, when the body came out empty.
///
/// The colon is part of the same rule, and it is the one a generator gets wrong in a way that points
/// somewhere else entirely: a header with nothing under it is a parse error, and a header *without* a
/// colon whose children follow is worse — the children become the next siblings, so one missing colon
/// turned into a run of "no widget called `action`" that read like a rule about prop names.
///
/// `prelude` is the props a header carries, already indented one level in: they are the block's first
/// lines, so a header with props and no children still has a block.
///
/// Returns whether it wrote anything, so a caller with a leaf spelling (`use`) can write that instead.
pub(super) fn block(
    header: &str,
    prelude: &str,
    children: &[Node],
    depth: usize,
    out: &mut String,
    ctx: &mut Ctx,
) -> bool {
    let mut inner = String::new();
    body(children, depth + 1, &mut inner, ctx);
    if inner.trim().is_empty() && prelude.trim().is_empty() {
        return false;
    }
    emit(out, depth, &format!("{header}:"));
    out.push_str(prelude);
    out.push_str(&inner);
    true
}

/// One widget line, written only when it still says something.
pub(super) fn lower_widget(
    widget: &str,
    head: &str,
    rest: &str,
    depth: usize,
    node: &Node,
    out: &mut String,
    ctx: &mut Ctx,
) {
    if head == "vpgrid" || widget == "grid" {
        lower_grid(widget, rest, node, depth, out, ctx);
        return;
    }
    let (caption, props) = positional(!CAPTIONLESS.contains(&widget), rest);
    let mut line = widget.to_string();
    let mut content = false;
    if let Some(caption) = caption {
        // `text "hi"` and `add "art/room.png"` carry their content this way. Two captions have to be
        // *read* rather than copied, and both are things a Vela screen has no value for: a picture
        // that is a host call or a skin path (`SideImage()`, `gui.main_menu_background`) is not an
        // image anything declares, and a string that interpolates or carries a text tag is Ren'Py's
        // own text — `[config.name!t]` and `{a=…}` are a namespace and a tag Vela does not have.
        if widget == "image" {
            if picture(&caption) {
                line.push(' ');
                line.push_str(&value(&caption));
                content = true;
            } else {
                ctx.gaps.pictures.push(format!("`{caption}`"));
            }
        } else if widget == "text" {
            match text_content(&caption) {
                Some(text) => {
                    line.push(' ');
                    line.push_str(&text);
                    content = true;
                }
                None => {
                    ctx.gaps
                        .unknown
                        .push(format!("the `text {caption}` it draws"));
                    return;
                }
            }
        } else {
            line.push(' ');
            line.push_str(&value(&caption));
            content = true;
        }
    }
    if NEEDS_CONTENT.contains(&widget) && !content {
        return;
    }
    let props = props_of(&props, depth + 1, ctx);
    let mut body_text = String::new();
    if head == "vpgrid" {
        // `vpgrid` is a grid inside a viewport (`SCREENS.md §3.2`), and the grid is what scrolls.
        let mut grid = String::new();
        if block("grid", "", &node.children, depth + 1, &mut grid, ctx) {
            body_text = grid;
        }
    } else {
        body(&node.children, depth + 1, &mut body_text, ctx);
    }
    // A widget whose picture, props and children all went says nothing, and an `image` with a
    // leftover prop after it is a parse error rather than a line. A `spacer` is the exception: it
    // says something by *being* there, which is what Ren'Py's bare `null` means.
    let has_block = !props.trim().is_empty() || !body_text.trim().is_empty();
    if !has_block && !content && widget != "spacer" {
        return;
    }
    if has_block {
        emit(out, depth, &format!("{line}:"));
        out.push_str(&props);
        out.push_str(&body_text);
    } else {
        emit(out, depth, &line);
    }
}

/// `textbutton "Yes" action x` is a `button` holding a `text` in Vela.
pub(super) fn lower_button(
    head: &str,
    rest: &str,
    depth: usize,
    node: &Node,
    out: &mut String,
    ctx: &mut Ctx,
) {
    // `textbutton "Yes"` and `imagebutton "face.png"` carry their words or their picture; a plain
    // `button` is the *lowering* of one, so it takes props and no caption of its own.
    let (caption, props) = positional(head != "button", rest);
    if head == "imagebutton" {
        ctx.gaps
            .unknown
            .push("`imagebutton`, a button whose face is a picture".to_string());
    }
    let props = props_of(&props, depth + 1, ctx);
    let mut inner = String::new();
    if let Some(caption) = caption {
        // The caption goes through the same rule a `text`'s content does: a link tag or an
        // interpolation into a namespace Vela does not have is reported, and the button keeps its
        // action — dropping the whole widget would lose the one thing a button is for.
        match text_content(&caption) {
            Some(text) => emit(&mut inner, depth + 1, &format!("text {text}")),
            None => ctx
                .gaps
                .unknown
                .push(format!("the `{caption}` it is labelled with")),
        }
    }
    body(&node.children, depth + 1, &mut inner, ctx);
    if inner.trim().is_empty() {
        return;
    }
    emit(out, depth, "button:");
    out.push_str(&format!("{props}{inner}"));
}

/// A grid, and the `vpgrid` that is one inside a viewport (`SCREENS.md §3.2`, §5.1).
///
/// Ren'Py writes the shape as `grid <cols> <rows>` — or as `cols`/`rows` lines under the widget — and
/// Vela's grid takes the *column* count and wraps when it has more cells, so the row count is
/// reported. The count is a prop of the grid's header rather than a line under it (`required: true`
/// in the registry), which is why a grid does not go through the ordinary prop path.
pub(super) fn lower_grid(
    widget: &str,
    rest: &str,
    node: &Node,
    depth: usize,
    out: &mut String,
    ctx: &mut Ctx,
) {
    let Some((columns, children)) = grid_shape(rest, &node.children, ctx) else {
        ctx.gaps
            .unknown
            .push("the `grid` and the cells it holds".to_string());
        return;
    };
    let (wrapper, cells) = if widget == "viewport" {
        viewport_props(&children)
    } else {
        (Vec::new(), children)
    };
    let header = format!("grid columns {columns}");
    let mut grid = String::new();
    let inner_depth = depth + usize::from(widget == "viewport");
    if !block(&header, "", &cells, inner_depth, &mut grid, ctx) {
        return;
    }
    if widget == "viewport" {
        let mut window = String::new();
        for child in &wrapper {
            lower_prop(
                &head_of(&unmarked(child.text.trim())).0,
                unmarked(child.text.trim())
                    .split_once(' ')
                    .map_or("", |(_, rest)| rest),
                depth + 1,
                &mut window,
                ctx,
            );
        }
        emit(out, depth, "viewport:");
        out.push_str(&window);
        out.push_str(&grid);
    } else {
        out.push_str(&grid);
    }
}

/// A `vpgrid`'s children, split into the props its *viewport* carries and the cells of its grid.
///
/// `yinitial` is the wrapper's `initial` — where the window starts — and a `spacing` or a
/// `style_prefix` under the same widget is the cells' business. Everything else is placement or a
/// system, which is reported wherever it sits.
pub(super) fn viewport_props(children: &[Node]) -> (Vec<Node>, Vec<Node>) {
    let mut wrapper = Vec::new();
    let mut cells = Vec::new();
    for child in children {
        let (head, _) = head_of(&unmarked(child.text.trim()));
        if matches!(head.as_str(), "yinitial" | "initial") {
            wrapper.push(child.clone());
        } else {
            cells.push(child.clone());
        }
    }
    (wrapper, cells)
}

/// A grid's column count, and the children that are not part of its shape.
///
/// A `vpgrid`'s `yinitial` is its viewport's `initial`, and Ren'Py's `xfill`/`ysize` on one are
/// placement, so those lines belong to the wrapper rather than to the grid — everything else is a
/// cell or a prop of the grid, and `spacing` among them is the grid's own `gap`.
pub(super) fn grid_shape(
    rest: &str,
    children: &[Node],
    ctx: &mut Ctx,
) -> Option<(String, Vec<Node>)> {
    let mut columns: Option<String> = None;
    let mut rows: Option<String> = None;
    let mut inline = rest.trim().trim_end_matches(':').trim();
    while let Some((token, after)) = take_value(inline) {
        inline = after.trim_start();
        if columns.is_none() {
            columns = Some(token);
        } else if rows.is_none() {
            rows = Some(token);
        }
    }

    let mut kept = Vec::new();
    for child in children {
        let text = unmarked(child.text.trim());
        let (head, after) = head_of(&text);
        match head.as_str() {
            "cols" | "columns" if columns.is_none() => columns = Some(after.trim().to_string()),
            "rows" if rows.is_none() => rows = Some(after.trim().to_string()),
            _ => kept.push(child.clone()),
        }
    }
    if let Some(rows) = rows {
        ctx.gaps.unknown.push(format!(
            "the grid's row count (`{rows}`), which Vela's grid does not carry: it wraps at the \
             column count instead"
        ));
    }
    let columns = ctx.resolved(columns.as_deref()?)?;
    Some((columns, kept))
}
pub(super) fn style_body(nodes: &[Node], depth: usize, out: &mut String, ctx: &mut Ctx) {
    for node in nodes {
        let text = unmarked(node.text.trim());
        let (head, rest) = head_of(&text);
        if head.is_empty() || head.starts_with('#') || rest.is_empty() {
            continue;
        }
        if rest.contains('(') {
            // `Frame("gui/frame.png", …)` is a Ren'Py displayable: a picture a *screen* places,
            // not a value a style paints, and Vela's `background` takes a colour.
            ctx.gaps.pictures.push(format!("`{head}`"));
        } else if PAINTED.contains(&strip_state(&head)) {
            if let Some(setting) = ctx.resolved(rest) {
                emit(out, depth, &format!("{head} = {setting}"));
            }
        } else {
            ctx.gaps.placement.push(head);
        }
    }
}

/// A `style X is Y:` header, which is `style X from Y:` in Vela.
pub(super) fn style_header(name: &str, rest: &str, ctx: &Ctx) -> String {
    let name = renamed(name, ctx.renames);
    let rest = rest.trim().trim_end_matches(':').trim();
    match rest.split_once(" is ") {
        Some((_, base)) => match ctx.style(base) {
            // `is default` and `is empty` are Ren'Py's built-ins, and both mean "inherit nothing",
            // so dropping them is faithful rather than a loss. So is a base Vela has nothing for: a
            // `from` naming a style the module does not declare is `E5008`, and the base's own look
            // is a report entry where it belongs.
            Some(base) => format!("style {name} from {base}:"),
            None => format!("style {name}:"),
        },
        None => format!("style {name}:"),
    }
}
