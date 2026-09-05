# devtool

Amenbo's portable developer-support CLI — a single static Go binary (no runtime,
no venv) you can drop into any project regardless of its language.

On macOS it gives a task its own throwaway **dev GUI** — bundle, app-data and
all — so several implementation sessions can run in parallel without installing
over each other, and it stands up a **fake outside world** the dev GUI can be
verified against — including the failures the real one will not produce on
demand. It also raises the throwaway **macOS VM** that GUI is driven in — and
walks a pre-distribution screen road inside it — so verification takes a screen
that is not the one being worked on.

## Build

It is **optional**, and so is its toolchain: Amenbo builds, tests and ships
without Go, and nothing outside this directory depends on it. Build it only if
you want it.

```sh
make devtool        # installs to ~/.cargo/bin/devtool, to type by hand
# or: cd devtool && go build -o ~/.cargo/bin/devtool .
```

That copy is **for a person to type**, and it is one file for the whole machine.
The root makefile's own targets do not call it: they build `devtool/.bin/devtool`
from the checkout they are running in and call that, so what a build runs is the
tree in front of it rather than whoever installed last. Two checkouts each
installing their own is otherwise how a target comes to ask for a subcommand the
installed copy has not got — after the minutes it spent building.

## Model

The checkout a task is written in is a git worktree **outside the repo**, in a
sibling dir, cut by Amenbo's official `worktree` plugin:

```
<repo>/../<repo-name>-worktrees/<id>/    git worktree checkout on task/<id>
```

devtool reads that layout and cuts none of it. Three tools, three jobs, and none
of them reaching into another's: **Amenbo** holds the backlog, the **`worktree`
plugin** holds git, and **devtool** holds the one piece of isolation neither can
give — a GUI bundle, which is installed machine-wide and so cannot live in a
checkout at all.

Outside-the-repo is what makes the checkout a **pure development environment**.
Two concerns are kept physically apart:

- **Project management** (status / comment / done) → the **prod `amenbo` binary,
  run from the MAIN repo**, against the real backlog.
- **Debug verification** (does my code work) → the worktree's **dev build**
  against a **throwaway store** (e.g. `make verify`), inside the worktree.

Because the worktree has no repo `.amenbo` in its ancestry, Amenbo commands run
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
| deleted by | nothing — it is permanent | `devtool devgui rm <id>` |

