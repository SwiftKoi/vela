# Navigating a story in an editor

A walkthrough of the language server, using `examples/standard` — three modules, four labels that
cross between them, and one label (`main.back_from_street`) called from a file that does not declare
it. Every step below is asserted by `crates/vela-cli/tests/lsp_walkthrough.rs`, against this project and
the real `vela lsp` binary, so the document cannot drift away from what the server does.

## Pointing an editor at it

The server speaks LSP over stdin and stdout:

```
vela lsp examples/standard
```

There is no bundled plugin yet, so the client config is the generic one. In Neovim:

```lua
vim.lsp.start({
  name = "vela",
  cmd = { "vela", "lsp", "." },
  root_dir = vim.fn.getcwd(),
})
```

The project directory is loaded by the same code path `vela check` uses, so the editor sees exactly the
files the command line sees — including the naming rule that makes `chapters/street.vela` the module
`chapters.street`.

Diagnostics arrive when a file is opened and after every change, and they are the same list
`vela check` prints. If they ever disagree, that is a bug: the parity is asserted in
`crates/vela-cli/src/tests/lsp_parity.rs`.

## 1. Where does this jump go?

Open `src/main.vela` and go to `label start:`.

```vela
label start:
    scene bg.room with dissolve
    ren "It rained all evening."
    "There was the street, and there was the fire."
    "The street first."
    jump chapters.street.arrive
```

Put the cursor on `chapters.street.arrive` and ask for the definition. It opens
`src/chapters/street.vela` at `label arrive:`, which is in a different file *and* a different module.
The qualifier is not a string the server pattern-matches: it is resolved the way the compiler resolves
it, so a typo is a diagnostic rather than a jump into the wrong place.

## 2. What is this name?

Hover it, in either file. The answer names the kind, the module-qualified name, and the file:

```
**label** `chapters.street.arrive` — declared in `chapters/street.vela`
```

Hover also answers for things there is nowhere to jump to, which is the half no index can do — it asks
the type checker what is at the caret:

| Caret | Answer |
| --- | --- |
| `Ending.cold` (inside `cold`) | `` `Ending` `` |
| `var ending: Ending = Ending.cold` | `` **local** `ending: Ending` `` |
| `nights += 1` | `` `int` `` |
| `MAX_TRUST` in a comparison | `` **const** `main.MAX_TRUST`: `int` — declared in `main.vela` `` |

The last two are what makes this more than a declaration lookup: an expression's type and a declaration's
type are both answers, and neither is written down where the caret is.

## 3. Who calls this?

Go back to `src/main.vela` and put the cursor on `back_from_street` in

```vela
label back_from_street:
    trust = trust + 1
```

Ask for references. Two of the three hits are in `src/chapters/street.vela`, where they are written as
`jump main.back_from_street` — a qualified reference in a file that has to import this module to name
it. That is the question a text search answers badly: it finds the strings, but not which module owns
the label, and it cannot tell a reference from a comment about one.

## 4. What can I jump to from here?

Inside `src/main.vela`, in a label body, type `jump` and a space — *before* typing the target — and ask
for completion. `jump ` on its own does not parse yet, which is exactly when the question is being asked,
so the answer comes from the text as well as the tree. The list is the labels *this module can reach*: its
own by name, and the imported ones qualified the way they have to be written.

```
back_from_hearth             label
back_from_street             label
chapters.hearth              module · chapters.hearth
chapters.hearth.settle       label · chapters.hearth
chapters.street              module · chapters.street
chapters.street.arrive       label · chapters.street
ending                       label
start                        label
tally                        label
```

A label in a module nothing here imports is not offered, because writing it would not resolve. That is
the difference between a completion list and a list of every string in the project, and it is asserted
with a fixture that has an unreachable label in it (`crates/vela-lsp/src/tests/completion_tests.rs`).

In a `screen`, the same question at the start of a line offers the widget vocabulary instead — `column`,
`text`, `button`, and the rest — because that is the only thing such a line can begin with.

## 5. Rename it everywhere

Rename `back_from_street` to `returned_from_street`. The edit touches two files: the declaration in
`src/main.vela`, and the two qualified references in `src/chapters/street.vela`. After it, `vela check`
is clean and the story still links, which is the point — a rename that leaves a reference behind would
be a compile error in a file you were not looking at.

## 6. What is this widget?

In `src/main.vela`, inside a `screen`, hover the name a line begins with:

```
**widget** `column` — It is a container, and takes children.
```

`column` is not declared anywhere a name is — the symbol index has never heard of it, and the checker
never types it — so this comes from a third source: the widget and action registries in `vela-ui`, the
same schemas `vela doc` renders. Hover an action the same way:

```
**action** `close_screen()` — Dismiss the screen this action is in.
```

Only where the word can mean what it says. `text` as a prop's value on that same line is a value, not a
widget, which is the rule the completion list already uses — and it is why a variable named `text`
still hovers as a variable.

If the project has generated the reference — `vela doc --out docs/reference`, one line in a build
script — the hover also links to that widget's section:

```
**widget** `column` — It is a container, and takes children.

[Widget reference](file:///…/docs/reference/widgets.md#column)
```

Without the page there is deliberately no link: a link to a file that is not there is one an editor
offers to open and then fails on. `examples/standard` has not generated one, which is why the answers
above stop at the sentence; the link itself is asserted in `crates/vela-lsp/src/tests/docs_tests.rs`.

## What is not here yet

These are gaps rather than bugs. The first two are the rest of `docs/spec/TOOLING.md §4`'s capability
list; the third is the checker's, not the server's, and the server reports it honestly rather than
guessing:

- **Completion after a `.`** — enum variants, struct fields, module members. The trigger character is
  deliberately not declared, because a `.` promises a member list the server does not have.
- **Completion of widget properties and screen actions** — `text` is offered, `size` is not yet.
- **A struct constructor's type inside a collection** — hovering `directions` in
  `src/chapters/street.vela` says `list<?>` rather than `list<Direction>`, because a call to a struct's
  constructor is not typed yet. The checker knows what `Direction("north", 4)` is when it is assigned
  directly; in a list literal it does not.
- **Inlay hints, semantic tokens, code actions, signature help.**
- **Renaming across an unopened file** — the edit is computed from the workspace, so this works, but the
  file's own copy on disk has to be reloaded by the editor afterwards.
