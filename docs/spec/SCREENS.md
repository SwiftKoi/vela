# Screen System Specification

Status: **draft, normative for M7.**

Screens are Vela's declarative UI. Ren'Py's screen language is a genuine strength, but it
inherits the same problem as the rest of the script layer: it is dynamically evaluated, so a
typo in a widget name or a missing prop is a runtime error, and every widget's layout is a
pile of `xpos`/`xmaximum`/`align` adjustments. Vela keeps the declarative feel and fixes both.

## 1. Goals

1. **Declarative.** A screen describes *what*, never *how to repaint*.
2. **Statically checked.** Unknown widgets, wrong props, wrong types, and bad screen arguments
   are compile errors (`E5xxx`).
3. **Real layout.** A constraint-based solver, not absolute positioning arithmetic.
4. **Reactive by change, not by polling.** Screens re-evaluate only the bindings whose inputs
   changed.
5. **Accessible structurally.** Every interactive node participates in the accessibility tree
   by construction, not by opting in.
6. **Hot-reloadable.** Editing a screen recompiles and swaps it without losing runtime state.

## 2. Declaration

```vela
screen dialogue(name: str?, line: str, show_choices: bool = false):
    layer ui
    box at bottom, stretch_x:
        pad 24
        column gap 8:
            if name is not none:
                text name style = speaker
            text line style = body
```

Screens are **pure functions of their arguments and bound state**. They produce a widget tree;
they never mutate `World` during layout. Mutation happens through *actions* only (§7), which
keeps rendering one-directional and testable.

> **Runtime note (M8).** A screen is not yet a command, and there is no `show screen` statement.
> The presenter draws a screen *named for the command it is presenting*: a `Say` goes through a
> screen called `dialogue` when the project declares one — called positionally with
> `(speaker, line)`, and with `none` for a narrator — and falls back to the built-in dialogue
> box otherwise.
>
> Screens *layer*: a stack is drawn bottom-first over the dialogue. `Escape` (`cancel`) closes
> the top screen, or opens a screen called `pause` when none is open; `menu_up`/`menu_down`
> move focus over the action-bearing nodes in tree order (§10); `advance`/`confirm` activates
> the focused one instead of the story. The focus highlight is drawn by the runtime, not the
> screen, because focus has to be visible and a screen can forget to draw it.
>
> Of the action set, **`open_screen`, `close_screen`, and `quit` are dispatched**; the rest
> (`jump`, `set`, `play`, `call`, …) need the VM or `World` and are not wired. A screen opened
> this way takes no arguments yet, so its parameters must have defaults. Pointer hit-testing
> exists (`vela_ui::focus::at`) but the host delivers semantic actions rather than coordinates
> (§11), so nothing calls it. Stated plainly rather than implied.

Compile-time checks:
- `screen` calls must match the declared parameter list (`E5004`).
- Every widget name must be in the `WidgetRegistry` (`E5005`).
- Every prop must exist on that widget and typecheck (`E5006`).
- Styles referenced must exist (`E5007`).
- A `bind` expression's type must be displayable (`E3005`).

## 3. Widget tree

The built-in node set. Each is a registered widget (`CONVENTIONS.md §4.2`), not a hardcoded
variant — the list below is the *default set* shipped in `vela-ui`, and a plugin can add more.

| Node | Kind | Purpose |
| --- | --- | --- |
| `box` | container | Single child, alignment and padding |
| `row` / `column` | container | Linear stacks with `gap` |
| `grid` | container | Fixed rows/columns |
| `flow` | container | Wrapping flow layout |
| `stack` | container | Z-ordered overlay |
| `absolute` | container | Explicit position (escape hatch; linted as `W4007` when avoidable) |
| `text` | leaf | Shaped, wrapped text |
| `image` | leaf | Asset or image-set reference |
| `button` | interactive | Action-bearing node |
| `bar` / `slider` | interactive | Value binding |
| `input` | interactive | Text entry |
| `spacer` | leaf | Flexible space |
| `video`, `live2d`, `particles` | leaf | Rich media |

## 4. Layout

### 4.1 Model

Layout is a two-pass constraint solver in the spirit of flexbox, with named anchors instead
of coordinate math:

- **Pass 1 (measure)**: parent proposes constraints `(min, max)` on each axis; child returns
  its desired size.
- **Pass 2 (arrange)**: parent assigns final rects according to its layout mode.

### 4.2 Props

