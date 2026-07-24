# devtool

amenbo's portable developer-support CLI — a single static Go binary (no runtime,
no venv) you can drop into any project regardless of its language.

It stamps out and tears down **per-task git worktrees** — and, on macOS, the
task's own throwaway **dev GUI** — so several implementation sessions can run in
parallel without stepping on each other, and it measures what a diff does to the
`amenbo agent --json` entry.

## Build

It is **optional**, and so is its toolchain: amenbo builds, tests and ships
without Go, and nothing outside this directory depends on it. Build it only if
you want it.

```sh
make devtool        # builds to ~/.cargo/bin/devtool
# or: cd devtool && go build -o ~/.cargo/bin/devtool .
```

## Model

A task's worktree lives **outside the repo**, in a sibling dir:

```
<repo>/../<repo-name>-worktrees/<id>/    git worktree checkout on task/<id>
```

Outside-the-repo is deliberate — the worktree is a **pure development
environment**. Two concerns are kept physically apart:

- **Project management** (status / comment / done) → the **prod
  `amenbo` binary, run from the MAIN repo**, against the real backlog. devtool's
  own reservation does exactly this (prod binary, anchored to the main worktree root).
- **Debug verification** (does my code work) → the worktree's **dev build**
  against a **throwaway store** (e.g. `make verify`), inside the worktree.

Because the worktree has no repo `.amenbo` in its ancestry, amenbo commands run
there cannot reach the real backlog — they fall to an isolated/throwaway store.
That is the containment guarantee, and it also makes `make verify` isolate
cleanly (no `.amenbo` ancestor to hijack the mktemp store).

### The task's own dev GUI

A GUI bundle is installed machine-wide, so a worktree cannot contain it. The
task gets its own throwaway instead — its own bundle identifier, product name
and app-data — and the shared `amenbo (dev)` app stays where it is, as the
permanent place a grown setup (plugins, catalog, projects) lives.

| | shared dev app | the task's instance |
|---|---|---|
| identifier | `work.amenbo.app.dev` | `work.amenbo.app.dev.<id>` |
| app-data | `amenbo-dev` | `amenbo-dev-<id>` |
| bundle | `/Applications/amenbo (dev).app` | `/Applications/amenbo (dev <id>).app` |
| built by | `make install-gui-dev` | `make install-gui-dev TASK=<id>` |
| deleted by | nothing — it is permanent | `devtool task finish <id>` |

So **verify a task in its own app**: with no hand reaching the shared bundle,
two parallel sessions cannot install over each other, and the collision is gone
by construction rather than by taking turns.

devtool provisions and deletes the instance; the Makefile builds it. That split
is deliberate — a bundle costs minutes to build and ~38MB on disk, so only the
tasks that actually look at a GUI pay for one. Beyond seeding that instance's
app-data, devtool provisions no amenbo store of its own.

All of this is macOS-only, which is where the dev GUI is installed at all;
elsewhere `task start` / `task finish` simply do not mention it.

## Commands

### `devtool task start <id> [--base main] [--no-reserve] [--no-deps]`

1. Reserves `<id>` (`task status <id> in_progress`, prod amenbo from the main
   repo) and verifies status is `in_progress`. Double-work is guarded by
   `status` alone, so `in_progress` **is** the reservation, and a same-status
   re-reserve is idempotent.
2. Adds a git worktree at `<repo>/../<repo-name>-worktrees/<id>` on a fresh
   branch `task/<id>` branched from `--base` (default `main`).
3. For a GUI checkout (`app/package.json` present) runs a **best-effort**
   `npm ci` in `app/` so the worktree is ready for `npm run typecheck/build/test`
   without a manual install. Each worktree keeps its own real (gitignored)
   `node_modules` — no symlink — so parallel sessions stay isolated. It never
   fails `task start`: a missing npm, offline registry, or failed install only
   warns. Skip it with `--no-deps`.
4. For a GUI checkout on macOS, seeds the app-data of the task's own dev GUI
   (`work.amenbo.amenbo-dev-<id>`) by cloning the shared dev store, so the
   instance opens on the setup grown in the shared app rather than an empty one.
   Best-effort in the same sense as the `npm ci`, and an app-data already sitting
   there is left alone. The bundle itself is not built here — that is
   `make install-gui-dev TASK=<id>`, run only when the task needs to look at it.
5. Prints an eval-able `cd` to **stdout** (diagnostics go to stderr):

```sh
eval "$(devtool task start <id>)"   # cd into the worktree
```

`--no-reserve` skips the reservation and only verifies an existing one (e.g. the
task is already `in_progress`).

It refuses to start when the reservation is not yours to take, and the refusal says
which case it is — because the two look the same on disk and only one of them is
yours to clear:

- **the worktree exists and the backlog holds the task `in_progress`** — another
  session is on it. Take a different task; do not look inside the worktree, judge
  whether it is stale, or delete it.
- **the worktree exists and the task is not `in_progress`** — a worktree you left
  behind. `devtool task finish <id>`.
- **the reservation was rejected (`already_reserved`)** — another session reserved it
  first. The reservation is a compare-and-swap, so it only takes from `todo`.

### `devtool task finish <id> [--base main] [--force] [--reset]`

Safely tears it down. Without `--force` it **refuses** unless:

- the worktree has no uncommitted changes, **and**
- branch `task/<id>` is merged into `--base`.

Then it removes the worktree and deletes the branch, and prunes the
`<repo>/../<repo-name>-worktrees` base dir once it holds no other worktree. It
also deletes the task's dev GUI — the `/Applications` bundle **and** its
app-data, naming each path it removed. Those live outside the worktree, so this
is the only place they are reclaimed; skipping it costs ~38MB per task, forever.
`--reset` also returns the task to `todo` (`amenbo task status <id> todo`;
otherwise the `in_progress` status is left as-is — finish your task with
`amenbo task done <id>`). `finish` works whether you run it from the main
checkout or from inside the worktree.

### `devtool agent size [--base main] [--json]`

Prints what this tree does to the size of the `amenbo agent --json` entry, section
by section:

```
agent --json entry size — this tree vs merge-base with main (ab513f194c7f)

  section                 base      head     delta
  notes                  5,922     4,100    -1,822
  ...
  TOTAL                 48,794    46,972    -1,822
```

**A signal, not a gate — it always exits 0.** The entry is what every AI session
reads first, so bytes landing there are paid once per session, forever. A byte
ceiling would not hold: the constant is raised by whoever trips it, and the thing
worth catching (rationale written in the voice of a spec) hides inside
legitimately long fields, where no size check can see it. Only a reader can tell a
spec from an argument, and only while writing it. So the delta puts that question
to the author instead of answering it.

- **head** is this tree as it stands — uncommitted changes included.
- **base** is the merge-base with `--base`, so what others landed on main is not
  billed to your diff.
- The base is built in a **persistent rig** (a detached worktree under the user
  cache dir) and the result is cached by commit SHA. A throwaway worktree has no
  `target/`, so every run would pay a cold build; the rig only ever holds base
  commits, so it stays warm. A base that has not moved costs nothing.
- Measurement goes through `make verify`, which is what pins the isolation (a
  throwaway `AMENBO_HOME` **and** a CWD with no `.amenbo` ancestor) — the real
  store is never read.

## Env

- `AMENBO_BIN` — backlog binary for `status`/`show` (default `amenbo`). Set to
  `amenbo-dev` to drive an isolated store in tests.
