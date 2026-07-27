# devtool

amenbo's portable developer-support CLI — a single static Go binary (no runtime,
no venv) you can drop into any project regardless of its language.

It stamps out and tears down **per-task git worktrees** — and, on macOS, the
task's own throwaway **dev GUI** — so several implementation sessions can run in
parallel without stepping on each other, it measures what a diff does to the
`amenbo agent --json` entry, and it stands up a **fake outside world** the dev
GUI can be verified against — including the failures the real one will not
produce on demand.

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
| executable | `amenbo-app-dev` | `amenbo-app-dev-<id>` |
| header badge | `DEV` | `DEV AMB-T-<id>` |
| the CLI it names | `amenbo-dev` | `amenbo-dev` — it installs none of its own |
| built by | `make install-gui-dev` | `make install-gui-dev AMB-T-ID=<id>` |
| deleted by | nothing — it is permanent | `devtool task finish <id>` |

Each build runs under an executable name of its own — production keeps
`amenbo-app` — so a name reaches one app and not another: `pgrep -x
amenbo-app-dev-<id>` finds that one instance, and `System Events` lists it under
the same name. A *click*, though, still lands on whichever window is in front.
The badge is how you tell them apart *inside* the window: it sits in the header,
so it survives a cropped screenshot, and production carries none at all. To
reach one without a click, ask for its pid (`devgui pid` below) and drive that
pid — the badge tells you afterwards what you shot, the pid decides beforehand
what you shoot.

So **verify a task in its own app**: with no hand reaching the shared bundle,
two parallel sessions cannot install over each other, and the collision is gone
by construction rather than by taking turns.