| Prop | Applies to | Meaning |
| --- | --- | --- |
| `pad <n>` | containers | Inner padding |
| `gap <n>` | `row`/`column`/`grid`/`flow` | Space between children |
| `align <anchor>` | containers | Child alignment within available space |
| `anchor <anchor>` | any | Self-positioning in parent |
| `size <w> <h>` | any | Fixed size (`auto`, `<n>`, `<pct>%`) |
| `min` / `max` | any | Size bounds |
| `grow <n>` | any | Flex weight along the parent's main axis |
| `at <transform>` | any | Position/scale/alpha offset |
| `stretch_x` / `stretch_y` | any | Shorthand for the matching axis |
| `background <colour>` | any | Fill painted behind the node, as a theme token (`theme.bg`) |
| `style <name>` | any | A `style` declaration whose `color` (and `size`) this node draws with |
| `action <call>` | interactive | What activating the node does: `action open_screen(settings)` |

Both `align` and `anchor` compose: a node that names its own `anchor` positions itself in the
slot its parent offers, and one that does not takes the parent's `align`. They are separate
props for the same reason `grow` and `stretch_y` are — self-positioning and child-alignment are
different questions, and a node often wants one without the other.

`background` is the only colour prop on a node; text colour comes from `style`, so a var and its
box can be recoloured independently. A literal (`background = 0x203040`) is allowed but
`W4008`-linted, for the reason §5 gives.

> **`at` and transforms.** The transform grammar is M13's, so no screen can express one yet.
> Until it can, `at <anchor>` is read as an anchor — `box at bottom` puts the box at the bottom
> of its layer — rather than being silently dropped. When transforms land, an anchor-shaped
> value should keep working and a transform-shaped one should take over.

Anchors: `top_left`, `top`, `top_right`, `left`, `center`, `right`, `bottom_left`, `bottom`,
`bottom_right`.

**Design rule:** if a layout requires arithmetic on pixel coordinates, the layout system is
missing a prop. `absolute` exists, but using it for something a container could express is
`W4007` — a lint that teaches the better tool rather than a hard error that blocks a
legitimate edge case.

## 5. Styling and theming

Styles are typed, cascading, and materialized into tokens — no stringly-typed style soup.

> **Note.** The token syntax below is indicative. Styling is settled at M7, when
> screens are implemented; the language proper only fixes that a declaration body is
> `key = value` lines (`LANGUAGE.md §7`).
>
> **M7 status: settled, in favour of this syntax.** The type word in `color bg = ...` is
> not decoration — it is what tells a palette that `bg` is a colour while `sm` in
> `space sm = 4` is a length. The parser does not record how a number was written, so both
> arrive as the same kind of value; the type word is the only thing that distinguishes
> them, and a setting body is therefore `[type] key = value` (`LANGUAGE.md §7`).

```vela
theme dusk:
    color bg        = 0x10121a
    color fg        = 0xe6e6f0
    color accent    = 0x6ea8fe
    space sm        = 4
    space md        = 8
    font  body      = @"fonts/inter.ttf" size 22 leading 1.4

style speaker from body:
    color = theme.accent
    weight = bold

style body from:
    color = theme.fg
```

Rules:
- A style may inherit via `from`; inheritance is checked (a style cannot inherit from itself,
  `E5008`).
- Only tokens and typed values are permitted; a raw magic color in a screen is `W4008`.
- Theme switching at runtime re-resolves styles without recompiling screens.
- **Contrast checking**: a foreground/background pair below a WCAG threshold is `W4009`, with
  the computed ratio in the message. Accessibility as a lint, not a manual audit.

## 6. Animation

Animation is declarative over state, which is what makes it deterministic and skippable.

```vela
transform slide_in:
    from x = -1.0, alpha = 0.0
    to   x = 0.0,  alpha = 1.0
    over 0.4s ease_out

transition dissolve(d = 0.3s):
    out: alpha 1.0 -> 0.0
    in:  alpha 0.0 -> 1.0
```

- **Transforms** are reusable, composable value animations.
- **Transitions** describe a change between two scene states.
- **Springs** are available as an easing: `spring(stiffness, damping)`.
- All animation is driven by `World::clock`, so animating, pausing, and skipping are all
  deterministic and replayable.
- "Skip" and "fast-forward" are honored by advancing the clock arithmetically — no special
  animation-skipping code paths, which is exactly why Ren'Py-style skip behaves consistently
  here.

## 7. Actions

Interaction produces typed actions; screens never mutate state directly.

```vela
button:
    text "Tell the truth"
    action jump(forest.confession)          # story action
    action set(trust, trust + 1)            # state action
    enable_if trust > 3                     # static condition
```

The action set is a registry. Built-ins: `jump`, `call`, `return`, `set`, `toggle`,
`play`, `stop`, `open_screen`, `close_screen`, `wait`, `quit`, `quick_save`, `quick_load`.
Adding an action is a registry entry, not a UI-core edit.

## 8. Reactivity

### 8.1 Bindings

A screen may read `World` state; it re-evaluates only what changed.

