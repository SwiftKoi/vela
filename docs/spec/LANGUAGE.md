# Language Specification

Status: **draft, normative for M1–M5, extended by M9's asset references and M10's tests.**

The surface language is called **Vela** (`.vela` files). It is indentation-significant,
statically typed, and compiled ahead-of-time to bytecode. Its design goal is a specific
tension: *keep `"Hello."` a one-line program, while making everything else statically
analyzable.*

## 1. Design goals

1. **A bare string is dialogue.** No ceremony for the common case.
2. **No implicit globals.** Every name resolves statically; typos are compile errors.
3. **Types are inferred where obvious, required where ambiguous.** `var x = 3` is fine;
   `var x` is an error.
4. **Exhaustiveness is checked.** `match` over an enum must cover every case or have `else`.
5. **No arbitrary-code escape hatch.** There is deliberately no `eval`, no embedded general
   language, no `exec`. Logic is written in Vela functions; anything Vela cannot express is a
   *capability* added via the plugin ABI (`ARCHITECTURE.md §6.4`). This is the single most
   opinionated decision in the language and it is the reason the analyzer can be sound.
6. **One statement per line.** Blank-line-insensitive, deterministic formatting. Clean diffs.

## 2. Lexical structure

- **Encoding**: UTF-8. A BOM is rejected (`E0001`).
- **Newlines**: `\n` or `\r\n`, normalized. `\r` alone is `E0002`.
- **Indentation**: spaces only. A tab in leading whitespace is `E0003`. The corpus is
  formatted at 4 spaces; indentation width within a block must be consistent (`E0004`).
  Mixed widths at the same level are `E0005`.
- **Comments**: `#` to end of line. No block comments. A comment is *trivia with content*: no rule
  of the grammar mentions one, so the parser never sees it — but a tool that rewrites a file must
  put it back, and a tool that documents code has to read it, so the lexer records every comment
  beside the tokens rather than dropping it (`TOOLING.md §3`).

  The comment block directly above a declaration **is that declaration's documentation**. This is a
  rule, not a marker: consecutive comment lines, the last of which is the line before the
  declaration, with nothing between them. A blank line ends the block — a comment set apart from
  what follows is about the file, not about the declaration — and a comment written after code on
  the same line documents nothing, because it is about the code it sits beside.
- **Line continuation**: a line ending in `\` continues (used for long expressions).
- **Identifiers**: `[A-Za-z_][A-Za-z0-9_]*`.
  - values and modules: `snake_case`
  - types and enum variants: `PascalCase`
  - constants: `SCREAMING_SNAKE_CASE`
  A name that violates its convention is `W1001` (lint, not an error).
- **Reserved words**: `and as at call character const default elif else enum false fn for
  from hide if image import in is jump label match menu none not or pause play queue return
  scene screen show stop struct style theme transform true use var wait when while with`
- **Integer literals**: decimal, or hexadecimal with a `0x` prefix; `_` separators are
  allowed (`1_000`, `0xff_00_00`). A malformed or out-of-range literal is `E0006`.
  Hexadecimal exists because colours are naturally written that way, and the obvious
  spelling `#rrggbb` would collide with the comment marker.
- **Float literals**: `1.0`, `1e3`, `1.5e-2`. A trailing `.` is `E0007`.
- **String literals**: `"..."` with escapes `\" \\ \n \t`.
  `[expr]` inside a string is **interpolation** (§5.5), evaluated in the surrounding scope.
  Unterminated string is `E0008`.
- **Two sigils, two jobs.** `[` interpolates a value; `{` is **reserved for text tags**, which are
  not implemented (`E0010`). Doubling is the escape: `[[` is a literal `[` and `{{` a literal `{`.
  A backslash before either (`\[`, `\{`) is also literal — an escape that does nothing is
  normalized away by the formatter, which always writes the doubling.

  The split is deliberate and it is Ren'Py's, for the same reason: both things live *inside*
  dialogue, and one sigil cannot be both. `"{b}Hi{/b}"` has to mean bold while `"Score: [score]"`
  means the number, and with one sigil the first reads as "interpolate `b`". That is also why a
  brace is an **error** rather than a literal today: the two readings differ in meaning, and
  accepting the wrong one now would silently change what already-written dialogue says on the day
  tags arrive.