What the instance opens on is the shared dev store as it was when the task
started. Anything a screen needs *beyond* that is put there with
[`task cli`](#devtool-task-cli-id---no-build----amenbo-args) below, in the
instance's own store — never in the shared one, which no task may edit.

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
   `make install-gui-dev AMB-T-ID=<id>`, run only when the task needs to look at it.
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

### `devtool task cli <id> [--no-build] -- <amenbo args…>`

Runs an amenbo command against **the store the task's own dev GUI reads**, so a
screen can be given something to show. A dev GUI shows what is in its store: a
rejected task, a card with a due date, a plugin in some state all have to be
*put there* before the screen that renders them can be looked at.

```sh
devtool task cli 696 -- --actor human --project myproj task add --title 'due today' --due today
devtool task cli 696 -- --actor human --project myproj task reject 5 --reason 'out of scope'
```

The CLI is the **worktree's own** `target/debug/amenbo` — rebuilt first, unless
`--no-build` — pointed at that store with `AMENBO_HOME`. Nothing is built per
task that was not being built anyway: the app-data name is fixed at build time
(`AMENBO_APP_NAME`), but what it selects is a *directory*, and `AMENBO_HOME`
names the same one at run time — the seam `make verify` already isolates
through. Before this existed the only way in was to rebuild the CLI with
`AMENBO_APP_NAME=amenbo-dev-<id>`: two minutes, for a binary one task could use.

Details worth knowing:

- **Arguments go after `--`.** Without it a `--json` of amenbo's would be read
  as a flag of devtool's.
- **The exit code is amenbo's**, so a seeding step that failed fails visibly.
- **It runs in the store's own directory.** Relative paths resolve there, and a
  `bind` writes its `.amenbo` beside the store it points into — which teardown
  reclaims with the rest of the instance. In the worktree that same pointer
  would be a live one for *any* amenbo run there, the production binary
  included, which is the reach the worktree is kept outside the repo to deny.
- **The binary still introduces itself by its own channel** (it was not built
  with `AMENBO_APP_NAME`), so what is keyed to the channel rather than the store
  — the command name in guidance text, the perf log's default — reads as
  production. It writes the right store; it says the wrong name doing it.
- **It will migrate that store if the tree is ahead of it.** An isolated store
  is an arm of the migration gate, and that is the wanted answer here: the
  task's own GUI is built from the same tree and would carry it forward the
  moment it opened.

macOS only, like everything else about the per-task instance.

### `devtool task finish <id> [--base main] [--force] [--reset]`

Safely tears it down. Without `--force` it **refuses** unless:

- the worktree has no uncommitted changes, **and**
- branch `task/<id>` is merged into `--base`.

Then it removes the worktree and deletes the branch, and prunes the
`<repo>/../<repo-name>-worktrees` base dir once it holds no other worktree. It
also deletes the task's dev GUI — the `/Applications` bundle **and** its
app-data, naming each path it removed. Those live outside the worktree, so this
is where they are reclaimed; skipping it costs ~38MB per task, forever
(`devgui sweep` is the way back for a task that skipped it).

A **running** instance is asked to quit first, and if it will not, both halves
are left where they are. Deleting the store under a running instance does not
remove it — it writes the store back on the way out, and the teardown that
reported it reclaimed reads as green while the leftover returns minutes later.
`--reset` also returns the task to `todo` (`amenbo task status <id> todo`;
otherwise the `in_progress` status is left as-is — finish your task with
`amenbo task done <id>`). `finish` works whether you run it from the main
checkout or from inside the worktree.

### `devtool devgui pid [<id>] [--front]`

Prints on **stdout** the pid of a running dev GUI, so it can be handed straight
to the tools that take one:

```sh
# the dev GUI this checkout launches, fronted, and its window id for screencapture -l
swift app/scripts/uiauto/uiauto.swift window "$(devtool devgui pid --front)"
```

The bundle a process was executed out of is what the lookup matches, and only the
app process itself, so what comes back is a pid a window actually belongs to.
That is one step finer than a name: an executable name reaches the right
instance, but a helper process a bundle spawns carries none of its own.
`System Events`' front window is no help at all — it answers with whichever app
is in front, in practice the **production** one.

- With no `<id>` it answers for the dev GUI **this checkout** launches — a task
  worktree's own instance ahead of the shared app, the same order a launch takes.
  An `<id>` names one instance directly, from anywhere.
- `--front` activates it first. uiauto skips a window behind another Space, and
  a shot of a window nobody fronted is a shot of whatever is over it.
- Nothing running is a **non-zero exit** with the build command named, not an
  empty answer that reads as a pid of zero.

### `devtool devgui sweep [--yes]`

Lists every per-task dev GUI on this machine and says which ones no worktree
claims any more. `task finish` is the only other thing that deletes an instance,
so a session that died — or one that never ran it — leaves ~38MB of bundle plus a
store behind under a number nobody will type again.

- **An instance a worktree still owns is never touched, and never offered.** That
  is the same hands-off line a pre-existing worktree draws elsewhere: the
  worktree is the evidence a session owns that number, and whether the session is
  "really" still working is not this command's to judge.
- If `git worktree list` cannot be read, it **refuses** rather than guess — an
  unreadable answer would read as "nothing is claimed", i.e. delete everything.
- Reporting is the default; `--yes` reclaims. What goes is a store, and the
  report is the review.
- A running instance is asked to quit and skipped if it will not (see
  `task finish` above).

Only the digits form an instance: a hand-made `amenbo (dev wip).app` is
somebody's own, and the shared `amenbo (dev)` app is permanent.

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

### `devtool fixtures refresh [--catalog <url|path|repo dir>] [--amenbo <bin>] [--repo owner/name]`

Captures the outside world into `devtool/fixtures/`, from the real sources:

```
devtool/fixtures/catalog.json                          the plugin catalog's list
devtool/fixtures/plugins/<name>.json                   one plugin's detail — what an install reads
devtool/fixtures/update/latest.json                    the update check's answer
devtool/fixtures/github/repos/<owner>__<name>.json     /repos/{repo}
devtool/fixtures/github/releases/<owner>__<name>.json  /repos/{repo}/releases/latest
devtool/fixtures/github/readme/<owner>__<name>.md      /repos/{repo}/readme
```

**They are copies, never written by hand.** A hand-written fixture drifts from
what the producer actually sends, and the mismatch shows up as a green check over
a broken screen — an aggregation that quietly stopped copying a field is the kind
of thing only a real capture catches. The plugins whose details are taken and the
repositories fetched are the ones the catalog itself names, so no list is kept
beside it to go stale; `--repo` adds one the catalog does not name yet.

`--catalog` takes the envelope from somewhere else — a URL, or the path of a copy
some other run generated. The details are taken from beside it, the same way they
are published.

**Point it at a checkout of the catalog repository and the catalog is built from
the manifests**, which is the answer while the published catalog lists nothing:
there is no copy to take, and the reviewed manifests are the material either way.

```sh
devtool fixtures refresh --catalog ../amenbo-plugins
```

It runs the same aggregation the catalog's CI runs, in the one way a developer
can: the split into a list entry and an install detail is `plugin validate --json`'s,
so nothing here holds a second copy of amenbo's schema, and what is added is what
only an aggregation knows — the digest of the detail as written, when the manifest
first landed (git), and the curation list's recommendation. Validation needs a
build that carries the plugin commands, so it uses **this checkout's** (`--amenbo`
picks another); the released CLI on the PATH does not have them yet.