```vela
text "Trust: {bind trust}"
bar value = bind trust_cap range 0..10
```

### 8.2 Change tracking

`World` maintains a **change log** of modified fields per command. After each VM step, the UI
diffs that log against the set of fields each binding depends on, and re-evaluates only the
bindings whose inputs are dirty. This is what keeps a 60fps dialogue screen from re-running
layout over the whole tree every frame.

Dependency sets are computed **statically** at screen-compile time, so there is no runtime
dependency-tracking machinery and no way for a binding to silently miss an update.

### 8.3 No implicit repaint

A widget repaints when its inputs change or when the animation clock advances it. There is no
"repaint everything" mode. This is a defined property, not an optimization: it makes frame
cost predictable, which is the `ARCHITECTURE.md §8` posture applied to UI.

## 9. Widget registry ABI

The interface a widget implements. Adding one touches no core file
(`CONVENTIONS.md §4.2`).

```rust
pub trait Widget: Send + Sync {
    /// Unique name as used in screens.
    fn name(&self) -> &'static str;

    /// Declared props: name, type, default. Used by the type checker and docs.
    fn props(&self) -> PropsSchema;

    /// Pass 1: desired size under parent constraints.
    fn measure(&self, cx: &mut MeasureCx, node: &Node, c: Constraints) -> Size;

    /// Pass 2: assign child rects (containers only).
    fn arrange(&self, cx: &mut ArrangeCx, node: &Node, rect: Rect) {}

    /// Emit draw commands (leaves and decorated containers).
    fn paint(&self, cx: &mut PaintCx, node: &Node, rect: Rect, out: &mut DrawList) {}

    /// Accessibility contribution (defaults provided).
    fn a11y(&self, node: &Node) -> A11yNode { A11yNode::default() }

    /// Minimal inputs that should trigger re-evaluation.
    fn deps(&self, node: &Node) -> DepSet { DepSet::none() }
}
```

`measure`/`arrange`/`paint` are separate traits in the implementation if that keeps files
small; the registry exposes them behind one facade. The prop schema is the single source of
truth for the type checker, the LSP completion list, and the generated docs — three consumers,
one definition, so they can never drift.

## 10. Accessibility

Structural, not a mode (VISION Principle 8).

- Every interactive widget emits an `A11yNode`: role, label, value, state, and focus order.
- **Self-voicing** reads the accessibility tree, so a new widget is voiced correctly the
  moment it is registered — no per-widget audio code.
- Focus order is derived from tree order and `absolute` positions, and is queryable:
  `vela test --a11y` asserts a screen is fully navigable by keyboard.
- Contrast is linted (§5); missing labels on interactive nodes are `W4010`.
- Text rendering uses the system's preferred scaling where the host provides it.

## 11. Input

Input is abstracted to **semantic actions**, not keys. A project defines a binding profile;
the engine resolves device events to actions (`advance`, `skip`, `rollback`, `menu_up`,
`ui_confirm`, `screenshot`, …). Gamepad, keyboard, mouse, and touch are profiles over the same
action set, so a screen never asks "what key was pressed".

## 12. Hot reload

Editing a screen inside `vela run` recompiles it and **diffs the widget tree**:

- Nodes with stable ids keep their state (scroll position, input buffer, animation phase).
- Nodes without ids are matched positionally.
- A structural change that cannot be reconciled falls back to a full screen rebuild, with a
  console note saying so.

Because screens are pure functions of arguments plus bound state, rebuild is safe by
construction — there is no hidden mutation to lose. This is the payoff for §2's purity rule.

> **Implemented so far (M8).** `vela run` polls the project's sources on the host's idle tick
> (five a second) and, on a change, recompiles every screen and swaps them into the running
> window; the story is untouched. A file that no longer parses keeps the **last good** screens
> and says so — an author mid-edit should not see a blank window.
>
> The **diff is not consulted yet**: `vela_ui::reload::diff` exists and is tested, but no widget
> carries state — there is no scroll offset or input buffer to preserve — so the rebuild is
> unconditional and node-id preservation has nothing to act on. It lands with the first stateful
> widget, which is also what the `list` in §13 waits on. Stated rather than implied.

## 13. Open questions

1. **Scroll and virtualization** (M7): how much of a long list is materialized? Start with a
   `list` widget that materializes only the visible window; expose `virtual` explicitly.
2. **Rich text runs** (M7): inline styling inside a `text` node (`[b]…[/b]`-style) versus a
   `runs` list of styled spans. The latter is typed; prefer it, but prose authors will want
   inline. Decide with real script samples.
3. **3D stage integration** (M13): how transforms compose with camera space for the `live2d`
   and 3D-model widgets.
4. **RTL and vertical text** (post-1.0): `vela-text` is designed to allow it; not in the 1.0
   surface.