The right-hand column is where it lands on **this machine**. The same instance goes
into the throwaway VM instead with `make install-gui-dev-vm AMB-T-ID=<id>` — same
names, same layout, another machine — and every command that addresses it answers
for that machine with `--vm`. See [`devgui install`](#devtool-devgui-install-id---vm)
and [the destination flag](#which-machine-a-command-is-about----vm).

Each build runs under an executable name of its own — production keeps
`amenbo-app` — so a name reaches one app and not another: `pgrep -x
amenbo-app-dev-<id>` finds that one instance, and `System Events` lists it under
the same name. A *click* aimed at a point still lands on whichever window is in
front; `click-named <pid> <name>` fronts the pid it was given before pressing,
so that one reaches the instance named and not whatever was there.
The badge is how you tell them apart *inside* the window: it sits in the header,
so it survives a cropped screenshot, and production carries none at all. To
reach one without a click, ask for its pid (`devgui pid` below) and drive that
pid — the badge tells you afterwards what you shot, the pid decides beforehand
what you shoot. Shooting it is `devgui shot`, which walks that same pid to the
window and captures only it.

So **verify a task in its own app**: with no hand reaching the shared bundle,
two parallel sessions cannot install over each other, and the collision is gone
by construction rather than by taking turns.

What the instance opens on is the shared dev store as it stood when the bundle
was built. Anything a screen needs *beyond* that is put there with
[`devgui cli`](#devtool-devgui-cli-id---no-build----amenbo-args) below, in the
instance's own store — never in the shared one, which no task may edit.

devtool seeds and reclaims the instance; the Makefile builds it. That split is
deliberate — a bundle costs minutes to build and ~38MB on disk, so only the
tasks that actually look at a GUI pay for one. Beyond that instance's app-data,
devtool provisions no Amenbo store of its own.

All of this is macOS-only, which is where the dev GUI is installed at all;
elsewhere the `devgui` commands are no-ops.

#### The record the instance leaves in Background Task Management

Installing the tick from an instance (or turning its login item on) registers it
with macOS under a label of its own — `work.amenbo.tick.amenbo-dev-<id>`,
parented to `work.amenbo.app.dev.<id>` — and **that registration is not
`devgui rm`'s to take back**. Deleting the bundle does not remove it, and
`SMAppService`'s unregister only flips it to `disabled` (both measured). macOS
exposes no per-item delete either: `sfltool` offers `resetbtm` alone, which
wipes every app on the machine.

So a `sfltool dumpbtm` read during verification can hold rows for numbers nobody
will type again. **Tell them apart by identifier**: the row under test is
`2.work.amenbo.app` / `8.work.amenbo.tick` with nothing after it, and anything
carrying `.dev.<id>` or `-dev-<id>` is an instance's. The `Name` cannot do that
job — it freezes at whatever was registered first and never updates.

They do not accumulate forever, though. Measured 2026-08-21: the rows an earlier
round of tick verification left (3354 / 3355 / 3358 / 3359) are gone, while
production's own two kept counting their generations up — so no `resetbtm` ran.
The machine had rebooted in between and nothing in the log names the removal, so
the reboot is what fits, not what is proven.

What does survive every reboot is launchd's disabled ledger
(`launchctl print-disabled gui/$(id -u)`), which still names instances torn down
long ago. It holds a label and nothing else, and `launchctl` writes entries
there without ever deleting one.

## Commands

### `devtool devgui seed <id>`

Clones the shared dev store into the app-data of task `<id>`'s own dev GUI
(`work.amenbo.amenbo-dev-<id>`), so the instance opens on the setup grown in the
shared app rather than an empty one.

`make install-gui-dev AMB-T-ID=<id>` runs this, so a task that builds its
instance the ordinary way never types it. Details:

- **An app-data already sitting there is left alone.** It is the session's own
  work, and a fresh clone would throw it away.
- **A number with no checkout under it is refused.** An instance belongs to a
  task being written somewhere; app-data under a number no worktree claims is
  precisely what the sweep goes looking for.
- **Everything past that reports and carries on.** No shared store to clone, or
  a clone that failed, leaves an instance that opens empty — a poorer screen to
  verify, never a reason to fail the build that asked.
- The bundle itself is not built here; that is the Makefile's, and only the
  tasks that look at a GUI pay for one.

### Which machine a command is about — `--vm`

Every command that addresses an instance reads **this machine** by default, and
answers for the guest with `--vm`:

```sh
devtool devgui pid   696 --vm --front     # the pid in there
devtool devgui shot  696 --vm             # shot in there, png brought back here
devtool devgui cli   696 --vm -- …        # writes the store in there
devtool devgui rm    696 --vm             # reclaims it in there
devtool devgui sweep --vm                 # what is in there, and what is orphaned
```

**The default does not move.** A clone or a fork with one Mac has no VM, so the
host route has to keep working unchanged; `--vm` is a second destination.

Only the machine asked changes. The guest layout mirrors this one, so the same
names, the same paths and the same pid lookup do the work — a listing becomes an
`ls` over ssh, a removal an `rm -rf`, and the screen tool driven is the copy
`devtool vm screen` put in there. Two of them cannot be typed in the guest at
all: the sweep needs `git worktree list`, which only this machine can answer, and
`cli` runs a build of this checkout.

### `devtool devgui install <id> --vm`

Puts the task's own dev GUI **in the throwaway VM** instead of on this machine —
the screen it is driven on, so a verification run does not take this Mac's
keyboard and mouse:

```sh
make install-gui-dev-vm AMB-T-ID=696     # build it here, put it in there
devtool devgui install 696 --vm          # put the built bundle in there again
# /Applications/amenbo (dev 696).app
```

**One build per id at a time.** The make route runs under a lock named for the id
(`~/Library/Caches/amenbo-devgui-<id>.lock`), and a second run of the same id stops
rather than queues behind the first. Two runs of one id share a worktree and so one
cargo `target`, where the second does not fail but waits ("Blocking waiting for file
lock on build directory") — so a build asked for twice looks alive from the outside
while neither side moves. Another id is another worktree and another `target`, and is
not held up by this. See `scripts/devgui-build-lock.sh`.

**The build stays on the host.** Only the placing moves, so the guest needs
neither Rust nor node, and the `.app` baked here runs in there unchanged — same
arch, same OS generation (43MB across in 0.96s, measured).

**This machine stays the default destination**, and that route is the Makefile's
own (`make install-gui-dev AMB-T-ID=<id>`): a clone or a fork with one Mac has no
VM, and a default that needed one would leave it unable to verify anything.
So `--vm` is not optional here — it is the whole reason the
command exists, and without it you are asking for the route that has one already.

- **It raises the VM if none is running.** When a clone is thrown away is a
  person's call (`devtool vm rm`); when one is raised is not.
- **The instance is quit in the guest first**, by executable name
  (`amenbo-app-dev-<id>`). An app replaced under itself writes its store back on
  the way out, over the bundle just sent. One that will not quit is a non-zero
  exit, not a half-replaced bundle.
- **What is sent is staged and moved into place**, never copied over what is
  there: `scp -r` onto an existing directory merges into it, and a bundle
  carrying files from an older build looks exactly like an implementation that
  does not work.
- **The store is this machine's shared dev store, sent across** — the same setup
  (plugins, catalog, projects) a host instance is seeded from. The guest has no
  shared dev app of its own and never will: it is a clone thrown away at the end
  of a session. A store already in there is left alone, and everything past that
  reports and carries on — an instance that opens empty is a poorer screen, not
  a reason to fail the placing that asked.
- **The instance gets a folder of its own, bound to a project in its store** —
  `/Users/admin/amenbo-work-<id>`, which is where `devgui cli --vm` runs. An
  `.amenbo` pointer names exactly one store, so a single one in the guest's home
  belongs to one instance and is refused for every other (`pointer_other_store`),
  and that refusal is what `--actor ai` hits: the facet draws its reach from the
  pointer of the folder it stands in. It goes beside the store rather than inside
  it, because a throwaway store is a whole-directory clone and a pointer left in
  there would ride into the next instance. Binding is a human's act and is done
  under that facet, `--force`d past whatever the home still holds; the project is
  the store's lowest-numbered one, and re-pointing it is one command:
  `devtool devgui cli <id> --vm -- --actor human bind --project <n> --force`. A
  store holding no project at all gets one raised in the folder instead (`init`,
  which binds as it goes) — the same move `make verify INIT=1` makes on its own
  throwaway store.
  **No CLI is built for this** — placing a bundle should not wait on a second
  toolchain run — so a checkout with no debug build yet leaves the folder cut and
  unbound, and the first `devgui cli --vm` binds it.
- **The guest layout mirrors this machine's exactly** (`/Applications` bundle,
  `~/Library/Application Support` store), so what addresses an instance by path
  reads the same on both sides and only the machine it is asked of changes. The
  bound folder is the one thing the guest holds that the host has no counterpart
  for: on this machine every instance's store is a directory of its own, so a
  pointer beside one is already one instance's and no other's.
- **Nothing lands on this machine.** The bundle is read out of the build
  directory, so the host `/Applications` and the host app-data are untouched —
  and `devgui rm` reclaims neither of the guest's halves. Throwing the VM away is
  what reclaims those, all at once.

Opening it is one line — `devtool vm exec -- open -a '/Applications/amenbo (dev
696).app'` — and the printed path is that argument. macOS only, and it needs
`tart` the way everything under `vm` does.

### `devtool devgui cli <id> [--no-build] [--vm] -- <amenbo args…>`

Runs an Amenbo command against **the store the task's own dev GUI reads**, so a
screen can be given something to show. A dev GUI shows what is in its store: a
rejected task, a card with a due date, a plugin in some state all have to be
*put there* before the screen that renders them can be looked at.

```sh
devtool devgui cli 696 -- --actor human --project myproj task add --title 'due today' --due today
devtool devgui cli 696 -- --actor human --project myproj task reject 5 --reason 'out of scope'
```

The CLI is the **worktree's own** `target/debug/amenbo` — rebuilt first, unless
`--no-build` — pointed at that store with `AMENBO_HOME`. Nothing is built per
task that was not being built anyway: the app-data name is fixed at build time
(`AMENBO_APP_NAME`), but what it selects is a *directory*, and `AMENBO_HOME`
names the same one at run time — the seam `make verify` already isolates
through. The other way in is to rebuild the CLI with
`AMENBO_APP_NAME=amenbo-dev-<id>`: two minutes, for a binary one task can use.

Details worth knowing:

- **Arguments go after `--`.** Without it a `--json` of Amenbo's would be read
  as a flag of devtool's.
- **The exit code is Amenbo's**, so a seeding step that failed fails visibly.
- **It runs in the store's own directory.** Relative paths resolve there, and a
  `bind` writes its `.amenbo` beside the store it points into — which `devgui
  rm` reclaims with the rest of the instance. In the worktree that same pointer
  would be a live one for *any* Amenbo run there, the production binary
  included, which is the reach the worktree is kept outside the repo to deny.
  (`--vm` runs in the instance's bound folder instead — see below.)
- **The binary still introduces itself by its own channel** (it was not built
  with `AMENBO_APP_NAME`), so what is keyed to the channel rather than the store
  — the command name in guidance text, the perf log's default — reads as
  production. It writes the right store; it says the wrong name doing it.
- **It will migrate that store if the tree is ahead of it.** An isolated store
  is an arm of the migration gate, and that is the wanted answer here: the
  task's own GUI is built from the same tree and would carry it forward the
  moment it opened.

`--vm` writes the store of the instance in the guest instead. The same build is
sent across — the guest holds no toolchain, and the two machines are the same
arch — and run in there, pointed at that store the same way. It is sent on every
run, because the reason the CLI is rebuilt first is that the tree it seeds a
store for keeps moving.

Where it runs is the one place the two routes part: **in the guest it runs in the
instance's bound folder** (`/Users/admin/amenbo-work-<id>`), not in the store.
That is what makes `--actor ai` usable in there at all — a facet reaches only the
project the pointer of its folder names — so the examples above are the same ones
with `ai` in them:

```sh
devtool devgui cli 696 --vm -- --actor ai task add --title 'due today' --due today
```

The folder is cut and bound by `devgui install --vm`, and by this command when it
finds one missing (an instance can be seeded before a bundle is ever put in
there). It is bound to the store's lowest-numbered project; re-point it with
`--actor human bind --project <n> --force`, which lands there like any other
command run through here.

macOS only, like everything else about the per-task instance.

### `devtool devgui pid [<id>] [--front] [--vm]`

Prints on **stdout** the pid of a running dev GUI, so it can be handed straight
to the tools that take one:

```sh
# every named element on the screen of the dev GUI this checkout launches, fronted
swift scripts/screen.swift find "$(devtool devgui pid --front)"
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
- `--front` brings it forward first. A window behind another Space cannot be
  found at all, and a shot of a window nobody fronted is a shot of whatever is
  over it.
- Nothing running is a **non-zero exit** with the build command named, not an
  empty answer that reads as a pid of zero.
- `--vm` answers for the instance in the guest, and the pid is a pid **in
  there** — what takes it is the screen tool in the guest, or `devtool vm exec`.
  There is no shared dev app to fall back on in the VM, so with no `<id>` it is
  the checkout's own task or nothing.

### `devtool devgui shot [<id>] [--no-front] [--vm]`

Captures the instance's **own window** and prints its png's path on stdout:

```sh
devtool devgui shot 696
# /var/folders/…/amenbo-devgui-696-2751829313.png
```

It resolves the pid and hands it to the screen tool
(`swift scripts/screen.swift shot <pid> <out.png>`), which finds the window and
shoots it. Finding the instance is devtool's half; operating a screen is the
tool's.

- **The id it shot by stays in the tool.** Nothing here can aim a click by a
  rectangle, which is the point: name the thing to press
  (`swift scripts/screen.swift click-named <pid> <name>`) and neither the shot's
  scale — an external panel shoots at 1 where the built-in one shoots at 2 — nor
  a screen that has moved since the shot is yours to get right.
- **An instance drawing two windows has to be told which one** — `--window
  <title>`, passed straight through to the tool, which matches the title drawn in
  the bar (whole first, then by part). Left out, the tool refuses and lists the
  titles that are up rather than shooting whichever was in front: a shot of the
  wrong window is a picture of a screen nobody asked for, and it looks exactly
  like a picture of the right one. An instance drawing one window says nothing
  and is answered, as it always was.
- **It fronts the instance first**, the opposite default from `pid`: a window
  behind another Space is not on-screen at all, so it cannot even be found.
  `--no-front` is for capturing a state that fronting would disturb.
- Screen recording has to be granted to the terminal running this, or nothing is
  written — which comes back as a non-zero exit saying so, not as an empty png.
- `--vm` shoots the window in the guest with the screen tool in there and brings
  the png **back out**, so the path printed is one on this machine — the thing
  that looks at a shot is here. Nothing has to be granted on this side for it:
  the recording is the guest's.

### `devtool devgui rm <id> [--vm]`

Deletes one task's instance: the `/Applications` bundle **and** its app-data,
naming each path it removed. Both live outside the checkout, so removing the
worktree leaves them where they are — skipping this costs ~38MB per task,
forever, and `devgui sweep` is the way back for a task that skipped it.

A **running** instance is asked to quit first, and if it will not, both halves
are left where they are. Deleting the store under a running instance does not
remove it — the instance writes its store back on the way out, and the removal
that reported it reclaimed reads as green while the leftover returns minutes
later.

`--vm` takes the instance in the guest, the same two halves the same way, plus
the bound folder that only the guest has — a pointer left behind would name a
store that has just been deleted. Throwing the VM away (`devtool vm rm`) takes
every instance in there at once, so this is for reclaiming one while the clone
goes on being used.

### `devtool devgui sweep [--yes] [--vm]`

Lists every per-task dev GUI on this machine and says which ones no worktree
claims any more. An instance outlives the checkout it belongs to, so a session
that died — or one that ended without `devgui rm` — leaves ~38MB of bundle plus
a store behind under a number nobody will type again.

- **An instance a worktree still owns is never touched, and never offered.** That
  is the same hands-off line a pre-existing worktree draws elsewhere: the
  worktree is the evidence a session owns that number, and whether the session is
  "really" still working is not this command's to judge.
- If `git worktree list` cannot be read, it **refuses** rather than guess — an
  unreadable answer would read as "nothing is claimed", i.e. delete everything.
- Reporting is the default; `--yes` reclaims. What goes is a store, and the
  report is the review.
- A running instance is asked to quit and skipped if it will not (see
  `devgui rm` above).

Only the digits form an instance: a hand-made `amenbo (dev wip).app` is
somebody's own, and the shared `amenbo (dev)` app is permanent.

`--vm` sweeps the guest instead — an orphan's bound folder goes with its halves,
though a folder alone is never what makes an instance show up in the listing —
and **it can only be asked from here**: what makes an instance live is a
checkout, and the checkouts are on this machine.
Asked inside the guest, git has nothing to answer with — and a sweep that cannot
tell live from orphan refuses rather than guess, so it would simply never run.

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
so nothing here holds a second copy of Amenbo's schema, and what is added is what
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
nothing in the CLI reads an entry's own claim back. It is what the first reading
of `verification/scenarios/plugin-from-a-catalog.yaml` is looked at for.

**Each of them declares an event and a setting**, in its own detail document —
the second document a catalog is served as, fetched when a row is opened. The
panel above it is drawn from the entry the list already had, and the enable line
under it is a phrase of the interface, so those declarations are the only thing
on an opened plugin that says which catalog's document was fetched. That is what
the reading after it, on that same road, is looked at for.

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

### `devtool plugin round --manifest <path.json> [--program <path>] [--set k=v] [--events <list>] [--keep]`

One plugin, one lap, in a store that is thrown away afterwards:

```sh
# what a plugin's own subscriptions receive, without writing a plugin to look
devtool plugin round --manifest ../amenbo-plugin-slack/dev/manifest.json \
  --set webhook_url=http://127.0.0.1:9/hook

# the build itself, on the events it cares about, with the store left to poke at
devtool plugin round --manifest dev/manifest.json --program ./slack \
  --events comment,deleted --keep
```

It raises a throwaway base (`AMENBO_HOME`, removed unless `--keep`), lays the
plugin down **by hand** the way a plugin repo's own `make install` does — a
directory under `plugins/` holding `manifest.json` and the executable under the
plugin's own name — fills in what the manifest declares, opens the gate, fires
the events an AI's writes fire, empties the queues, and then shows what the
plugin was handed and how each run ended (`amenbo plugin log`).

**The manifest is the JSON form**, the file a plugin repo already keeps for its
own hand-install. A `.yaml` one is refused rather than converted: what an install
lays down is JSON, and a converter here would be a second reading of a contract
Amenbo owns.

**Without `--program` it installs devtool's stand-in** — a script that records
each document it is handed and answers nothing — so "what does a subscriber
actually receive" is answerable without writing a throwaway plugin for it. The
subscription is still the manifest's, so what comes out is what *that* plugin
would have been sent.

**A required setting nobody named is filled** (from the field's own default or
candidates, since a field with candidates refuses anything else), because the
gate refuses to open while one is empty. An optional one is left empty: that is a
state the plugin is meant to run in.

**Nothing is asserted.** The receiving side — a webhook to stand in for, a
checkout to look at afterwards — is the plugin author's, who is the only one who
knows what "it worked" means. The queues are emptied with `amenbo plugin flush`,
asked again while any queue is still held by a runner a write started, and a
window that closes with something still waiting is reported as that.

### `devtool vm up | rm | status`

Raises the throwaway macOS VM the GUI is verified in, and throws it away.

```sh
devtool vm up          # prints the address on stdout
# 192.168.64.4
```

Driving a screen means posting `CGEvent`s to `cghidEventTap`, which takes the
keyboard and the mouse of whatever Mac it runs on for as long as it runs. The way
out is a second screen, and a guest of the same arch and OS generation is one the
existing tools work inside unchanged.

- **The golden image is never started.** `up` cuts a clone from it (0.03s, no
  disk of its own until it is written to) and starts that. A golden that has been
  booted has picked up state and stopped being a known ground.
- **A clone already running is used as it stands.** A test does not raise a second
  VM, and it does not throw away the one a session has been working in.
- **`rm` is the only thing that throws it away**, and nothing here decides on its
  own that a session is over.
- The VM is started `--no-graphics`: no window on the host, and a virtual display
  in the guest all the same. `system_profiler SPDisplaysDataType` answers empty in
  there and the display is nonetheless real — do not read that answer as "no
  screen".
- **The VM is started `--no-clipboard` too**, so the guest keeps a clipboard of
  its own. Shared, the host's is pushed in whenever it changes, and a ⌘V in the
  guest puts down whatever was copied at the host rather than what the run copied
  a moment ago — a path copied in the file panel came out of the paste as another
  session's text, having read back correctly from `pbpaste` just before. Nothing
  here carries anything in or out by clipboard, so there is nothing on the other
  side of the switch.
- **`up` waits for a GUI session, not for a ping.** `/dev/console` owned by the
  account is what says there is a screen to draw on; without one the screen tools
  fail in the shape that is hardest to read — exit 0, nothing delivered. Measured:
  address at ~7s, ssh at ~10s, console at ~11s.
- **`up` also settles what that screen is** — see below. A screen nobody set is
  not a screen anything can be asserted against twice.
- `tart run` is detached into its own process group, so a Ctrl-C on devtool does
  not take the VM with it. Its output goes to
  `$TMPDIR/amenbo-vm-amenbo-vm.log`, which is where a VM that failed to boot says
  so.
- `status` reports the golden, the clone, its address and the mode its screen is on.

#### The screen the guest comes up on

Two things have to be said for the guest to have a screen worth shooting, and
`up` says both:

1. **How big the panel is.** `up` sets it on the clone before starting it
   (`tart set --display 1920x1200`), rather than inheriting whatever the golden
   carried — a shot is only comparable against a shot taken on the same screen,
   and `vm golden --refresh` would otherwise move it. The size is read as
   **points**, so the panel behind it is 3840x2400 pixels. Asking in pixels
   instead (`1920x1200px`) builds a 1x panel, and text read off a 1x shot comes
   back wrong often enough to fail asserts that are sound (`Inbox ame`,
   `♥ Installed`).
2. **Which mode the desktop takes on it.** macOS does not take the panel's own:
   measured on a clone freshly cut from the golden with the panel set to
   1920x1200pt, the desktop comes up **1024x768pt stretched across it** — too
   narrow for the layouts that only appear on a wide window (the horizontal
   rail), and stretched under every shot. The mode is in the guest's own list
   the whole time; it has to be asked for, which `up` does for the session
   (nothing is written into the clone's preferences — the clone is thrown away,
   and `up` asks again every time).

The asking is a second small Swift tool, carried inside devtool and compiled and
sent the way the screen tool is. It is not a verb on `scripts/screen.swift`
because that one runs on a developer's own Mac as well, and something that
reconfigures a display does not belong next to click and type there.

### `devtool vm exec -- <command…>` / `devtool vm push <local…> <remote>`

Reach into the running clone. `exec` runs a command in there with this process's
own stdio and **ends the way it ended**, so a step that failed in the guest does
not read as green out here; `push` sends files, recursively, since what is
usually sent is a `.app`.

```sh
devtool vm exec -- 'stat -f %Su /dev/console'
devtool vm push "/Applications/amenbo (dev 3578).app" /Users/admin/
```

Arguments to `exec` go after `--`, and quoting is the caller's the same way it is
with `ssh` — what follows is joined and handed to the guest's shell.

The host key is deliberately **neither checked nor remembered**: a clone is cut
fresh from the golden and carries a new one each time, so a pinned entry would
refuse the next clone rather than catch anything. What is reached is a VM on this
machine's own private network, raised from an image on this machine's own disk.

### `devtool vm screen`

Compiles this checkout's `scripts/screen.swift` on the host, puts the binary in
the guest at `/Users/admin/screen`, and prints that path on stdout.

Compiled and sent rather than baked into the golden: the golden then holds no copy
of a tool this tree keeps changing (12s to build cold, 0.09s to send — measured),
the guest needs no Swift toolchain, and the golden can be replaced without
anything having to be re-baked into it.

**`click`/`click-named` in there still want a `front` first.** The tool does not
call it, and a VM's bare desktop has a Terminal on it that a click is otherwise
taken by — exit 0, nothing delivered.

**`drop-file` carries a file that is in the guest.** A drop reads the disk the
screen is on, so what is dragged in is a path in there — `devtool vm push` is how
one gets there. The drag takes the front for as long as it lasts and gives it back
after, so a road can read the window it was dropped on straight away.

**A shortcut is one press: `key <keycode> --cmd`.** ⌘C is `key 8 --cmd` and ⌘V is
`key 9 --cmd`. The modifier rides on the event's flags and is never held as a key
of its own, so nothing is left pressed if the run stops between the two.
`--shift` / `--opt` / `--ctrl` are there the same way, and a subcommand other than
`key` refuses them rather than ignoring them.

**`find`/`click-named` read one window, not the app.** They take the same
`--window <title>`, and with two windows up they refuse without it — a name
reached on the wrong window is a check that passed without looking at the screen
it was written for.

**A name on two kinds of element is said apart with `--role <role>`.** The role is
the first column `find` prints. The task pane's assignee is an `AXPopUpButton`
called by the person it holds — `Assignee` is the static text beside it, and a
press on that exits 0 having done nothing — while the filter panel's checkbox
carries the same `Unassigned`. Without the role the tool refuses rather than
pressing one of them, and a point is what that costs.

**A date field is written with `set-date`, never clicked and typed into.** The
click that reaches one opens a picker, and the picker takes every key sent after
it — `type "12312099"` and a raw keycode arrive nowhere alike — while the year
moves one step per press of the up arrow, which is seventy presses to reach 2099.
`set-date <pid> <name> <yyyy-mm-dd>` puts the day in with one call and reads it
back. `--near <name>` says which row is meant, for a screen where one name reaches
several fields: the dimension manager's value rows each carry a `Start date` and
an `End date`, where the task pane carries one `Due date` and one `Start date`.

**A day nobody has named yet has no field to write into.** An empty date input
draws today as its placeholder, so both screens say the absence in words instead
and put the field up only once somebody asks for one — `None` with an `Add` beside
it on the task pane, `No start date` / `Ongoing` on a dimension's value. Press that
first, or `set-date` answers `no date field on screen is called <name>`.

### `devtool vm golden [--refresh]`

Reports on the image clones are cut from — is the base pulled, is the golden
there, is the key where it is looked for — and with `--refresh` takes the base
again (`tart pull`) and cuts the golden from it anew.

The base is a third party's (`ghcr.io/cirruslabs/macos-tahoe-base`): SIP disabled,
TCC granted to `/usr/libexec/sshd-keygen-wrapper`, Gatekeeper off — none of which
we set, and all of which the screen tools need. Its contents are not inspected.
Only the way the golden is made would change to move off it.

- **The clone is checked before the pull.** The pull is the expensive half, and
  refusing afterwards would have spent it for nothing.
- **Enrolling the key is left to a person, and named rather than done.** It takes
  the image's password, which is a credential to type. `--refresh` prints the
  `ssh-copy-id` line to run and says to stop the golden again afterwards.

### `devtool vm verify seed | install | run | step | log | pull`

Walks a **pre-distribution screen road** (`verification/scenarios/`) inside that VM.

```sh
devtool vm verify seed ~/dist/22.2.0/amenbo-darwin-arm64.pkg # optional: the version already there
devtool vm verify install ~/dist/amenbo-darwin-arm64.pkg     # or --from-run <run id>
devtool vm verify run verification/scenarios/link-a-folder.yaml
# … drive the screen in the guest, then:
devtool vm verify step --note 'pressed Link a folder'
devtool vm verify pull --out ./evidence
```

**The harness is not changed, and nothing here repeats what it does.** `verify-gui` still launches
the shipped bundle, holds the pid that launch answered with, stands up the world the scenario
declares and shoots one screen per step. That is the reason the harness *moves into* the guest
rather than being driven from outside: a pid held on this side would name a process on that one.

What is added is the four things a run in there needs and a run here does not.

**`install`** sends and installs the shipped build, the harness, the scenarios, the fixtures and the
screen tool.

- **The build is a path, or `--from-run <run id>`** — the mac artifact of a CI run, never the
  release's download URL: a release download is counted, and a development one cannot be subtracted
  afterwards. Holding the bytes against what the release published is the release procedure's own
  step, upstream of this, which is what handing this command a path keeps room for.
- **A build for the other architecture is refused by name.** It installs cleanly and then will not
  start, which is a failure several steps away from its cause.
- **The install is run in the session that owns the guest's screen** (`launchctl asuser`), as the
  account rather than as root, and watched rather than waited on. That is what lets the postinstall's
  one-time migration ask its admin password where somebody can see it — and what answers it is this
  side, typing the image's password into the field the dialog opens focused and pressing Return.
  Measured 2026-09-05: 22.2.0 seeded system-wide, 22.3.0 installed over it, `/Applications/Amenbo.app`
  and `/usr/local/bin/amenbo` both gone afterwards. A machine with no old copy is asked nothing, and
  the install goes through untouched.
- **Nothing is built for the guest.** Host and guest are the same architecture, so the harness
  compiled here runs there, and the guest needs neither Rust nor node.
- The first `swift <source>` on a machine builds a module cache and takes some twenty seconds; every
  one after it is under a second. That is paid here, rather than inside the harness's own window for
  the app to draw a window — which that first call would otherwise run out.

**`seed`** puts a build in there and stops — no harness, no scenarios, no screen tool. It is what the
next `install` goes on over.

- **A clone is cut from a bare macOS, so an install into one is always a first install.** The road
  most people actually walk — a version already there, being replaced under a running app — was the
  one the VM could not reach. Two commands walk it: this one with the version they are on, then
  `install` with the one being shipped. `install` says what it went on over.
- **Whatever is installed is taken down first.** An installer skips a payload older than the bundle
  it finds — the component is version-checked — so a seed that only ran the installer would leave the
  newer build standing and answer with its version (measured in the guest: 22.2.0 seeded over 22.3.0
  answered 22.3.0). The store is not touched: a machine on that version has one, and it is what a
  migration is rehearsed against.
- **`--system-wide` leaves it where a release from before the per-user move did**: the bundle in
  `/Applications` owned by root, the CLI symlinked into it from `/usr/local/bin`, and no per-user
  copy or PATH line anywhere. That is the install whose postinstall asks for an admin password once,
  and the only elevation in a per-user lifetime.
- **The old build itself is not the seed — the shape is.** A build from that era is not obtainable
  any more, and it is not what the next install reads: the postinstall keys on those two paths
  (`scripts/build-pkg-mac.sh`) and on nothing else about what stood there.
- **The password is answered by `install`, on the guest's own screen.** What the seed gives is the
  machine the question is asked on; the asking is the next command's, and both halves of it are easy
  to lose. An install driven over ssh has no session to draw the dialog in — `osascript … with
  administrator privileges` comes back `-60007`, and the block being best-effort the install goes on
  without it — and an install run as root is never asked for a privilege it already holds. Either way
  the old copy and its link are still there afterwards, with the link still shadowing the new CLI on
  the stock PATH, and the run reads exactly like a migration that worked.

**`run`** starts one road and comes back when the harness has handed over its first step. It does
not wait for the run: a road is walked by somebody, and that somebody is whoever calls `step`
between one hand-over and the next.

- **`--screen` and `--fixtures` are passed explicitly.** The harness resolves both relative to its
  own executable, which in the guest is a path on this side of the machine. Without the first a run
  fails a minute in, having launched an app and photographed nothing; without the second a road that
  copies a fixture fails before that, standing up its world.
- A previous run's app is taken down first. The harness takes its own down when it ends, and the one
  case it cannot is the one that matters: a run somebody stopped part-way leaves a window that the
  next run's shots would have in front of them.

**`step`** sends one line and waits for the harness to say something next. **The steps come from a
file that is appended to, not from a pipe somebody holds** — the harness's stdin is `tail -n 0 -f`
over that file, so nothing has to stay alive between two commands, what was sent stays on disk to be
counted, and a run that has ended takes the tail down with it. `log` re-reads the same tail without
advancing.

**`pull`** brings the shots and the manifest out. They are what a `Review` step is closed from and
what a red one is read by, and they are of no use inside a machine that is thrown away.

The road itself is still walked by whoever is driving. In the guest that is the screen tool:

```sh
devtool vm exec -- 'PID=$(pgrep -f "Amenbo.app/Contents/MacOS/amenbo-app" | head -1);
  swift /Users/admin/screen.swift find $PID'
devtool vm exec -- '… swift /Users/admin/screen.swift click-named $PID "Link a folder"'
devtool vm exec -- '… swift /Users/admin/screen.swift set-date $PID "Due date" 2099-12-31'
```

### Host and guest drifting apart

Every command that reaches the clone compares `sw_vers -productVersion` on both
sides and says when they have drifted, on major and minor. **Nothing is stopped
over it**: what the guest is for is standing in for this machine,
and a guest several releases away stops standing in for it — but what it costs to
be wrong about that is a rebuilt golden.

The patch is left out on purpose. A guest one security update behind is the
ordinary state of an image republished weekly, and reporting it every single run
is how a warning stops being read.

## Env

- `AMENBO_HOME` — not read, **set**: `devgui cli` puts the task's own store there
  for the CLI it runs. It is Amenbo's own isolation seam, the same one
  `make verify` points at a mktemp store.
