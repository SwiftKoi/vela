# Screen System Specification

Status: **draft, normative for M7, extended by M8's hot reload and M9's screen pack.**

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

A parameter's type is optional: `screen dialogue(name, line)` is the same screen with both
parameters accepting anything. An absent type is `Unknown` (`LANGUAGE.md §5.4`), which is how
Ren'Py's screens are written and therefore how a migrated one arrives — the migration has nothing
to refuse or invent.

Screens are **pure functions of their arguments and bound state**. They produce a widget tree;
they never mutate `World` during layout. Mutation happens through *actions* only (§7), which
keeps rendering one-directional and testable.

> **Not yet.** A screen is not yet a command, and there is no `show screen` statement.
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
- A `use`'s arguments must match the screen's declared parameters (`E5010`, §2.1).
- Every widget name must be in the `WidgetRegistry` (`E5005`).
- Every prop must exist on that widget and typecheck (`E5006`).
- Styles referenced must exist (`E5007`).
- A `bind` expression's type must be displayable (`E3005`).

### 2.1 Composition

A screen includes another with `use`, and may hand it a block that the used screen places wherever it
writes `transclude`:

```vela
screen game_menu(title):
    box:
        text title
        transclude

screen preferences:
    use game_menu("Preferences"):
        column:
            text "Text speed"
```

Three rules, and all three follow from §2's "a screen is a pure function of its arguments":

- **`use name` is a call with no arguments.** There is deliberately no Ren'Py-style sharing of the
  caller's scope: a used screen sees what it was passed and nothing else, which is what keeps a
  dependency set static (§8.2) and a screen's meaning independent of where it is used.
- **The name resolves within this file**, like a style (§5). There is no project-wide screen table, so
  a screen cannot name another module's screen.
- **A block is placed, not dropped.** A block handed to a screen that never writes `transclude` is
  `W4012` — a warning, because the author wrote content that would otherwise vanish silently. A
  `transclude` with nothing passed draws nothing: nothing was written, so nothing is lost.

Arguments bind as a call reads: positional values in order, named ones by name, and a parameter the
call omits keeps its default. A parameter's type may be absent, in which case it accepts any argument
(`LANGUAGE.md §5.4`) — which is how most migrated screens are written.

The checker refuses a `use` of a screen that does not exist (`E5009`), a call whose arguments do not
fit the screen's parameters (`E5010`), and a composition that loops (`E5011`). A cycle is not a slow
screen, it is an unbounded one: drawing it would draw it again.

> **Implemented (M12.1).** `use`, `transclude`, and the rules above. The screen pack moved to version
> 4 to carry both lines (§13.1), and `crates/vela-ui/tests/compose.rs` pins the diagnostics, the tree
> a composition draws, and the fold into the caller's dependency set.

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
| `style <name>` | any | A `style` declaration whose `color`, `size`, and `font` this node draws with |
| `action <call>` | interactive | What activating the node does: `action open_screen(settings)` |

Both `align` and `anchor` compose: a node that names its own `anchor` positions itself in the
slot its parent offers, and one that does not takes the parent's `align`. They are separate
props for the same reason `grow` and `stretch_y` are — self-positioning and child-alignment are
different questions, and a node often wants one without the other.

`background` is the only colour prop on a node; text colour comes from `style`, so a var and its
box can be recoloured independently. A literal (`background = 0x203040`) is allowed but
`W4008`-linted, for the reason §5 gives.

> **Note.** `at` and transforms. The transform grammar is M13's, so no screen can express one yet.
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
> **Revised (M7).** The type word in `color bg = ...` is
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
    font  body      = "sans"

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
- **Styles and themes resolve within the file that declares them.** A screen uses the styles, theme,
  and characters of its own module; there is no project-wide style table, and a screen cannot name a
  style declared in another file. This is the same boundary as `default` and every other
  non-label name (`LANGUAGE.md §6.1`), and it is why a screen pack is one file's worth (§13). It is
  written down because it is the sort of thing an editor makes *visible*: a completion list that
  quietly omits another file's styles looks like a bug unless the rule is the rule.
- **Contrast checking**: a foreground/background pair below a WCAG threshold is `W4009`, with
  the computed ratio in the message. Accessibility as a lint, not a manual audit.

A theme's `font` token is a **name**, and a style draws text with it:

```vela
theme cjk:
    font ui  = "sans"
    font kanji = "SourceHanSans"

style japanese:
    font = theme.kanji
```

- **A font token names a font the engine has**, not an asset path: loading a face is the asset and
  theme work of item 15, and a name the engine does not carry resolves to nothing.
- **An unroutable font falls back to the screen's own font**, silently, for the same reason a
  `style_prefix` that names nothing does (§5.2): a project may write the token before the face
  behind it is wired up, and a screen that drew no text at all would be the worse answer.
- **`size` is a style setting, not part of the font token.** Ren'Py's `text_font` names a face;
  its `text_size` is this section's `size`, already.