- **Path literals**: `@"assets/forest.png"` — a compile-time-checked asset reference (§7.5).

## 3. Grammar

EBNF (`{ x }` = zero or more, `[ x ]` = optional, `|` = alternative).

```ebnf
program    = { top_item } ;

top_item   = use_decl | const_decl | default_decl | effect_decl
           | struct_decl | enum_decl
           | character_decl | image_decl | transform_decl
           | screen_decl | style_decl | theme_decl
           | fn_decl | label_decl ;

(* --- declarations --- *)
use_decl       = "use" module_path [ "as" IDENT ] ;
module_path    = IDENT { "." IDENT } ;

const_decl     = "const" IDENT [ ":" type ] "=" expr ;
default_decl   = "default" IDENT [ ":" type ] "=" expr ;
              (* World state: persisted in saves, reset on New Game *)

(* An effect is a capability the host provides (RUNTIME.md §3). It has no body: the
   declaration is how a script says what it needs, not how it is implemented. The name is
   dotted because effects are grouped by capability, and a segment may be a reserved word —
   `audio.play` is the obvious case. *)
effect_decl    = "effect" module_path "(" [ params ] ")" [ "->" type ] ;

fn_decl        = "fn" IDENT "(" [ params ] ")" [ "->" type ] block ;
params         = param { "," param } [ "," ] ;
param          = IDENT ":" type [ "=" expr ] ;
label_decl     = "label" IDENT block ;

struct_decl    = "struct" IDENT block_field+ ;     (* indented field list *)
block_field    = IDENT ":" type [ "=" expr ] ;
enum_decl      = "enum" IDENT variant+ ;
variant        = IDENT [ "(" params ")" ] ;

character_decl = "character" IDENT block ;
image_decl     = "image" IDENT "=" ( path_lit | expr ) ;
transform_decl = "transform" IDENT block ;
screen_decl    = "screen" IDENT "(" [ params ] ")" block ;
style_decl     = "style" IDENT [ "from" IDENT ] block ;
theme_decl     = "theme" IDENT block ;
block          = ":" NEWLINE INDENT { stmt } DEDENT ;

(* --- statements --- *)
stmt       = say_stmt | menu_stmt | jump_stmt | call_stmt | return_stmt
           | scene_stmt | show_stmt | hide_stmt | with_stmt
           | play_stmt | stop_stmt | queue_stmt
           | pause_stmt | wait_stmt
           | if_stmt | while_stmt | for_stmt | match_stmt
           | var_stmt | assign_stmt | expr_stmt ;

(* A statement starting with STRING, or with one or more IDENTs followed by a
   STRING on the same line, is a say_stmt. This is the one ambiguity in the
   grammar and it is resolved by one token of lookahead. Image attributes go
   BEFORE the string (see §10 open question 5). *)
say_stmt   = [ speaker ] STRING { opt } [ "with" IDENT ] ;
speaker    = IDENT { IDENT } ;            (* character, then image attributes *)
opt        = "(" IDENT "=" expr { "," IDENT "=" expr } ")" ;

menu_stmt  = "menu" [ STRING ] ":" NEWLINE INDENT choice+ DEDENT ;
choice     = STRING [ "if" expr ] block ;

jump_stmt  = "jump" label_ref ;
call_stmt  = "call" label_ref [ "with" IDENT ] ;
return_stmt= "return" [ expr ] ;

scene_stmt = "scene" image_ref { attr } [ "at" transform_list ] [ "with" IDENT ] ;
show_stmt  = "show" image_ref { attr } [ "at" transform_list ] [ "with" IDENT ] ;
hide_stmt  = "hide" image_ref [ "with" IDENT ] ;
with_stmt  = "with" IDENT ;
transform_list = IDENT { "," IDENT } ;

play_stmt  = "play" IDENT expr { "loop" | "fade" FLOAT } ;
stop_stmt  = "stop" IDENT [ "fade" FLOAT ] ;
queue_stmt = "queue" IDENT expr ;
pause_stmt = "pause" [ expr ] ;
wait_stmt  = "wait" ( expr | "click" ) ;

var_stmt   = "var" IDENT [ ":" type ] "=" expr ;
assign_stmt= assign_target ( "=" | "+=" | "-=" | "*=" | "/=" ) expr ;
assign_target = IDENT { "." IDENT | "[" expr "]" } ;
expr_stmt  = expr ;

if_stmt    = "if" expr block { "elif" expr block } [ "else" block ] ;
while_stmt = "while" expr block ;
for_stmt   = "for" IDENT "in" expr block ;
(* A match block contains arms, not statements. *)
match_stmt = "match" expr ":" NEWLINE INDENT match_arm+ DEDENT ;
match_arm  = "when" pattern [ "if" expr ] block
           | "else" block ;
pattern    = module_path [ "(" IDENT { "," IDENT } ")" ] | "_" ;

(* --- expressions --- *)
(* The conditional is a *suffix* so the grammar is unambiguous: `a if c else b`
   parses as an expression, then `if`, rather than as two adjacent expressions.
   `match` is deliberately absent — LANGUAGE.md §10 defers match-as-expression. *)
expr       = or_expr [ "if" expr "else" expr ] ;
or_expr    = coalesce { "or" coalesce } ;
coalesce   = and_expr [ "??" coalesce ] ;
and_expr   = not_expr { "and" not_expr } ;
(* `not` and `!` are two operators, not two spellings: `not` binds looser than a
   comparison and `!` tighter than every binary operator, so `not a == b` and
   `!a == b` are different programs. *)
not_expr   = "not" not_expr | cmp_expr ;
cmp_expr   = add_expr [ cmpop add_expr ] ;
cmpop      = "==" | "!=" | "<" | "<=" | ">" | ">="
           | "is" | "is not" | "in" | "not in" ;
add_expr   = mul_expr { ("+" | "-") mul_expr } ;
mul_expr   = unary { ("*" | "/" | "%") unary } ;
unary      = ("-" | "!") unary | postfix ;
postfix    = primary { "." IDENT | "(" [ args ] ")" | "[" expr "]" } ;
primary    = INT | FLOAT | STRING | path_lit | "true" | "false" | "none"
           | list_lit | map_lit | module_path | "(" expr ")"
           | lambda ;
list_lit   = "[" [ expr { "," expr } [ "," ] ] "]" ;
map_lit    = "{" [ map_entry { "," map_entry } [ "," ] ] "}" ;
map_entry  = expr ":" expr ;
lambda     = "fn" "(" [ params ] ")" "->" expr ;
args       = expr { "," expr } [ "," ] ;

type       = type_atom { "?" } ;
type_atom  = "int" | "float" | "bool" | "str" | "none"
           | "list" "<" type ">"
           | "map" "<" type "," type ">"
           | "(" type { "," type } ")"
           | module_path ;
label_ref  = module_path ;
image_ref  = IDENT { "." IDENT } ;
path_lit   = '@' STRING ;
```

