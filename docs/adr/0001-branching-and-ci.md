# ADR 0001 — Branching, merging, and CI triggers

**Status.** Accepted.
**Date.** 2026-09-14.
**Supersedes.** Nothing.

## Context

CI runs seven architecture and policy checks (`REPO_LAYOUT.md §6`). They are the mechanism that
keeps the file-size budget and the rank rules from decaying, so they matter more than a typical
test suite: a structural violation is cheap to fix when it is introduced and expensive to fix a
hundred commits later.

The first commit landed directly on `main`, which makes it impossible to tell "green because it
was checked" from "green because nobody looked". Two questions had to be settled together,
because the answer to one decides whether the other is affordable:

- **When does CI run?** A run on every push to every branch spends CI on work that cannot merge
  yet, and makes `main`'s gate mean less.
- **What reaches `main`?** Squash-merging lands one commit per pull request. Rebase-merging
  lands every commit, which makes each intermediate commit part of `main`'s history — and
  therefore something that has to be green.

## Decision

### 1. Work happens on branches; `main` is always green

**One branch per implementation step.** A step is whatever lands as a single squash commit —
usually a whole milestone, sometimes something smaller. Branches are not per file, per commit,
or per work item: splitting a step across branches buys nothing, because the merge is a squash
either way, and the intermediate branches would have to be reconciled for no reason.

**Name a branch for the step, not for its contents — and do not rename it.** A branch named
after a sub-part goes stale the moment the step grows past that part, and renaming mid-flight
severs the link to the commit the branch was cut from and to anything already built against it.

| Name | Use |
| --- | --- |
| `m<N>` | A milestone step — `m1` for the whole front end. Traceable to `docs/roadmap/`. |
| `<type>/<slug>` | A step that is not a milestone: `chore/…`, `fix/…`, `docs/…`. |

A pull request is opened **when the work is complete**, not while it is being assembled.

### 2. CI runs in exactly three cases

- A push to `main` — verifies the commit that actually landed.
- A pull request **targeting `main`** — the merge gate.
- Manual dispatch — test a branch without opening a pull request.

Pushing to a feature branch with no pull request open does not start CI. Pushing five times while
building a branch therefore costs nothing; opening the pull request runs CI once, on the final
state.

### 3. Squash merge

Every pull request lands on `main` as a single commit. Two consequences:

- Intermediate commits on a branch never reach `main`, so they need neither CI nor greenness.
  This is what makes decision 2 affordable.
- `main` reads as one commit per work item, which is the history we actually want to scan and to
  `git bisect` through.

### 4. Superseded pull-request runs are cancelled

`cancel-in-progress` is enabled for pull requests only, grouped by ref. A pull request that is
open while fixes are pushed gets one live run rather than a queue of stale ones. A `main` run is
never cancelled, so a broken merge cannot be hidden behind a quick follow-up push.

## Consequences

- `main` is green because it was checked, and each commit on it corresponds to one reviewed work
  item.
- A branch's first CI signal arrives when the pull request is opened. A developer who wants it
  sooner runs the same gates locally (`cargo xtask all`, `cargo test --workspace`) — which is the
  intended inner loop anyway, since they are fast.
- The pull-request run tests the branch contents; the post-merge run tests the *squashed* commit,
  which is a new object that did not exist before. That second run is not redundant: it verifies
  the thing that actually shipped.
- Branch protection on `main` becomes meaningful — the required check is the pull-request run,
  and squash is the only enabled merge strategy.

## Alternatives considered

- **Run CI on every branch push.** Rejected as unnecessary rather than harmful: pull requests are
  opened when the work is done, so the extra runs would cover states that never merge.
- **Merge commits instead of squash.** Rejected: it clutters `main` with every in-progress commit,
  which is the history we have to read.
- **Rebase merge.** Rejected for the same reason, plus it makes every intermediate commit part of
  `main` and therefore something that must be individually green — requiring CI on every commit,
  the opposite of decision 2.
- **No CI on pull requests; only on push to `main`.** Rejected outright. That would make `main`
  the first place a change is ever tested, which for architecture guards inverts their purpose:
  they exist to catch structural decay before it lands, not after.