> **Revised (M12.1).** The `font` token above was written `font body = @"fonts/inter.ttf" size 22
> leading 1.4` — a path with a size and a leading. Three things were wrong with that: it does not
> parse (a setting body is `key = value`, `LANGUAGE.md §7`, and a token carries no trailing words),
> `leading` has no reader (`vela-text` lays out by size and width), and the path is an *asset*,
> which is what item 15 loads. The token is a name and the size stays the `size` setting.

> **Implemented (M12.1).** A `style` setting `font` — `font = theme.kanji`, a token reference — is
> read into the node's paint and threaded through measuring and drawing, so one screen can draw two
> scripts. The token table travels in the screen pack (version 6). `crates/vela-ui/tests/theme.rs`
> pins the token table, `tests/instantiate.rs` that a style sets a node's font, and `tests/paint.rs`
> that an unroutable font falls back rather than drawing nothing.


### 5.1 Interaction states

A style may give a property a value per interaction state, by prefixing the setting's key:

```vela
style item:
    color             = theme.fg
    hover_color       = theme.accent
    selected_color    = theme.accent
    insensitive_color = theme.dim
```

The states are `hover`, `selected`, `idle`, and `insensitive`. **`idle` names the value a setting has
on its own**, so `idle_color` and `color` are one setting and the later line wins — which is what
Ren'Py's `idle_*` also means, and why there is no fifth field to store.

An override is a **diff**, not a replacement: a state that sets only `color` keeps the value's
`background`. The override is resolved through the style's inheritance chain like any other setting, so
a derived style changes the states it names and keeps the rest.

**`selected` is the focused control, and it belongs to everything that control draws** — the words
inside a focused button change with it, because they are part of that button (§10). The other two are
stored and resolvable but nothing selects them yet: `hover` needs a pointer, which the host does not
deliver (§11 resolves device events to semantic actions rather than coordinates), and `insensitive`
needs `enable_if` evaluated, which no phase does. A state nothing can select is a state nothing draws.

> **Implemented (M12.1).** A `style`'s settings carry a value per state, resolved through its chain and
> chosen at paint time from the focus cursor. `crates/vela-ui/tests/instantiate.rs` pins the resolution
> — including that `idle_color` and `color` are one setting and that an override is a diff — and
> `tests/paint.rs` pins that the focused control's subtree is the one that changes, which is also what
> holds the painter's focus numbering to the one `focus::hotspots` produces.

### 5.2 Style prefixes

A screen gives its widgets a style without naming one on each of them:

```vela
screen say:
    style_prefix say

    window:
        text "The rain has stopped."
```

`style_prefix say` means every widget in the block falls back to `say_<widget>` — `say_window` for the
`window`, `say_text` for the `text` — when the project declares that style. Ren'Py writes the prefix as
a string; here it is a **name**, because Vela names a style the way it names anything else (`style =
body`) and dropping the quotes is the migrator's job.

- **A widget's own `style = …` wins.** The prefix is what a widget falls back to, not what it is given,
  so a node that names a style keeps it and the rest of the screen still gets the skin.
- **A prefix that names nothing is not an error.** It falls back to the widget's own defaults, which is
  what makes a prefix safe to write before every style it names exists.
- **The block is the scope.** A nested block that declares its own prefix overrides the enclosing one
  inside itself and nowhere else — an `if` branch and a widget's children are blocks like any other.
- **A `use`d screen does not inherit the caller's prefix.** A screen is a function (§2.1): its look
  cannot depend on where it was used, and a prefix that leaked across a `use` would make it do exactly
  that.

> **Implemented (M12.1).** `style_prefix` as a screen line, resolved per widget with the fallback above,
> and carried in the screen pack (version 5). `crates/vela-ui/tests/instantiate.rs` pins each rule: the
> prefixed style per widget, an explicit `style =` winning, the silent fallback, a nested override
> scoping to its block, and a used screen starting unsuffixed.

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

**An action is a value, not only a syntax.** A screen may take one as a parameter and hand it to a
widget, which is what lets the *caller* supply the answer rather than the screen hard-coding it:

```vela
screen confirm(message, yes_action, no_action):
    column:
        text message
        button:
            text "Yes"
            action yes_action
        button:
            text "No"
            action no_action
```

Written where it is used, an action is a call (`action quit()`); handed in, it is a name
(`action yes_action`). Both mean the same thing to the widget that holds it, and an action travels
through `use` arguments like any other value (`§2.1`).

The action set is a registry, and the checker **reads** it: a call whose name is not registered is
`E5012`, and one whose argument count does not match is `E5013`. Adding an action is a registry
entry, not a UI-core edit.

Two things the registry also says, because a reader of the reference deserves both. An entry is
either **dispatched** — the runtime acts on it — or *declared, not dispatched yet*, which is the
state of most of the vocabulary: `preference`, `file_page`, `language`, and the rest are this
language's words for systems the later milestones build, named now because a screen that uses one has
to check now. And the set is not a list of Ren'Py's names: `ShowMenu`, `Start`, and `MainMenu` are
absent because a menu is a screen (`open_screen`) and the beginning is a label (`jump`).