## 4. Statements and their semantics

### 4.1 Say
```vela
"Rain again."                          # narration
eileen "I hate the rain."              # character speech
eileen "Then let's go." with dissolve  # transition into the next block
eileen sad "I'll wait."                # attribute selects a variant image
eileen "Hi." (volume=0.5)              # say-scoped options
```
`eileen` must be a declared `character` (`E5001`). `sad` must be a declared attribute for
that character's image set (`E5002`). A bare IDENT in say position that is not a character
is `E2001`.

### 4.2 Menu
```vela
menu "What do you say?":
    "Tell the truth" if trust > 3:
        trust += 1
        jump confession
    "Lie":
        jump cover_up
```
Every choice body is a block; falling off the end of the last choice continues after the
menu. A choice with an `if` that can never be true on any path is `W4001`. A menu where
*every* choice is statically unreachable is `E4005`.

### 4.3 Flow control
`jump` transfers; `call` pushes a return frame; `return` pops (or ends a top-level label).
Targets are resolved at compile time — an undefined label is `E5003`, and a `jump` to a
label in another module requires a `use` (`E2002`).

The compiler builds the **story graph** (`vela-hir::StoryGraph`): nodes are labels, edges
are jump/call/menu-fallthrough. This graph powers four diagnostics for free:
- `W4002` unreachable label
- `W4003` label with no terminating `return`/`jump` on some path
- `E4004` a menu where all choices eventually jump back to the same node (dead loop)
- `W4004` call depth that provably cannot terminate

