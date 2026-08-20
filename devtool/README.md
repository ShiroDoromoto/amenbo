# devtool

Amenbo's portable developer-support CLI — a single static Go binary (no runtime,
no venv) you can drop into any project regardless of its language.

On macOS it gives a task its own throwaway **dev GUI** — bundle, app-data and
all — so several implementation sessions can run in parallel without installing
over each other, and it stands up a **fake outside world** the dev GUI can be
verified against — including the failures the real one will not produce on
demand.

## Build

It is **optional**, and so is its toolchain: Amenbo builds, tests and ships
without Go, and nothing outside this directory depends on it. Build it only if
you want it.

```sh
make devtool        # builds to ~/.cargo/bin/devtool
# or: cd devtool && go build -o ~/.cargo/bin/devtool .
```

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

Each build runs under an executable name of its own — production keeps
`amenbo-app` — so a name reaches one app and not another: `pgrep -x
amenbo-app-dev-<id>` finds that one instance, and `System Events` lists it under
the same name. A *click*, though, still lands on whichever window is in front.
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

### `devtool devgui cli <id> [--no-build] -- <amenbo args…>`

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
- **The binary still introduces itself by its own channel** (it was not built
  with `AMENBO_APP_NAME`), so what is keyed to the channel rather than the store
  — the command name in guidance text, the perf log's default — reads as
  production. It writes the right store; it says the wrong name doing it.
- **It will migrate that store if the tree is ahead of it.** An isolated store
  is an arm of the migration gate, and that is the wanted answer here: the
  task's own GUI is built from the same tree and would carry it forward the
  moment it opened.

macOS only, like everything else about the per-task instance.

### `devtool devgui pid [<id>] [--front]`

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

### `devtool devgui shot [<id>] [--no-front]`

Captures the instance's **own window** and prints its png's path on stdout:

```sh
devtool devgui shot 696
# /var/folders/…/amenbo-devgui-696-2751829313.png
```

It resolves the pid and hands it to the screen tool
(`swift scripts/screen.swift shot <pid> <out.png>`), which finds the window and
shoots it. Finding the instance is devtool's half; operating a screen is the
tool's.

- **Which window it shot, and the id it shot by, stay in the tool.** Nothing here
  can aim a click by a rectangle, which is the point: name the thing to press
  (`swift scripts/screen.swift click-named <pid> <name>`) and neither the shot's
  scale — an external panel shoots at 1 where the built-in one shoots at 2 — nor
  a screen that has moved since the shot is yours to get right.
- **It fronts the instance first**, the opposite default from `pid`: a window
  behind another Space is not on-screen at all, so it cannot even be found.
  `--no-front` is for capturing a state that fronting would disturb.
- Screen recording has to be granted to the terminal running this, or nothing is
  written — which comes back as a non-zero exit saying so, not as an empty png.

### `devtool devgui rm <id>`

Deletes one task's instance: the `/Applications` bundle **and** its app-data,
naming each path it removed. Both live outside the checkout, so removing the
worktree leaves them where they are — skipping this costs ~38MB per task,
forever, and `devgui sweep` is the way back for a task that skipped it.

A **running** instance is asked to quit first, and if it will not, both halves
are left where they are. Deleting the store under a running instance does not
remove it — the instance writes its store back on the way out, and the removal
that reported it reclaimed reads as green while the leftover returns minutes
later.

### `devtool devgui sweep [--yes]`

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

## Env

- `AMENBO_HOME` — not read, **set**: `devgui cli` puts the task's own store there
  for the CLI it runs. It is Amenbo's own isolation seam, the same one
  `make verify` points at a mktemp store.