What it cannot do it does not fake: signing each distributable takes a key only the
catalog's CI holds, so the details carry the manifest's own `url` / `checksum` and
no signature. The fake catalog is one whose plugins can be browsed and opened, and
do not install — which is what the market, the detail view and the update banner
are looked at with. An install is exercised against the real thing.

### `devtool fixtures gui [--fail <face>=<mode>] [--fresh] [--port n] [--app path] [--no-launch]`

Serves those fixtures on a local host and starts the dev GUI pointed at it,
through the three overrides the app already reads (`crates/amenbo-core/src/env.rs`)
— there is no development-only branch in the product. The GUI it starts is this
checkout's own instance when it has one, and the shared dev app otherwise;
either way the launch line names the binary, and `--app` picks one by hand — the
bundle (`/Applications/amenbo (dev <id>).app`) or the executable inside it, since
the executable is what the launch takes and the bundle is what a person has:

| face | env var | what it answers |
|---|---|---|
| `catalog` | `AMENBO_PLUGIN_CATALOG_URL` | the market list, catalog registration |
| `github` | `AMENBO_GITHUB_API_URL` | one opened plugin's stars, downloads, README |
| `update` | `AMENBO_UPDATE_JSON_URL` | the update banner |

**`--fail` is the half the real world cannot be asked for.** `--fail github=429`
is a rate limit on demand, `--fail all=timeout` is a request that never comes
back, and any status works (`404`, `500`). These are the branches that never get
exercised against the real API, because the way to reach them there is to spend
the quota or unplug the network.

**The fake world serves two catalogs, and the second one is registered for you.**
The official one is the capture; beside it sits an invented third-party catalog
(`In-house catalog`, two plugins, with a `catalog-key.pub` of its own), which
`fixtures gui` registers in the store the GUI is about to open and unregisters on
the way out. It has to be registered rather than pointed at by an env var,
because a registered catalog *is* a record in the store — and without one the
screens that only a second shelf produces cannot appear at all: a market row
badged with the catalog it came from, that catalog as a choice in the provenance
filter, the fingerprint shown before a key is pinned. The line it prints says how
many plugins joined the merged view and which key was pinned. Nothing on it is
signed, so it stops at browsing; an install is exercised against the real thing.

**Both of its entries claim `official: true`, and neither is entitled to it.** The
badge is the official index's to grant, and the merge clears the claim on
everything a registered catalog serves — so the rows come up badged with the
shelf's name. The claim is there to make that clearing visible: with the flag
off, the badge would read the same whether the merge folded or did nothing, and
nothing in the CLI reads an entry's own claim back. It is what
`verification/scenarios/plugin-browse.yaml` is looked at for.

**`--fresh` runs against a throwaway store** (`AMENBO_HOME`), so every cache
starts cold. Without it a catalog fetch is answered from disk for an hour and a
repository's figures for six, so the fake world is usually never asked and an
injected failure never bites. The cost is that the store is empty too: `--fresh`
is for looking at the market, the detail and the update banner, not at tasks.
Every request is logged, so "it did not ask" is distinguishable from "it asked
and the fixture was missing".

It replaces no test that talks to the real world: a fake answers what it was told
to answer, so it can only confirm what we already believe. The `#[ignore]`d tests
against the real API stay.

## Env

- `AMENBO_BIN` — backlog binary for `status`/`show` (default `amenbo`). Set to
  `amenbo-dev` to drive an isolated store in tests.
- `AMENBO_HOME` — not read, **set**: `task cli` puts the task's own store there
  for the CLI it runs. It is amenbo's own isolation seam, the same one
  `make verify` points at a mktemp store.