### 4.4 Statements are opt-in complexity
This progression must compile at every step:

```vela
label start:
    "Hello, world."
```
```vela
label start:
    "Hello, world."
    show eileen at center
    eileen "Hi there."
    menu:
        "Wave":
            jump wave
        "Leave":
            jump leave
```
```vela
label start:
    var warmth: int = 0
    show eileen at center
    match warmth:
        when x if x > 5: eileen "You're glowing."
        when 0: eileen "..."
        else: eileen "It's cold."
```

## 5. Type system

### 5.1 Primitive types
`int` (64-bit signed), `float` (64-bit IEEE-754, operations pinned for determinism),
`bool`, `str` (immutable, UTF-8), `none`, `list<T>`, `map<K,V>` (insertion-ordered).

### 5.2 Nominal types
`struct` (product) and `enum` (sum, with payloads). Both are declared, never structural.
Enums may not be empty (`E3001`). Recursive types are allowed only through `list` or a
declared indirection, to keep the serializer's schema finite.

### 5.3 Optionals
`T?` is sugar for "T or none". `none` is assignable only to `T?`. There is **no implicit
null**: use `match` to unwrap, or `x ?? default`. Unsafe unwrap is `E3002`; there is no
`!` operator.

### 5.4 Inference
Local `var` declarations infer from the initializer. Module `default` declarations may omit
the type only if the initializer's type is unambiguous (literal, constructor, or `const`).
Function parameters and return types are always explicit (`E3003`). Inference never widens
across branches without a declared type (`E3004`) — this keeps error messages local.

### 5.5 String interpolation
`"Score: [score]"` desugars to a concatenation of `str(score)`. The interpolated expression
must be `str`-convertible; a value with a struct type requires an explicit conversion
(`E3005`) so output formatting is never implicit. Float interpolation uses the pinned
formatter (`RUNTIME.md §4`) — this is a determinism requirement, not a style choice.

### 5.6 Exhaustiveness and reachability
- `match` on an enum must list every variant or have `else` (`E4001`).
- Arms after a wildcard/`else` are unreachable (`W4005`).
- `if` with a statically-known constant condition warns (`W4006`).
- A function whose declared return type is non-`none` must `return` on every path
  (`E4002`).

### 5.7 No implicit conversions
No `int`↔`float`↔`str` coercion. `int(x)`, `float(x)`, `str(x)` are explicit. Arithmetic
between `int` and `float` is `E3006` — this eliminates an entire class of display bugs and,
more importantly, makes arithmetic results deterministic and obvious.

## 6. Modules and namespacing

A file is a module; its path under `src/` is its name (`chapters/forest.vela` →
`chapters.forest`). Modules are imported with `use`.

```vela
use chapters.forest as forest
jump forest.clearing
```

**There is no global label namespace.** A flat namespace is one of the main scalability
failures of a long-running project: every story added to it is a chance of a collision. Names are
qualified, and the LSP can rename across modules safely.

Visibility: everything is private to its module unless marked `pub` on the declaration.
Unused `use` is `W1002`.

### 6.1 Linking