> **Implemented (M12.1).** An action is a value a screen can be given and a widget can hold, and the
> checker holds the vocabulary to the registry: `E5012` for a name that is not registered, `E5013` for
> the wrong number of arguments, and a bare name that is a parameter is left alone because that is how
> an action arrives. Of the twenty-six entries, seven are dispatched (`open_screen`, `close_screen`,
> `hide`, `quit`, `quick_save`, `quick_load`, `rollback`) — the rest need the VM or `World`, and the
> reference page, the hover, and an activation that reaches one all say so rather than doing nothing
> quietly.

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

> **Implemented (M8).** `vela run` polls the project's sources on the host's idle tick
> (five a second) and, on a change, recompiles every screen and swaps them into the running
> window; the story is untouched. A file that no longer parses keeps the **last good** screens
> and says so — an author mid-edit should not see a blank window.
>
> The **diff is not consulted yet**: `vela_ui::reload::diff` exists and is tested, but no widget
> carries state — there is no scroll offset or input buffer to preserve — so the rebuild is
> unconditional and node-id preservation has nothing to act on. It lands with the first stateful
> widget, which is also what the `list` in §14 waits on. Stated rather than implied.

## 13. The screen pack

A **bundle** must not compile anything at run time (`BUILD_AND_ASSETS.md §1`, `RUNTIME.md §8`),
and that holds for the interface as much as for the story. `vela run` on a *project* parses a
screen's declaration at startup; a run from a *bundle* does not. Instead, `vela build` compiles
each module's screens once and writes a **screen pack**:

```
dist/
  manifest.json
  scripts/main.velac        the story, compiled
  screens/main.velspk       the interface, compiled
```

A pack carries the module's name and the module's `ScreenSet` inputs — the `screen` and `style`
declarations and the active theme's colour palette. Running a bundle decodes a pack and rebuilds
the set against the engine's own widget vocabulary, so there is no parser, no checker, and no
`.vela` file in the path to drawing a screen.

### 13.1 Container

Binary, little-endian, and shaped exactly like `.velac` (`BYTECODE.md §3.1`) — the two are the
same kind of thing and should not look like two different ideas:

```
magic     [u8; 4]   b"VELS"
version   u16       PACK_VERSION
flags     u32       reserved; written zero
sections  each u32 length-prefixed, in fixed order:
            module    string
            screens   count + ScreenDecl
            styles    count + StyleDecl
            palette   count + (token, r, g, b)
            fonts     count + (token, name)
checksum  u64       FNV-1a over every byte before it
```

The reader is **total**: no input makes it panic. A length is checked against what is left before
anything is allocated, expressions are depth-limited, and the checksum catches the two ways a pack
actually arrives broken — a truncated transfer and a half-written file. It is not a signature.

- **Versioned.** A reader refuses a `pack_version` it does not know, naming the version, rather
  than reading it as if the fields it did not recognize were absent. The version is bumped for any
  change to the container or to the declaration fields it carries, because those *are* the format.
- **Binary, not text.** A pack is not source and is not meant to be edited; a readable form would
  be a second thing to keep in step with the language.
- **One per module.** A `ScreenSet` is one file's worth, because styles resolve where they are
  declared (§5). A module that declares no screens, styles, or theme produces no pack.
- **Not a second IR.** A screen is evaluated against its arguments and the active theme at layout
  time (§2), so a live expression tree exists at run time however it is encoded. The pack carries
  that tree rather than a widget IR that would need a second evaluator kept in step with this one.
- **Not the story.** Only the interface is packed; the prose lives in `.velac` and nowhere else.

> **Implemented (M9).** `vela_ui::ScreenPack` is the artifact: `vela build` compiles the
> declarations into it and writes one `screens/<module>.velspk` per module that declares UI, and
> `vela run <bundle>` decodes them with no parse. A pack from a version this build does not know is
> refused rather than half-read. `crates/vela-ui/tests/pack.rs` pins the round trip and the
> refusal; `crates/vela-ui/src/pack/tests.rs` pins every expression tag against the reader and a
> corrupt container against the refusal; `crates/vela-cli/src/tests/bundle_tests.rs` pins that a
> built bundle carries the screens, that the story is *not* in the pack, and that the runtime loads
> them.

## 14. Open questions

1. **Scroll and virtualization** (M7): how much of a long list is materialized? Start with a
   `list` widget that materializes only the visible window; expose `virtual` explicitly.
2. **Rich text runs** (M7): inline styling inside a `text` node (`[b]…[/b]`-style) versus a
   `runs` list of styled spans. The latter is typed; prefer it, but prose authors will want
   inline. Decide with real script samples.
3. **3D stage integration** (M13): how transforms compose with camera space for the `live2d`
   and 3D-model widgets.
4. **RTL and vertical text** (post-1.0): `vela-text` is designed to allow it; not in the 1.0
   surface.
