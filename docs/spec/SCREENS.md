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
- **Every name a screen reads must resolve** (`E2001`) — its parameters, its variables, what a loop
  bound, and a value this module declares. `LANGUAGE.md §1` is *every name resolves statically*, and a
  screen was the one construct that had never been held to it: `if nosuchname:` drew the arm nothing
  took, and said so nowhere, which is the same silence a `scene` naming an undeclared image had.
  Three positions are another vocabulary's and resolve elsewhere: a call's own name (an action the
  registry holds, a question the host answers — §7, §2.6), a `style = …` (a style, `E5007`), and
  `image`'s content when its head is not in scope (a *picture*, §3.1).

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

### 2.2 Conditionals

A screen draws the first arm whose condition holds, and nothing when none holds and there is no `else`:

```vela
screen status(ready: bool, waiting: bool):
    if ready:
        text "Ready."
    elif waiting:
        text "Waiting."
    else:
        text "Nothing."
```

- **The arms are clauses of one line.** `elif` and `else` belong to the `if` they continue, and there is
  no way to write one without it — the shape the statement form already has (`LANGUAGE.md §3`). A
  conditional is therefore one node, and choosing an arm is one decision rather than a walk that has to
  remember what the line before it concluded.
- **Every arm is checked.** Which arm draws is a runtime question (§8.2), so an unknown widget or a
  wrong prop in an `elif` is as real as one in the `then`, and an arm nobody has run yet is not an arm
  nobody wrote.
- **Every arm's reads are dependencies.** The same reason: a screen whose `elif` reads a field is stale
  when that field changes, whether or not the `elif` drew last frame.
- **A `transclude` in any arm places the caller's block** (§2.1), which is what lets a wrapper put
  content in one of several places.

A condition is a **comparison over what the screen has** — its parameters, its variables (§2.5), what a
loop bound, the project's own values, and the literals it writes:

```vela
if device == "keyboard" and shown < 2:
    text "Keys"
elif device != "mouse" or not ready:
    text "Other"
```

- **`==` and `!=` compare by value**, and never coerce: `1 == "1"` is false, because a number and the
  string that spells it are different values. `is` is equality too, and both spellings answer the same
  way — `none == none` is true, and `value is none` is how a screen asks whether it got one.
- **`<`, `<=`, `>` and `>=` order numbers and strings**, and report false for anything else: there is no
  order between a number and a string, so `1 < "a"` is false rather than a coercion nobody asked for.
- **`and`, `or` and `not` combine conditions**, with the same meanings and precedence the language gives
  them (`LANGUAGE.md §3`) — so the platform test Ren'Py writes as
  `A or (B and not C)` says what it looks like it says.
- **Comparisons are not a read of anything outside the screen**, which is why they are here rather than
  in §8's work: `if device == "keyboard"` is a decision about a value the screen holds. What §8 adds is
  the *state*, not the operator.

> **Note.** An arm is only as live as its condition can be. A condition is decided from the screen's own
> values, so one over data the runtime has not bound is false, and a chain over one draws its `else` —
> that is §8's work, stated so a chain that always takes one arm is a known limit rather than a puzzle.
> A condition that *calls* something is the other shape, and it is `W4013`: `if GamepadExists():` is
> false, and a screen cannot call anything that is not a question the host answers (§2.6). The
> difference matters — one is data the screen is waiting for, the other is a question nothing can
> answer — so the second is reported rather than left to look like a condition that simply did not
> hold.

> **Implemented (M12.1).** `if`/`elif`/`else` as arms of `ScreenLine::If`; the screen pack moved to
> version 7. `crates/vela-syntax/src/tests/screen_tests.rs` pins the shape, `tests/instantiate.rs` that
> the first arm to hold is the one drawn, `tests/check.rs` that an arm's mistakes are reported and that a
> call in a condition is `W4013`, `tests/compose.rs` that a `transclude` in an `elif` places the block,
> `tests/reactivity.rs` that an `elif`'s reads are the screen's, and `tests/conditions.rs` the
> operators — by kind, against `none`, and the chain the sample writes.

### 2.3 Input: `key` and `timer`

A screen answers input by naming what the player *meant*, never which button they pressed:

```vela
screen confirm(message, no_action):
    column:
        text message
        button:
            text "OK"
            action close_screen()
    key cancel action no_action

screen notify(message):
    box at top:
        text message
    timer 3.25 repeat action hide(notify)
```