A story is written as several modules and **runs as one**. `vela build` links them into a single
program: every label becomes `module.label`, so the entry point written in `vela.toml`
(`entry = "main.start"`) is also that label's name in the image, and `jump forest.clearing` is a
call to the label `chapters.forest.clearing`. There is no module left at run time — which is what
`vela-mir`'s resolved-by-lowering `LabelRef` was always for.

A reference may be written with the alias or with the module's full path; linking resolves either
to the module it names. Two modules that cannot be told apart are refused by the linker, naming
both files, because neither module could have seen the other:

- two modules declaring the same `default`: world state is global, so one name cannot be two slots;
- two modules declaring one effect with different signatures: an effect is a capability the host
  provides, named once for the whole engine.

**What crosses a module boundary, and what does not.** *Labels* do — that is the story graph, and
it is what a project is split along. *Values* do not: `forest.helper(2)` is `E2005`, because
checking is per-module (§5: types do not cross either, for the same reason — no signature is in
scope). A qualified name used for anything but `jump` and `call` is therefore **refused** rather
than lowered into something that faults when a player reaches it.

## 7. Declarations

### 7.0 Reserved words

**A reserved word is special at the start of a line, and an ordinary name everywhere else.**

```vela
label pause:             # a declaration named `pause`
    scene room           # a scene statement, because `scene` opens the line
    var scene = 1        # a variable named `scene`
    scene = 2            # assigned, because the line assigns rather than declares a scene
    scene()              # called, likewise
    show eileen.image    # and a member may be any word at all
```

The leading word decides what a line *is*. After that, words are free: `struct image`,
`label pause`, `enum menu`, and `eileen.image` are all names, and an expression reads them
like any other value. The point is that `scene`, `image`, `menu`, and `pause` are exactly
the vocabulary a visual novel reaches for, and a language that forbade them would be
fighting its own subject matter.

Two things are deliberately *not* names:

- **Expression words.** `true`, `false`, and `none` are literals; `if` and `fn` open forms
  of their own. They are caught before the general rule applies, which is why they cannot be
  shadowed.
- **A line's first word when the rest of the line continues its statement.** `scene room` is
  a scene statement even though `scene` may also be a variable. One token settles it: a
  scene statement is followed by an image, and an expression is followed by `=`, `(`, or the
  end of the line.

That last rule is the only place the leading position is ambiguous, and it is one token of
lookahead in one function — the same mechanism the grammar uses for narration versus a
character speaking.

**A speaker names a character, not a value.** `eileen "Hi."` resolves `eileen` among the module's
`character` declarations, which is a namespace of its own: a `var eileen` in the same body neither
shadows it nor is shadowed by it, and the checker says "no character named `eileen`" rather than
"undefined name". Ren'Py needed a whole `character.` store to escape the collision between a
character and a variable of the same name; the answer here is that the position is not a value
position at all, so there is nothing to collide with.

### 7.1 Character
```vela
character eileen:
    name = "Eileen"
    color = 0xf2a2b0
    image = "eileen"          # image set used for `show`/say attributes
    voice = @"audio/eileen/"  # directory prefix for voice lines
```

### 7.2 State
```vela
const MAX_TRUST = 10
default trust: int = 0        # persisted, reset on New Game
```
There is no `global`. Persistent state is `default`; ephemeral state is `var` inside a
label/function body. This split makes the save schema derivable *from the source* and keeps
save/load honest (`RUNTIME.md §5`).

### 7.3 Struct / enum
```vela
struct Route:
    name: str
    unlocked: bool = false

enum Ending:
    good
    bad(reason: str)
    secret(hint: str?)
```

### 7.4 Transform
```vela
transform fade_in:
    alpha 0.0 -> 1.0 over 0.4s ease_out
    x 0.0 -> 1.0 over 0.4s
```
Transforms are declarative and composable; see `SCREENS.md §6` for the animation model.

### 7.5 Assets are checked, not stringly-typed
`@"path"` is a path literal. The compiler resolves it against the project's asset manifest
at build time; a missing asset is `E7001` and an unused asset is `W7001`. The common
"works on my machine" bug — a file renamed on one machine and missing on another — is a
compile error here rather than a blank rectangle in front of a playtester.

