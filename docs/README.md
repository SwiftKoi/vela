# Documentation

What lives where, what is normative, and what an update to each kind looks like. The repository
`README.md` is the map for a first read; this is the map for someone about to change one.

## What lives where

| Directory | Answers | Normative? | Updated by |
| --- | --- | --- | --- |
| [`VISION.md`](VISION.md) | Why Vela exists: principles, differentiators, non-goals | yes — the product | rarely, and deliberately |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Layers, crate map, data flow, key decisions | yes — the shape | a change to the crate graph or a rule |
| [`spec/`](spec/) | The contracts: language, bytecode, runtime, screens, tooling, build | yes — behaviour | **in the same commit** as the behaviour |
| [`engineering/`](engineering/) | How to work here: repository layout, conventions, the extension matrix | yes — the process | when a rule changes |
| [`roadmap/`](roadmap/) | One file per milestone: work items, exit criteria, what it cost | no — the plan | on completion, and when a discovery lands |
| [`guides/`](guides/) | Prose walkthroughs, every step a test | no | with the feature it describes |
| [`reference/`](reference/) | The generated reference | no — generated | `vela doc --out docs/reference`, never by hand |

`docs/` is the contract (`ROADMAP.md`, rule 2), which means two things. A document that contradicts
the code is a defect, fixed in the same pull request as the code. And nothing here claims behaviour
that does not exist without saying so — a gap is written down as a gap.

## How a spec file records implementation state

Each file under `spec/` opens with a line naming the milestones that must obey it:

```
Status: **normative for M<a>–M<b>.**
```

Inside, a section with something to say about its own implementation carries **one** blockquote,
starting with one of exactly three status tags:

| Tag | Means |
| --- | --- |
| `**Implemented (M<N>).**` | It exists. Say where, and name the test or command that shows it. |
| `**Not yet.**` | It does not. Say what it waits on. |
| `**Revised (M<N>).**` | Building it changed the contract. Say what changed — this is the spec-first rule catching itself. |

A fourth tag is allowed for something that is neither, and it is the *only* other one:
`**Note.**` marks a caveat about the contract itself — a limit, an aside, a consequence — so that a
reader scanning for *is this real?* can tell the two apart at a glance. A rule or an example belongs
in the section body rather than in a blockquote; a note is a caveat that would be lost there.

## How a milestone file is laid out

Every file under `roadmap/` has the same sections, in the same order:

```
**Goal.** / **Depends on.** / **Crates.**

## Work items
## Exit criteria                   — checkboxes, ticked only with evidence
## Status                          — one paragraph: what landed, in what shape
## Found during implementation     — the bugs, spec fixes, and lessons; the value of the file
## Still open                      — only when there is something
## Risks                           — only when the milestone still has one
```

A milestone is complete when every exit criterion is ticked and the demo command is demonstrated in
the pull request (`ROADMAP.md`, rule 8). Ticking one takes evidence rather than confidence: name the
test, the command, or the CI job. A criterion that cannot be checked on the development machine says
so instead of being ticked.

## How a guide is written

Prose with a test behind every step, so the guide can only drift by being edited in the same commit
as the behaviour — `guides/lsp-walkthrough.md` and `guides/debugger-walkthrough.md` both name the
test each step is asserted by. A gap is stated as a gap.

## Reference is generated

`reference/*` is what `vela doc --out docs/reference` writes, and
`crates/vela-cli/tests/doc_golden.rs` fails when the committed pages differ from the schemas. Do not
edit them by hand; change the schema and regenerate.