- **A `key` binds a semantic action** — one of the host's (§11) — to an action this screen runs while it
  is shown. The name is written as a *name*, not Ren'Py's string (`key "game_menu"`), the same trade
  `style_prefix` makes (§5.2).
- **The top screen answers first**, before the runtime's own handling of that action — which is what
  makes a modal screen modal: `key cancel action no_action` is how the sample's confirm screen keeps
  Escape from dismissing it. A screen under another is not asked, exactly as it is not navigable (§10).
- **A name nothing delivers is `E5014`**, against the set the caller supplies. A binding that never
  fires is an escape hatch that quietly does nothing, and nothing about the screen would look wrong.
- **`timer <seconds> [repeat] action <call>` declares a deadline.** `repeat` makes it come round again.
- **A binding is an action, so it resolves like one**: `key cancel action no_action` means what
  `action no_action` means, including a parameter the screen was handed. A binding is not a placement,
  so it is not a widget — it is collected beside the tree and carried on the screen a runtime navigates.

> **Not yet.** A `timer` is *data*, not behaviour: the laid screen carries the deadline and the action
> (`Laid::timers`) and nothing fires them. §6 runs animation from `World::clock`, no clock reaches the
> screen runtime yet, and a deadline measured against wall time would be a frame nobody could replay.

> **Implemented (M12.1).** `key` and `timer` as screen lines; the screen pack moved to version 8.
> `crates/vela-syntax/src/tests/screen_tests.rs` pins the shape, `tests/check.rs` that an unknown name
> is `E5014` and a binding's action is checked like any other, `tests/instantiate.rs` that the bindings
> a screen declares resolve into the `Laid` it draws, and `crates/vela-cli/src/tests/ui_tests.rs` that
> the top screen answers its own — and that the vocabulary is the host's, name for name.

### 2.4 Loops

A screen draws a body once per element of a list:

```vela
screen choice(prompt, items):
    column:
        text prompt
        for option in items:
            button:
                text option.caption
                action option.action
```

- **The shape is the statement form's** (`LANGUAGE.md §3`): `for <name> in <expr>:` and a block. One
  loop in the language rather than two, so an author who knows the story syntax knows this one.
- **A field of the element resolves.** `option.caption` is that element's field, and `option.action` is
  an *action* like any other — which is what makes a menu's choices drawable as data.
- **The binding is a scope.** `for option in items` shadows an outer `option`, and the shadow ends with
  the loop — which is what keeps the dependency set honest (§8.2): a name the loop bound is not a read
  of world state. A `use` can pass a list (§7); a value that is not one walks as nothing, so the body
  draws nothing rather than failing.

> **Not yet.** Nothing *feeds* a screen a list: `Screens::dialogue` binds two strings and the runtime
> opens every other screen with no arguments, so a migrated `choice(items)` walks an unbound value and
> draws nothing — the construct is here, the data is not. It arrives with §7's systems (the menu's
> choices, which `Command::Menu` carries; the dialogue log), owned by M12.2 and M12.3. Until then a
> literal (`for pair in [{caption: "Yes"}]`) is the one list a screen builds for itself.

> **Implemented (M12.1).** `for` as a screen line; the screen pack moved to version 9.
> `crates/vela-syntax/src/tests/screen_tests.rs` pins the shape and the dotted-value rule below,
> `tests/instantiate.rs` that the body is drawn once per element, that a field resolves, and that the
> binding shadows, `tests/reactivity.rs` that the binding is not a dependency, and
> `tests/pack.rs` with `crates/vela-ui/src/pack/tests.rs` that the loop survives the codec.

> **Revised (M12.1).** A dotted value at the head of a widget's line is the widget's *content*, not a
> prop name: `text option.caption` parses as `text line` does, because no prop name contains a dot and
> the chain settles the ambiguity alone. The parser used to read `option` as a prop name and then have
> nowhere to put `.caption` — which is what the sample's menu writes.

### 2.5 Screen variables

A screen may declare a variable of its own, and write it while it is shown:

```vela
screen help:
    default device = "keyboard"

    column:
        text device
        button:
            text "Mouse"
            action set_screen_variable(device, "mouse")
```