**Checked, but not *typed* — deliberately, and for now.** A path literal has type `Unknown`, so it
can be passed where a path is expected and is resolved at build time, but it cannot be stored in a
`struct` field, held in a `default`, or declared as a parameter type: writing `fn play(track: Audio)`
is an error, not a feature that is missing. The reason is that asset identity is a *build* fact —
`BUILD_AND_ASSETS.md §9` answers it against the manifest, and the manifest is not part of the
program's type environment. Making assets a real type is what would let the save schema and a
plugin's ABI name one, so it is a change to agree on before either of them ships, not a detail to
discover while writing a struct.

> **Implemented (M9).** `E7001` is reported by `vela check` for every `@"path"` that
> names nothing in the project's manifest, underlined at the literal itself.
>
> Path literals are recorded **where they are parsed** (`ParseResult::paths`) rather than found
> later by walking the tree. That is the difference between a check that is complete by
> construction and one that is complete until someone adds a statement that takes a path and
> forgets to extend the walk: an `image` declaration and a `play` statement are both covered
> today, and so is whatever gains one next. A `@"…"` inside a comment or a string is not a path
> token and is not recorded, which a scan for the characters `@"` could not promise.
>
> `vela check` imports the project's `assets/` to answer this rather than reading the last
> build's `dist/manifest.json`: a check trusting a stale manifest would report about the assets
> as they were, and the editor and the build disagreeing is the failure this check exists to
> prevent. A single file compiled on its own has no manifest and is not checked — "compiled out
> of its project" is not "referenced a missing asset".
>
> **Not yet.** `W7001` (unused asset) needs a span to point at, and the manifest is not a source
> file the session knows about; until it is, there is nowhere honest to hang the diagnostic.
> `vela build` does not run the check either, because it does not compile scripts yet.

### 7.6 Tests

A `test` item is a test, and it lives in the same files as the story it exercises (`TOOLING.md §5`)
rather than in a format of its own:

```vela
test "picking the forest sets trust":
    run from chapters.forest.clearing
    choose "Explore"
    advance 4
    expect trust == 1
    expect visited(chapters.forest.river)
```

One thing here is a keyword and five things are not.

**`test` is reserved** — the parser has to know where a test begins, and no story has another use
for a heading called `test`. It is an item like `label` or `screen`, so §7.0's rule applies to it
unchanged: it is special at the start of a line, and a name everywhere else.

**The directives are contextual.** `run`, `advance`, `choose`, `expect`, and `cover` are ordinary
identifiers in the first position of a line inside a test, looked up by name — which means a story
keeps every one of them as a label, a variable, or a function. That is not a coincidence: `expect`
and `run` are exactly the words a story about expecting and running would use, and reserving them would
cost the language's own subject matter to buy the parser a table lookup. §7.0's doc comment in the
parser records four separate bugs from that trade being made the other way, which is why this one is
made deliberately.

Their arguments are parsed as the things they are, not as strings:

- `run from <label>` carries a dotted path *and* that path's own span, the pair a `jump` target
  carries, so a reference written in a test resolves the way every other reference resolves (`§6`) —
  and a rename can edit it.
- `expect` and `choose` take expressions rather than strings. That is what will let the checker type
  them — an assertion that could never hold should be a diagnostic before it is a failing run — and
  what lets a test name the menu text once and use it twice. Until the HIR records the directives,
  nothing checks them: the tree holds a real expression, and the checker has not been given it yet.
- `cover` takes `labels` or `variants`, and an unknown word is an error rather than a directive that
  quietly covers nothing.

```vela
test "every route reaches an ending":
    run
    cover labels
    cover variants
```

A `test` is not story content: it neither defines a name nor adds a node to the story graph, and
`vela build` does not carry it into a bundle.

