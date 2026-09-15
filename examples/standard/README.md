# standard

The example the CI gates run: `vela build --verify-reproducible` on it (gate 6) and the wasm smoke
against its web bundle (gate 9). It is deliberately the **broadest** thing that works today rather
than the smallest, so a regression in any of it fails a gate.

## What it is

Three modules and a picture, because a story is split across files the way a book is split across
chapters (`LANGUAGE.md §6`):

| Path | Module | What it holds |
| --- | --- | --- |
| `src/main.vela` | `main` | the interface (theme, styles, screens), the world state, and the endings |
| `src/chapters/street.vela` | `chapters.street` | a scene, its character, a `struct`, a `fn`, and a menu |
| `src/chapters/hearth.vela` | `chapters.hearth` | loops, conditionals, and a call back into the hub |
| `assets/art/room.png` | — | the background, imported by the texture importer |

`main.start` jumps into a chapter, the chapter calls `main.tally` and comes back, and the hub
keeps the state. Every one of those is a cross-module transfer, which is what linking is for
(`LANGUAGE.md §6.1`).

## What it exercises

- **Linking**: `jump` and `call` both cross a module boundary, in both directions.
- **Flow**: `menu` with a prompt, `if`/`elif`/`else`, `while`, `for` over a list, `match` over an
  `enum`, and `return`.
- **State**: `default` + assignment + `+=`, `const`, a `fn` with a parameter and a branch, and a
  `struct` whose field is read into a string.
- **Presentation**: `scene`/`show`/`hide`, `with dissolve`/`fade`, `wait click`, `pause`, a
  character speaking (`ren "…"`), and string interpolation.
- **Effects**: `rand.int`, declared and called — a capability the world provides rather than the
  host.
- **Screens**: `dialogue`, `pause`, `settings`, and `credits`, built from
  `box`/`row`/`column`/`text`/`button`/`bar`/`spacer`, with theme tokens, a style inheriting from
  another, and the runtime actions (`close_screen`, `quick_save`, `quick_load`, `open_screen`,
  `quit`).

## What it deliberately does not do

Each of these is a limit of the engine today, and the example is written within them rather than
around them:

- **State does not cross a module boundary.** A `default` belongs to the module that declares it —
  another module referencing it is `E2001` — and two modules declaring one is refused by the
  linker, because world state is global and one name cannot be two slots. So the chapters are pure
  scenes and the hub owns the state.
- **Values do not cross either.** `street.turns_word(2)` would be `E2005`: checking is per-module,
  so there is no signature to check the call against. Only *labels* cross.
- **No audio.** `play`/`stop` name an asset and nothing imports an audio format yet, so a project
  with one fails its build rather than shipping silence.