- **The shape is the module form's** (`LANGUAGE.md §3`): `default <name> = <expr>`. One word doing one
  thing at two levels — a file's `default` is world state, a screen's is the screen's.
- **The initializer runs once**, the first time that *instance* of the screen is laid out, and may read
  a parameter or a variable declared above it. Afterwards the value the caller holds wins, which is what
  makes a write survive the next layout rather than being undone by the line that declared it.
- **A variable is the screen's own scope**, exactly as a `for`'s binding is: reading `device` is not a
  read of world state, so the dependency set stays honest (§8.2) and the screen does not relay out when
  an unrelated `device` changes.
- **`set_screen_variable(name, value)` writes it** — an action of its own rather than `set`, because the
  store is the screen's and `set` is the world's. Ren'Py's `SetScreenVariable`, the same words.
- **The name is written as a name**, so the checker holds it to what this screen declares (`E5017`) and
  `E5016` refuses a variable named after a parameter: two bindings of one name in one screen would be two
  answers to one question, and the caller's is the one the screen would silently ignore.
- **Declared at the top level** (`E5015`): *whether* a variable exists, and so what it starts as, cannot
  depend on which arm drew.
- **The value is a value.** `set_screen_variable(device, item.kind)` writes what that field holds rather
  than the words `item.kind`, because the screen resolves its own arguments before the runtime sees them
  (§7) — which is the one place an action's argument is a value rather than a name.
- **It is per instance, not per declaration.** The runtime keeps one beside each screen it has open, so
  closing a screen and opening it again starts from the initializers, and a hot reload of the source
  leaves the values alone (§12).
- **A variable is named state, not node state.** §13's "nodes with stable ids keep their state" is about
  a *widget* carrying something across a rebuild — a scroll offset, an input buffer — and this is not
  that mechanism: a variable is a name, and it survives a reload by living outside the screen rather than
  by being matched to an old node.

> **Not yet.** Nothing *feeds* a screen a value from the world: a variable starts from a literal or from a
> parameter, and a read of `World` state is §8's work. A screen can therefore hold a choice the player
> made and cannot yet hold a number the story is tracking — which is why the sample's second use of
> `default` (`page_name_value`) is a *host displayable* rather than a variable (§7), and stays §7's.

> **Implemented (M12.1).** `default` as a screen line and `set_screen_variable` dispatched; the screen
> pack moved to version 10. `crates/vela-syntax/src/tests/screen_tests.rs` pins the shape,
> `crates/vela-ui/tests/variables.rs` that the initializer runs once, that a held value wins, and that a
> name the body no longer declares is dropped, `tests/check.rs` the three codes,
> `tests/reactivity.rs` that a variable is not a dependency, `crates/vela-ui/src/pack/tests.rs` the codec,
> and `crates/vela-cli/src/tests/ui_tests.rs` the whole loop: a write re-lays the screen it wrote.

### 2.6 Variants: where this screen is running

A screen may ask **where it is being drawn** — which platform the bundle was built for, and whether the
frame it is being laid out in has room:

```vela
screen main_menu:
    vbox:
        if variant("pc") or (variant("web") and not variant("mobile")):
            textbutton _("Help") action open_screen("help")
        if not variant("small"):
            add SideImage() xalign 0.0
```