> **Implemented (M10).** The item parses, formats, and prints as canonical
> (`tests/golden/parse/item_test.vela` is in the corpus every formatting gate covers), and an unknown
> directive or cover word is reported once, with the line it is on.
>
> An `expect` and a `choose` are typed by the checker — `expect trust` where `trust` is an `int` is
> `E3007`, because an assertion that cannot hold should be a diagnostic rather than a failing run — and
> a `run from` is resolved like any other reference: `E5003` for a label that does not exist, `E2002`
> for a module the file did not import. The reference is collected *beside* the story graph rather than
> in it, because a test does not run during the story and an edge into a label would be a claim about
> what the story does.
>
> **Not yet.** Everything that *runs* one: evaluating an assertion against the world, matching a
> `choose` against a menu, `cover`, and golden frames. `vela build` ignores the item, so a bundle
> carries no tests. That runner is the next thing in this milestone.

## 8. Diagnostics

Codes are permanent: never reused, never renumbered. Ranges are reserved by phase so a new
diagnostic has an obvious home.

| Range | Phase | Examples |
| --- | --- | --- |
| `E0xxx` | Lexical | `E0001` BOM, `E0003` tab indent, `E0008` unterminated string |
| `E1xxx` | Syntax | `E1001` unexpected token, `E1002` expected block |
| `E2xxx` | Names | `E2001` undefined name, `E2002` missing `use`, `E2003` duplicate definition, `E2005` a value in another module |
| `E3xxx` | Types | `E3001` empty enum, `E3002` unwrap of `T?`, `E3006` int/float mixing |
| `E4xxx` | Control flow | `E4001` non-exhaustive match, `E4002` missing return, `E4005` all menu choices unreachable |
| `E5xxx` | Story graph | `E5001` undefined character, `E5003` undefined label |
| `E6xxx` | Internal (bytecode verify) | `E6001` stack underflow — always a compiler bug, reported with a repro |
| `E7xxx` | Build/assets | `E7001` missing asset, `E7002` manifest digest mismatch |
| `W1xxx` | Style lints | `W1001` naming convention, `W1002` unused import |
| `W4xxx` | Suspicious code | `W4002` unreachable label, `W4005` unreachable match arm |
| `W7xxx` | Build lints | `W7001` unused asset, `W7002` oversized asset |

Every diagnostic carries: code, severity, primary `Span`, zero or more labeled secondary
spans, an optional machine-applicable suggestion (`Suggestion::Replace(span, text)`), and a
documentation anchor. A diagnostic without a stable code and a test is a build failure
(`CONVENTIONS.md §3`).

## 9. Determinism constraints visible to authors

The language surfaces determinism as rules an author can rely on:

- `rand()` advances the single `World` RNG; replay reproduces every call exactly.
- `map` iteration is **insertion-ordered**, forever. There is no unspecified order.
- `float` arithmetic has no platform-dependent fast-math; results are bit-identical across
  targets.
- No ambient clock. `now()` is an effect that reads the injected `Clock` and is recorded in
  the input log, so a save restores time-dependence exactly.
- Texture/audio loads never observe wall time; they are effects with recorded results.

An author who follows the language cannot accidentally write a non-replayable story.

## 10. Open questions

To be resolved during the milestones noted, each with a written decision record:

1. **Pattern matching depth** (M4): should `when` support nested destructuring and literal
   patterns, or only type-qualified + guards? Start narrow (type-qualified + guards), widen
   if real projects ask.
2. **`match` as an expression** (M4): useful, but interacts with exhaustiveness and
   inference. Deferred until match-as-statement is proven.
3. **Generics** (post-1.0): `list<T>`/`map<K,V>` are built-in. User generics are deliberately
   out of scope for 1.0 — they cost more in error-message quality than they return for this
   domain.
4. **Traits/interfaces for script code** (post-1.0): plugins cover the real need; script-level
   polymorphism may never be necessary.
5. **`say` attribute syntax** (M5): `eileen sad "..."` vs `eileen "..." at sad` — the former
   reads better; confirm against the image-attribute model.