- **A variant is a question the host answers, not an action.** `variant("pc")` is a *value* — `true` or
  `false` — so it belongs in a condition (§2.2), in a `text`, or as an argument to `set`. It is a call
  because that is how a screen names a host thing everywhere else (§7's `jump(…)`, `set(…)`), and
  because Ren'Py writes it that way, so `renpy.variant("pc")` migrates unchanged.
- **The vocabulary is closed**: `pc`, `web`, `mobile`, `small`. A name outside it is **`E5018`** at the
  call, not Ren'Py's silent `False` — a migration that writes `tablet` or `touch` hears about it now
  rather than drawing the `else` arm forever. The name has to be written out, because a name the engine
  cannot see whole cannot be checked.
- **Two sources, one set.** `pc`, `web` and `mobile` come from the bundle's descriptor
  (`target.json`'s `variants`, written per target by `commands/target.rs`; a *source* run declares `pc`,
  because the platform the process is on is the answer). `small` comes from the **frame** the screen is
  laid out in: below three quarters of the frame the *project* declares — `vela.toml`'s
  `[project] size`, which is Vela's own 1280×720 when a project declares none. Ren'Py decides `small`
  from the *physical diagonal* of the device; Vela's is the room the frame has, because that is the
  question the sample's own use is asking — "there's no room" for the side image — and because a
  desktop build has no diagonal to measure. Vela owns that fraction and says so here rather than
  deriving it from something unmeasurable; the *frame* is the project's, because a game designed at
  1920×1080 is not `small` at 1500×900.
- **The size class is per layout, not per run.** It is added where the frame is known
  (`ScreenSet::lay`), so a window that is resized asks again with the new answer; the platform half
  travels with the screen set, beside its pictures and fonts.
- **A question is decidable, so `W4013` does not fire.** That warning is for a condition that *calls*
  something nothing can answer (`§2.2`); a variant is a call that can be.

> **Implemented (M12.1).** `Variant`/`Variants` in `crates/vela-ui/src/variants.rs`; the descriptor's
> `variants` field, which existed and was empty since M9, now carries a target's platform names, and
> `vela.toml`'s `[project] size` carries the frame `gui.init(width, height)` declared — Ren'Py's own
> `gui.init`, which the migration lands there (`TOOLING.md §8`).
> `tests/variants.rs` pins both sources, the sample's merged condition, and that the threshold follows
> the *declared* frame rather than a constant; `tests/check.rs` that a question is clean, an unknown
> name is `E5018` and `W4013` still fires beside one.

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

### 3.1 Pictures

`image` is the one leaf whose content is a *reference* rather than a value, and it draws it:

```vela
screen gallery(item):
    column:
        image bg.room        # a name written here
        image item.icon      # a name the screen was given
```

- **A path whose head is not in scope is the picture's own name**; one the screen was *given* is a
  lookup. So `bg.room` names an asset and `item.icon` reads the element a `for` bound (§2.4) — the same
  question `theme.bg` asks of a token, and the reason an image is not the one thing in a screen that
  cannot come from data.
- **Its size comes from the platform.** A screen cannot know how big a picture is, so the table that maps
  a name to a texture also carries its dimensions, and layout measures from that. A name the platform
  says nothing about measures to nothing and draws nothing: a picture that has not been built yet is a
  build in progress rather than a broken screen, and a guess would be a picture nobody asked for.
- **The table is injected, not compiled** (`vela_ui::images`). Only the party that uploaded a picture
  knows which texture it became, and `vela build` compiles screens rather than pixels — so a set is built
  without one and told where its pictures are once a window (or a capture) exists. That is also why the
  screen pack does not carry pictures and its version did not move.

> **Implemented (M12.1).** `image` draws, from a written path or a value, with the size coming from the
> platform's table. Until this it laid out at nothing and painted nothing — a registered widget with no
> behaviour, which the tables above had implied otherwise. `crates/vela-ui/tests/images.rs` pins the
> resolution, `tests/paint.rs` that the texture and rectangle reach the draw list, and
> `crates/vela-cli/src/tests/ui_tests.rs` that a name nobody uploaded is left out of the table.

### 3.2 Viewports

A viewport is a window onto content bigger than it:

```vela
screen log(lines):
    viewport size 400, initial 1.0:
        column gap 8:
            for line in lines:
                text line
```

- **The child keeps the size it asks for.** It is measured against *no* limit along the scroll axis, so
  a column taller than the box is not squeezed to fit — which is the whole difference between a viewport
  and a plain container, and what makes scrolling mean anything.
- **`initial` is a fraction of the travel**: `0` the top, `1` the bottom. A fraction and not an offset in
  pixels, for §4.2's reason — a pixel number is a layout that did the arithmetic the layout system should
  have done. Content that fits has no travel, so every fraction moves nothing.
- **The subtree is clipped to the box.** The renderer carries a clip rectangle on the vertices and drops
  what falls outside it, so content beyond the window is not drawn over its neighbours. Nested clips
  **intersect**, so a viewport inside a viewport shows what both admit.
- **It takes the space it is offered**, unless told otherwise: a window that sized itself to its content
  would never have anything to scroll.

> **Implemented (M12.1).** `viewport` as a container kind, the `initial` prop, and clipping in the
> renderer — a clip rectangle per vertex, so the batching by texture is untouched. Pinned by
> `crates/vela-ui/tests/viewport.rs` (the content is not squeezed; `initial` selects the start; the
> subtree is clipped and the clip is closed) and `crates/vela-render/tests/draw.rs` (a clip is stamped
> on the quads inside it and nowhere else; nested clips intersect).

> **Not yet.** Scrolling is *positioned*, not *driven*: nothing moves an offset after layout, so
> `mousewheel`, `draggable`, `pagekeys` and a draggable `scrollbar` thumb are M12.3's. They need the input
> map (§11), which resolves device events to semantic actions, and a **stateful** widget — which no widget
> is yet, and which §12's reload diff is waiting on. `vpgrid` is a viewport around a `grid` and is the
> migrator's translation (§2 is the same kind of move); `scrollbar` is written nowhere in the sample, so
> it stays **C**. Virtualization is open as §14 says: a viewport lays its whole content out and clips the
> drawing, which is not the same as materializing only the visible window.

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

style body:
    color = theme.fg
```

> **Revised (M12.1).** The derived style above is written `style speaker from body:`, and the base is
> `style body:` — a `from` takes a *name*, so `style body from:` with nothing after it is `E1004`
> rather than a style with no base. The example said otherwise until it was run.

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

- **A font token names a font the engine has**, not an asset path: loading a face is item 15's work, and
  a name the engine does not carry resolves to nothing.
- **An unroutable font falls back to the screen's own font**, silently, for the same reason a
  `style_prefix` that names nothing does (§5.2): a project may write the token before the face behind
  it is wired up, and a screen that drew no text at all would be the worse answer.
- **`size` is a style setting, not part of the font token.** Ren'Py's `text_font` names a face; its
  `text_size` is this section's `size`, already.

> **Revised (M12.1).** The token above was written `font body = @"fonts/inter.ttf" size 22 leading 1.4`
> — a path with a size and a leading. Three things were wrong with it: that does not parse (a setting
> body is `key = value`, `LANGUAGE.md §7`), `leading` has no reader in `vela-text`, and the path is an
> *asset*, which is what item 15 loads. The token is a name; the size stays the `size` setting.

> **Implemented (M12.1).** A `style` setting `font` — `font = theme.kanji`, a token reference — is
> read into the node's paint and threaded through measuring and drawing, so one screen can draw two
> scripts. The token table travels in the screen pack (version 6). `crates/vela-ui/tests/theme.rs`
> pins the token table and the screen-pack round trip, `tests/styling.rs` that a style sets a node's
> font, and `tests/paint.rs` that an unroutable font falls back rather than drawing nothing.

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

> **Not yet.** None of this exists, and the grammar above is the design rather than the surface:
> `transform` is a declaration with an unparsed body (`LANGUAGE.md §3`), a screen's `at` reads an anchor
> and nothing else (`§4.2`), and there is no animation clock — `World::clock` is read by the `time.now`
> effect and **nothing advances it**, which is why a screen's `timer` is carried and not fired (`§2.3`).
> M13 owns it.

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

An argument is either a **name** or a **value**, and the difference is who resolves it. A name —
`open_screen(settings)`, `hide(notify)` — is for the runtime: a screen cannot know what screens a
project declares. A value — `set_screen_variable(device, item.kind)` — is what the screen already had,
resolved against its own scope before the runtime sees it, because only the screen knows its scope
(§2.5). An argument whose name is *in* scope is a value and everything else is a name, which is the
rule `image bg.room` and `image item.icon` are told apart by, and the registry's schema says which kind
each one takes.

> **Implemented (M12.1).** An action is a value a screen can be given and a widget can hold, and the
> checker holds the vocabulary to the registry: `E5012` for a name that is not registered, `E5013` for
> the wrong number of arguments, and a bare name that is a parameter is left alone because that is how
> an action arrives. Of the twenty-seven entries, eight are dispatched (`open_screen`, `close_screen`,
> `hide`, `quit`, `quick_save`, `quick_load`, `rollback`, `set_screen_variable`) — the rest need the VM
> or `World`, and the reference page, the hover, and an activation that reaches one all say so rather
> than doing nothing quietly.

### 7.1 Settings, the one action whose argument is a vocabulary

`preference(name, value)` writes one of the player's settings and `toggle_preference(name)` flips a
boolean one. What a setting *is* belongs to `RUNTIME.md §2.1`; what a screen may **name** is here, and
it is a closed vocabulary rather than a value: `vela-ui::settings` holds the list, `vela doc` is where
a reader looks it up, and the checker holds a written name to it — **`E5020`** for a name that is not a
setting, **`E5021`** for a value the setting does not take.

That is `variant`'s shape and `variant`'s reason (`§2.6`): a name the engine can see is a name it can
refuse. Ren'Py learns this the hard way — `Preference("text speed")` is a *string* looked up at run
time, so a misspelling is a control that silently does nothing, which is exactly what the sample's
migrated settings screen was doing until this check existed (`TOOLING.md §8`).

**A name is written as a string, and a value is checked when it is a literal.** `preference("text_speed",
30)` is the spelling the migration emits and the reference shows, because a bare word is an *expression*:
`preference(display_mode, fullscreen)` is the name `fullscreen`, nothing declares it, and `E2001` is what
says so — two diagnostics for one mistake is the thing the checker avoids. The exception is a name the
*screen* itself declares: `preference(which, 30)` with `which` a parameter is `E5020`, because a computed
setting cannot be checked and nothing else would say so. A *value* is checked only when it is a literal,
for the same reason in reverse: `preference("text_speed", other)` is an expression, and reading `other` as
a word would report a correct screen. A choice is therefore a string too —
`preference("display_mode", "fullscreen")`.

**A toggle is an action rather than a value word.** Ren'Py spells a flip as the value `"toggle"`; here
it is `toggle_preference(name)`, because a value word would be a second vocabulary inside an argument
that only the runtime could check — and a checkbox needs to flip a setting that no screen can read yet.

> **Implemented (M12.2, in part).** The registry, the checker, the migration and the *write* are in; the
> *read* is not. `vela_ui::settings::write` is the one place the two actions are carried out, and both
> callers use it: the windowed player, where a settings screen's button is how a player changes one (and
> the settings file is rewritten at once), and the test runner, where a click is — so a settings button is
> one a test can press rather than one that reports a headless run cannot. Nothing *draws* a setting yet,
> so every entry in the list carries `read: false` and the reference says so (`RUNTIME.md §2.1`).

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

A name a block *binds* is not a read, and a name a screen *owns* is not either: a lambda's parameters, a
loop's binding (§2.4) and a screen's variable (§2.5) are all scoped, so the set holds what the body reads
from *outside* — which is what keeps `for option in items` from making the screen depend on any unrelated
field called `option`, and a `default device` from making it depend on one called `device`, forever.

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

> **Note.** A label is derived from the screen's *source*: an explicit `label` prop, or a single `text`
> child whose content is a literal. A `text` child whose content is a value — `text option.caption`, or
> the `text line` of every dialogue screen — therefore has no label, and `W4010` says so. That is honest
> rather than wrong: the tree is built from the declaration, so a caption that arrives at run time is a
> label self-voicing does not have. Deriving labels from the *evaluated* tree is what would fix it, and
> nothing does — found when a list could finally be walked (§2.4), because a data-driven caption is the
> first label that is real on screen and absent here.

## 11. Input

Input is abstracted to **semantic actions**, not keys. A project defines a binding profile;
the engine resolves device events to actions (`advance`, `skip`, `rollback`, `menu_up`,
`confirm`, `screenshot`, …). Gamepad, keyboard, mouse, and touch are profiles over the same
action set, so a screen never asks "what key was pressed".

The set is `vela_host::Action` — nine names, `Action::all()` — and it is what a screen's `key` binds
(§2.3). The checker validates against `vela_ui::input::SemanticActions`, which mirrors that list
because `vela-ui` cannot depend on `vela-host`: a screen checker that pulled a windowing library in to
validate a name would be the worse trade. `crates/vela-cli/src/tests/ui_tests.rs` asserts the two lists
agree, so a drift fails CI rather than binding a name the window cannot deliver.

> **Revised (M12.1).** This paragraph listed `ui_confirm` where the engine spells the action `confirm`.
> No such action has ever existed: the host's table is what a window delivers, and the doc now says what
> it says. Found while writing `E5014`, which makes the difference between the two visible.

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
>
> A screen's **variables** are a different kind of state and are already carried: they live outside the
> screen, in the runtime, so a reload re-lays out with them untouched (§2.5) rather than matching them to
> an old node. The mechanism this paragraph waits for is a widget's *own* state — the offset a drag moved
> — which is still to come.

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
