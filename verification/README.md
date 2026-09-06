# Pre-distribution verification

This subsystem verifies the **shipped / installed** Amenbo build as a black box, before a
release goes out. It is deliberately separate from `make test`, which exercises the
build-time workspace artifacts.

The single source of truth is the set of **scenarios**: declarative YAML naming what is being
proved and the road each driver takes to prove it, with no command line or coordinates baked in.
The goal is shared; the steps belong to the driver that walks them (`steps_cli` / `steps_gui`).

**What the drivers drive is the shipped bytes, and nothing else.** Each asks before it starts —
the CLI driver asks the binary `--bin` names, the GUI harness asks the CLI the bundle it was
pointed at ships — and the answer is the release workflow's stamp, which a build reports as
`release_build` in what `amenbo version --json` says. There is no flag to wave a local build
through: evidence gathered from a shipped build and evidence gathered from somebody's working tree
read alike afterwards, and a promotion resting on the second rests on nothing. Reading a road back
is not driving one, so `lint`, `emit` and `verify-gui --print` sit outside that line — they touch no
binary at all. To see a change you are writing on screen, build and drive its own dev GUI with the
development tooling; that is what it is for, and it is not this harness's road.

```
verification/
  scenarios/   the single source of truth (YAML). Every driver walks its own road through these.
  fixtures/    what a scenario cannot hold itself (a file carrying an amenbo ref for the lint, an image a picker is pointed at)
  core/        the scenario schema + validating loader (crate `amenbo-scenario`, `lint` + `emit` bins)
  cli/         CLI driver + runner — drive the shipped binary, assert via --json (crate `amenbo-verify-cli`)
  gui/         mac harness — scenario → screen checklist, shot and read by the screen tool (crate `amenbo-verify-gui`).
               It reaches into `cli/` for the two things a screen run needs of the binary: the
               throwaway store, and the vocabulary a `given:` world is stood up in.
```

`core/`, `cli/` and `gui/` are members of this cargo workspace, outside the main workspace, so the
root manifest's own clippy and tests never reach them. What does is a stage of its own on either
side: CI's `verification` job, gated to changes under `verification/` (`.github/workflows/_ci.yml`),
and `make gate-verification` locally, which `make gate` runs when this layer is what a change
touched and `make test` runs unconditionally. Both are the same two lines:

```sh
cargo clippy --manifest-path verification/Cargo.toml --all-targets -- -D warnings -A clippy::disallowed_methods
cargo test --manifest-path verification/Cargo.toml
```

`-A clippy::disallowed_methods` is not slack, it is the one rule that cannot apply here: this
workspace black-box-drives the shipped binary, so it reads process env raw (`AMENBO_BIN`) rather
than through `amenbo_core::env`, which it has no dependency on. A plain `cargo clippy
--all-targets` in this directory fails on those lines and on nothing else.

## One scenario per path a reader walks

A scenario is **one goal and the steps that reach it** — the path a reader actually walks, not a
feature out of a list. What this gate defends is the release, and what a release breaks is a path.

- **The file is named after the path**, and the scenario's `id` is the file's stem. Nothing pins the
  set's size or its names to Amenbo's own capability list: that list stays the feature inventory
  `amenbo agent --json` prints, and it is not the denominator of anything here.
- **A step belongs to the path it is on**, whichever commands it takes on the way. A path crosses
  several capabilities by definition — that is what makes it a path — so the question a step answers
  is "does this get the reader to the goal", never "which command owns this line".
- **A goal both the CLI and the screen can reach is one file**, carrying both roads. A goal only one
  of them can reach — what only the screen ever says, what has no screen at all — gets that driver's
  file alone, and is not a gap in the other.

The set is kept honest by the hand that changes the product, not by a count: add, change or drop a
path a reader walks, and its scenario is written, fixed or dropped in the same session.

One rule sits above all of that: **a line that needs an op the registry does not have is not written
here at all.** Growing the registry means growing every driver's mapping with it, which is its own
implementation in its own workspace; a YAML that runs ahead of it turns `verify-all` red and holds
up a release. File a task for the op instead, and leave the line out until it lands.

## CLI driver

`verify-cli` reads one scenario, maps each step to an invocation of the **shipped / installed**
`amenbo` binary, and judges the asserts from that binary's `--json` output — a black box, so it
knows the domain vocabulary, not the build under test.

```sh
cd verification
# drive a specific binary (e.g. the CLI extracted from a release .pkg):
cargo run -p amenbo-verify-cli --bin verify-cli -- scenarios/delegate-to-ai.yaml --bin /path/to/amenbo
# or the `amenbo` on PATH (the installed CLI), with a machine-readable result:
cargo run -p amenbo-verify-cli --bin verify-cli -- scenarios/delegate-to-ai.yaml --json
```

`--bin` (and `$AMENBO_BIN`) takes a relative path as well, read from where you run the command and
not from the throwaway directory the scenario is driven in — so a path to an artifact you unpacked
beside the repository means what you see. A value with no separator in it (`--bin amenbo`) stays a
`PATH` lookup. A binary the release workflow did not produce is refused by name, before the first
step of the first scenario.

The run is isolated by `AMENBO_HOME` pointed at a throwaway store plus a `.amenbo`-free CWD;
the real app-data is never touched, and `AMENBO_UPDATE_CHECK=0` keeps it off the
network. Exit code is the machine signal — `0`
when every assert passes, non-zero on any failed assert or execution error — so the runner reads it
directly. `--keep` leaves the throwaway store in place for inspection.

**One thing does leave the box.** `plugin install` resolves the official catalog over the network,
picks this platform's asset and verifies its signature against the key built into the binary — a
layer that exists only in a shipped build, and one no local fixture can stand in for without the
run reading green over the very thing it exists to catch. The plugin scenarios therefore need
the network and an intact catalog — the ones that install one, and the one that reads the browsing
view back — and they are the only ones that do.

The loopback is the other side of that. A third-party catalog is trusted on the signing key it
publishes beside its `catalog.json`, and a key is *served*, never written down — so a scenario that
walks the pin (`plugin catalog-stand`) has the run publish a catalog of its own on a port, and names
it by the `as:` binding rather than by a URL it could not have known. `catalog-rotate-key` serves the
other of two keys from the same address, which is a publisher rotating theirs as Amenbo sees it. The
host is `amenbo-static-host`, shared with the main workspace's own tests.

What that catalog puts on its shelf is the scenario's to write, under `offers:` — the one arg written
as a list of rows rather than as a word. Each row is the words a catalog's own documents carry: the
`name` it is fetched and badged by, the `desc` drawn under it, the `claims_official` badge it is not
entitled to (a shelf anyone may publish into holds no review, so seeing the merge clear that claim
needs one to have been made), and the one `setting` its author declares under the `label` a form
shows. Naming no rows is an empty shelf, which is what a road about the trust root alone wants. A
registration takes a `name:` for the same reason the rows are named at all: a catalog registered
without one is called after the host of its URL, and a loopback address with a port picked this run
is nothing a road can read a row's provenance back by. Nothing on such a shelf installs — the rows
carry no asset, and an install is walked against the real catalog, whose signature is the layer no
fixture can stand in for.

A row may also carry `about:` — what its author wrote about the plugin at length, which is the body an
opened panel is read by — and `translated:`, the same `desc`, `about` and `label` as its author wrote
them in other languages, keyed by language code. What the shelf then publishes is the shape a real
catalog publishes, which is two shapes rather than one. The lines go beside the list, one
`catalog.<lang>.json` per language, so a reader fetches their own and nobody pays for the other
eighteen; the description text and the labels go inside each row's own detail document, every language
at once, so a panel and a form already fetched follow a language change with no request behind it. That
split is the reason to write a row this way rather than to stand a shelf per language: what a screen
road has to be able to see is a listing redrawn from a document fetched for the new language, and a
panel redrawn from one nobody fetched again. A language no row drew a *line* in gets no document, and
the 404 the fetch meets is the answer a reader of an untranslated language already gets — so the
fallback is walked by leaving a row, or a whole shelf, untranslated rather than by breaking anything.
A row that carries no `about:` at all is the other fallback, one layer up: its panel is drawn from the
README of the repository every stood row names, which is where a plugin's description came from before
authors had anywhere else to write one.

Each op the driver maps is an arm in its domain's module under `cli/src/domain/` — `task depend` in
`task.rs`, `plugin install` in `plugin.rs` — and an op that is in the scenario registry but not yet
mapped fails loudly rather than passing silently. `cli/src/lib.rs` keeps only what every arm stands
on (the isolated session, the one invocation, the bindings, the report) and hands each step to the
domain it names, so "how is this op driven?" is answered by opening one file.

## Runner

`verify-all` drives a whole set of scenarios through that same driver, one after another, and
rolls their verdicts into one: green only when every scenario is green. Each scenario runs in its
own throwaway store, and a scenario that fails to load or whose binary errors is recorded as a red
entry — the run carries on, so one broken scenario never hides the rest.

```sh
cd verification
# every scenario under scenarios/ (the default), against a specific binary:
cargo run -p amenbo-verify-cli --bin verify-all -- --bin /path/to/amenbo
# a chosen subset (files and/or directories), with a machine-readable aggregate:
cargo run -p amenbo-verify-cli --bin verify-all -- scenarios/shape-a-task.yaml scenarios/delegate-to-ai.yaml --json
```

A scenario with no `steps_cli` road is **skipped**: printed as skipped, counted apart, and left
out of the verdict. If that leaves nothing to run, the exit is non-zero rather than
an empty green — a gate that verified nothing must not read as one that verified everything.

The exit code is the roll-up — `0` when every scenario that ran is green, non-zero when any is red
or errored — so a release gate reads it directly. The `--json` aggregate carries `total` / `passed`
/ `failed` / `skipped` / `green` plus each scenario's own report (or its error, or the drivers
whose roads it carries).

## GUI harness (mac)

`verify-gui` walks a scenario's `steps_gui` road as a **screen checklist**, and only a scenario
that has one — it refuses the rest by name instead of shooting a road written for the binary. It
bakes in no command line and no pixel: each step becomes a plain-language instruction of what to do
or confirm on screen, and the shooting is the screen tool's (`scripts/screen.swift`) — the harness
names the app by pid and receives one file per step in an evidence directory (plus a
`manifest.json` pairing each instruction, verdict and shot). The id it was shot by stays inside the
tool: a format nobody is handed is a format nobody parses.

**A step says which window it happens in, once the app draws more than one.** `window: <title>` on a
`steps_gui` step names the window by the title drawn in its bar — a whole title first, then any that
holds it, the same way a name reaches an element — and it reaches the sentence the operator is
handed, the shot the tool takes and the line the manifest keeps. Said nowhere, a step means the app's
one window; against an app with two the tool refuses and lists the titles rather than shooting
whichever was in front, because a shot of the wrong window is a picture of a screen nobody stood at
and it reads on the manifest exactly like a picture of the right one. It is the screen road's alone:
on `given` it names a window nothing has drawn yet, and on `steps_cli` one that never exists, and the
loader turns both away.

**The run owns the app it shoots.** It launches the `.app` bundle named by `--app`, with
`AMENBO_HOME` pointed at a throwaway store of its own, holds the pid that launch answered with, and
takes both down when it ends. Two things follow, and both are the point of doing it this way.
Nothing separates a shipped build started for a run from the same shipped build the user keeps
open — one executable name, one bundle, no badge on screen, and nothing stopping both from running
at once — so a run that went looking for a process could shoot either, and the evidence it filed
would read the same. And a screen road creates projects, tasks and bindings, none of which belong
in the store the operator actually works in; a store the run makes and drops leaves nothing for
anyone to remember to tidy.

**A road can ask for the app itself to be run again** (`store run-again`), which is the one step of a
road the harness carries out rather than the operator: this app goes down, another comes up on the
same store, and the pid moves with it. It is the harness's for the reason the first launch is —
an app opened from the machine would come up on the operator's own backlog and under no pid the run
can shoot — and it is where a road reads what Amenbo keeps of a run against what goes out with one.
The app is killed rather than asked to quit, the way it is taken down at the end: asking goes through
the app's name, and a name cannot pick out one instance. It happens before that step is handed over,
so what the operator is asked to confirm is the window already in front of them.

**A road can also walk the app out of its own door** (`store quit`, then `store answer-quit`). This
one is the operator's: the way out is pressed at the screen — `how: menu` is the item `⌘Q` reaches,
`how: last-window` the close on the app's one window, and the two arrive by different roads inside
the app, so a road that walked one proves half the gate. What comes of the press is what the step
declares: `asks: true` is the question that stands where a terminal is still open, and `asks: false`
is the app ending on the gesture because there was nothing to lose. `answer-quit` answers that
question — left out for the plain one, or naming `leave` or `cancel` with the `target:` the box has
to name. The box names reservations and moves none of them, so there is no answer that writes the
ledger. Every answer but `cancel` ends the app, and the harness brings another up on the same store **after** the step is
handed over rather than before it: the operator is the one who watched it go, and a shot is aimed at
a pid, so there has to be a window again by the time the step is photographed.

The executable inside the bundle is started directly rather than the bundle being `open`ed, since
the environment is what carries the store and `open` hands the launch to launchd with an
environment of its own. `AMENBO_HOME` is the product's own override, so the build under test is not
a different build for having been asked. `AMENBO_UPDATE_CHECK=0` rides along with it, the same way
it does on the CLI side: the app asks the release manifest as it comes up, and a road walked over
and over would otherwise put every one of those launches into the numbers the product is measured
by. The `PATH` is the third thing the launch carries, and the only one that is about the machine
rather than about Amenbo: the session's own directory of stand-in programs goes in front of the
inherited one, which is how a road says what a pane could be opened with
([`terminal can-start`](#given--the-world-a-road-starts-from)). It is handed over on every launch and
is empty unless a premise filled it, so a run that asked for nothing is a run on the operator's own
machine. The store follows this workspace's throwaway rules — one parent under the temp tree, a name
that does not lean on the pid, a sweep of what is over a day old on the way in — and the app is
started from a directory of its own, since a child inherits the harness's, and the harness is run
from this repository.

**The world a road starts from is stood up before the app is.** A scenario's [`given:`](#given--the-world-a-road-starts-from)
is walked by the CLI the same bundle ships (`Contents/MacOS/amenbo`), pointed at that same throwaway
store: the build under test stands up its own world, rather than whichever Amenbo the operator has
on `PATH`. It runs before the launch, since a store is read as the app comes up, and before there is
an evidence directory, so a premise that could not be stood up leaves no shots of a half-built
world. Where it stops is the screen's own moves — `steps_gui` is the operator's to walk, and a
driver that carried those out would leave the road proving something already done. A scenario that
declares no world gets none and opens on an empty store. What was stood up is written into
`manifest.json` under `world`, because the store it describes goes out with the run — and it is said
on stderr before the first step is handed over, because part of it is the operator's to reach for. A
file a premise wrote lies under a throwaway path, and the instructions are rendered from the YAML
alone, so nothing in the road can name it: the line that says where it landed is the only way to
find it in a picker.

An assert is judged by asking that same tool to read the shot back (macOS's own **Vision** behind
it): the harness derives the text the step expects on screen — for a `listed` assert, the bound
task's title — and passes when it is present in the reading (or absent, for `present: false`).
Both sides meet on their words rather than their glyphs — case, punctuation and the line a wrapped
card broke on are folded away, and so is the long vowel mark Vision returns where a title carries a
dash, which Unicode files under letters. That fold is the reader's habit, so the tool applies it to
what it read and hands back the unfolded reading as well; the harness folds its own expectation by
the same rule and matches. The reading as it came back is written next to the shot (`NN-…​.txt`),
which is what a person reads when a step comes out red.

**The shot is read whole, and then again in quarters.** Vision finds the regions on an image before
it reads any of them, and on a whole window's shot it misses small ones: a terminal pane half the
window wide wrapped `admin@…workshop % SCENARIO still taking what is typed` over three rows, and the
reading came back with the middle row's first half and the third row missing outright — not misread,
absent, while the same rows off a crop of that pane were read in full. The quarters overlap, so a row
the cut runs through stands complete in at least one of them, and what a cut did run through is
dropped rather than read: the whole shot's own reading has that row entire. A line the whole reading
already carries is not written twice, so the `.txt` beside the shot stays close to what one pass
would have said.

**One character inside those words is forgiven, and nothing more.** Vision reads the words on a
screen well and the glyphs inside them not always — `day's` came back as `dav's` on a title that was
otherwise perfect, and the fold keeps alphanumerics, so a verbatim search finds nothing. The
expectation is therefore matched with a budget of one edit over the whole of it, and only where the
folded expectation is at least 8 characters: under that a single edit is most of the word, and two
values a scenario tells apart (`core`, `gore`) are exactly that far from each other. The budget is
counted in characters rather than in words because the screen is also read in Japanese, where the
fold leaves a title with no spaces to count. Two misreads in one title is not what this is for —
that shot goes red and a person reads it. Which way it leans is the reassuring part: the same
tolerance that finds a misread title on a `present: true` step finds it on a `present: false` step
too, so it can red a run and never green one on a screen nobody stood up. A step that passed only
because a character was forgiven says so in the summary and carries `slipped` in `manifest.json`;
several of those in one run is a reader going wrong rather than a screen.

**Where one word ends is not read at all.** The reader cuts a row into regions wherever it likes and
the fold puts a space at every seam, so a pane that broke `before` across two rows hands back `be
fore`; and inside one region it drops a space that was there, so `the run` comes back as `therun`.
Both were on one reading of one terminal pane. Neither is a word misread — every letter is right,
and in order — so an expectation the spaced reading does not hold is matched again with the spaces
taken out of both sides, and the best of the two greens wins: one that needed nothing forgiven beats
one that spent the budget. That is what keeps the budget for the letters it is for, a pane being
narrow enough that a wrap and a lost space land on the same shot. What it gives up is telling `the
rapist` from `therapist`, which no reading of this screen could do either.

**Two pairs of glyphs are folded onto each other before any of that**: the digit `1` against the
letter `l`, and the digit `0` against the letter `o`. They are one drawing, not a reader's slip, so
they cost nothing out of the budget above and reach the expectations the budget cannot — a category's
key is a monospace word of five or six characters, well under the floor, and `channel` came back as
`channe1` off a shot it was plainly legible on. What it gives up is telling `route1` from `routel`,
which no reading of a photograph could do anyway. A lowercase `i` is deliberately not in the set: the
face this serves draws it with a dot, so folding it onto `l` would give away discrimination against a
misreading this screen does not produce. A green earned this way carries `slipped` like any other.
An assert OCR cannot mechanically judge — a structured `field` value — is a `Review`, unless it is
one of the few read off the window's tree or off the store instead (below): its shot is kept for an
AI/human eye and does not fail the run. A task's **title is one of those once the task
has ended**: done and rejected are drawn with a line through them, and the reader returns the glyphs
under that line as other letters (`SCENARIO — work is over` came back as `SCENARIOwotk is eveF`), so
no fold brings the two sides together. The harness follows each binding through its terminal states
and leaves such a step for an eye, saying so in the instruction. The half worth knowing is the
absent one: a reading that cannot find a title it is looking straight at passes a `present: false`
step, so those lines read green while proving nothing — which is what this takes away. Write the
machine-judged half of a road on cards that are still open. tesseract stays the Linux container path
(`scripts/docker/gui-e2e.sh`); each driver walks the road written for it.

### The asserts no screen draws — read off the store

OCR and an eye can close only what is drawn somewhere, and some state is not. The file a reader
chose for an avatar is kept beside the 96px square baked from it, and no screen draws that
original: left as a `Review`, a step about it says nothing a release could stand on, since its
shot is of a screen the state was never on.

So a short, closed table of asserts is put to the **store** instead — `amenbo_verify_gui::reads_the_store`,
which holds `store blobs` and nothing else yet. Such a step is judged by the very arm
the CLI road judges it with, reached through the driver that stood the world up, so the two drivers
answer one question one way. It comes out `read` in the summary (`☑`) and in `manifest.json`, and
carries what the store said as `told`. A count that does not meet reds the run like any failed
assert; a store that cannot be read at all ends it.

Where the reading is taken settles three things:

- **The step is still handed over and still shot.** What makes the reading honest is that the screen
  in front of it is the one the step before stood up, and the shot is the evidence of that.
- **It is read in the step's own place, not once the road has been walked.** A road says how many
  blobs the store holds before an image is registered and again after; a single reading at the end
  would answer both with the state the last step left.
- **A road carrying one needs a `given:`.** What reads the store is the premise's own driver, and
  booting one for a road that declared no world would raise a project the road was written without —
  so such a road is turned away before a store is made.

What stays off that table is as much the point. An assert whose words *are* on screen remains the
screen's: reading the store for those would check the build's records against themselves rather than
against what a reader sees.

### The asserts a screen draws cut — read off the accessibility tree

A name is drawn in the space the element carrying it stands in, and a name longer than that space is
drawn cut, the tail replaced by one glyph. Two of them are known, and both went red against a build
doing exactly what it should:

| assert | the space | drawn | read |
|---|---|---|---|
| `files listed` | the rail a bound folder's tree stands in, whose width a person drags | `grafting…` | `grafting.md` |
| `terminal label` | the row above a pane, which is only as wide as the pane | `SCENARIO named by h.` | `SCENARIO named by hand` |

Widening the rail, or closing the second pane, before every run is not a road anybody walks.

The eliding is the drawing and not the element: the window's accessibility tree carries the whole
name. So a second short, closed table — `amenbo_verify_gui::reads_the_tree`, which holds those two
and nothing else yet — takes its reading off that tree instead, by asking the screen tool to list
every named element under the window the step was shot at (`screen find <pid>`). The listing is not
parsed: what the assert asks is whether the name is on that window at all, which is the same search
a shot's reading is put to, so it goes through the same fold and the same match. A step on this table comes
out `pass`/`fail` like any other, and the listing is filed beside the shot the way a reading is.

**The shot is still taken and still filed.** The reading moves; the evidence does not — the picture
is what an eye reads the step back from, and a tree read is no more legible to a person than a
photograph of a screen nobody kept.

What stays off that table is as much the point, for the reason the store's is. An assert whose words
a shot can be read for stays the shot's: a build asked what it drew is a build checked against
itself, where a reading is the screen checked against the road.

### The asserts no window holds — read off the menu bar

Everything above is on the window a step is shot at. The bar across the top of the screen is not: it
belongs to whichever app is frontmost rather than to any one of its windows, and a menu nobody has
pulled down draws its items nowhere at all. So an assert about the words in it can be read off
neither a picture nor the window's tree.

It is Amenbo's own surface for all that. The bar is built by the app from the same dictionary its
screens are drawn from, and a build that translated its windows and left its menus in English would
pass every road here — which is what `store menu-reads` is for, and it is the whole of the third
table (`amenbo_verify_gui::reads_the_menu`).

The reading is `screen menu <pid>`: the app's own menu bar walked from its accessibility tree, each
heading and the items under it, one to a line. Three things follow from where it is taken.

- **The Apple menu is left out.** AppKit draws it in every app's bar and fills it with the system's
  own words, none of which are the app's — and every one of them would be a word a reading could
  find while looking for one that is.
- **Nothing is pulled down.** The tree carries the items of a closed menu, so the operator is asked
  to confirm a bar that is already standing there rather than to hold a menu open to be photographed.
- **The step names words of the interface, which no other assert does.** A road reaches one only
  after saying which language the interface is in (`store set-language`), so the word it names is one
  it is entitled to know.

The Linux container carries no toolchain, so it can't read the scenario itself. Its host launcher
(`make verify-gui-linux`) resolves the scenario through the `emit` bin and passes the card — the
`listed`/present title — into the container as `AMENBO_E2E_CARD`. tesseract reads the words but not
every glyph, so that path matches the title on its alphanumerics, not verbatim. `SCENARIO` selects
which scenario drives it (default `scenarios/delegate-to-ai.yaml`).

```sh
cd verification
# the crate's JSON face over the validated model — a shell consumer reads it through jq,
# never by reparsing YAML:
cargo run -p amenbo-scenario --bin emit -- scenarios/delegate-to-ai.yaml
```

```sh
cd verification
# launch the installed bundle against a throwaway store and shoot one shot per step:
cargo run -p amenbo-verify-gui --bin verify-gui -- scenarios/delegate-to-ai.yaml \
  --app ~/Applications/Amenbo.app
```

`--app` is the bundle itself, not a name: what is verified before a release is the `.app` the
installer put in place, and the bundle is asked whether it is one — through the CLI it ships, which
one build produces alongside the app and one installer carries with it. The executable to start is
asked of the bundle too (`CFBundleExecutable`) rather than assumed.

The screen tool is the input primitive too, called by whoever drives the screen between steps: its
`find` / `click-named` / `right-click-named` / `dblclick-named` / `point-named` / `click` /
`right-click` / `dblclick` / `point` / `drag` / `type` / `key` / `scroll` / `set-date` carry out the action steps the
checklist names. The run holds itself at the launch until the app is up, in front, and can be shot
at all — the proof it waits for is a shot it throws away, since an app the system has taken up is
not yet an app with a window, and a walk that started between the two would fail on its first step.
An app that never draws one inside a minute is reported as that, and one that exits on the way up is
reported the moment it does. `--evidence <dir>` chooses where
the shots and manifest land (default: a fresh dir under the temp tree); `--screen <path>` points at
a tool other than the repo's own, and `--fixtures <dir>` at a shelf other than
`verification/fixtures`. Both of those last two are resolved from where the harness was compiled
when they are left out, so both have to be given to a run that is not standing in this tree — which
is what `devtool vm verify run` passes for a run in the VM.
Exit is 0 when every assert a machine judged — off the shot, or off the store — passed and every
step was captured, non-zero on a failed assert or a load/capture/reading failure — a `Review` step is
closed by a human from the evidence, not by the exit code.

**Name what to press rather than aim at it.** `swift scripts/screen.swift find <pid>` lists every
element on screen with the name it answers to and where it stands, and `click-named <pid> <name>`
clicks the one of that name — bringing that pid's app to the front first, since a press lands on
whatever is frontmost where it is aimed and anything that took the front would swallow it silently.
`right-click-named` is the same press with the other button, which is the only way to reach a menu
drawn where the pointer is: until it is up there is no name on the screen to aim at.
`dblclick-named` is that press counted twice, for a row a single one only selects. The coordinate
`dblclick` is not a substitute: it takes no pid, so it fronts nothing, and a press meant for the app
under test lands on whatever window took the front — which no shot of the app says.
`point-named` is the pointer arriving and stopping there, which is the one thing none of the others
do: what it is for is a face that draws something for a pointer resting on it and takes it away when
it leaves — a panel dropped under a row — and every verb that presses would take that away again.
The shot the reading is made from is the one after it, so the road holds the pointer in a step of its
own (`terminal hold-label`).
The screen is a webview, so all three read it through the accessibility tree the app serves once
asked. They read one window and not the app, and take the same `--window <title>` a road's step
does — an app drawing two draws two screens, and a name reached on the wrong one is a check that
passed without looking at the screen it was written for.
A part of a name will do — the name an element answers to is not the label
on the screen (an emoji in front of the words belongs to it, and a card folds its lines into one
string), so a whole one is rarely knowable in advance. When several names hold what was asked for,
the tool prints them and presses nothing.
**A field is called by what it holds, not by the word standing beside it.** The task pane's assignee
answers to `Unassigned` or to the person it holds; `Assignee` is the label next to it, a piece of
static text with nothing to press — and a press on that exits 0 having done nothing, which is the one
failure a road cannot see. `find` says which is which in its first column, and
`--role <role>` presses the one of that kind: `click-named <pid> Unassigned --role AXPopUpButton`
reaches the pane's own field where the filter panel's `AXCheckBox` carries the same word. It is also
the way past the refusal a name in two places ends in — a point is what that costs otherwise, and the
paragraph below is why a point is the last resort.
A point worked out from a shot's pixels carries two errors instead: the shot's pixels are the window's
points times the scale of the display it was on (2 on a built-in panel, 1 on an external one), and the
screen goes on moving after the shot — opening the right pane pushes a column header down by tens of
pixels. Anything wide swallows both, which is why aiming works until it is aimed at something small:
the board's `＋` and the view tabs read as unreachable elements until the arithmetic was suspected
instead.

**What the tree holds is not what the window shows.** A webview keeps a row it has scrolled out of
sight in the tree, named and framed like anything else, and the frame stands past the window's edge.
A press aimed there is a screen point like any other, so it lands on the desktop and exits 0 having
pressed nothing — the same failure a road cannot see as a press on a label. `find` writes `outside
the window` at the end of those lines, and `click-named` and its two siblings refuse them instead of
aiming at them: scroll it into view first, then press it by name.

**A point is refused on the same ground, against whatever the subcommand knows.** `drag`, `drop-file`
and `scroll --at` are handed a pid, so each end is held to that window. `click`, `dblclick` and
`right-click` are not, so theirs is held to the displays instead — a point on none of them is a press
nobody could have meant, and it is the shape the scale conversion arrives in when it is made the
wrong way round.

**A page is walked with `scroll <pid> <dx> <dy>`, not with the keys.** Page Down is the one scrolling
key that reaches the webview — Page Up, Home, End and the arrows were posted the same way and nothing
moved — so a road that went down a pane had no way back up to what it had passed, and reopening the
pane does not reset it either, the position being kept. A wheel arrives where those keys do not.
Positive is the way back: `scroll <pid> 0 800` goes 800 points up the page, and toward its left
across — the amount is in points, not notches of a wheel. The pointer is put in the middle of the
window first, since a wheel lands where it is pointing rather than on whatever holds focus. That is
the whole of which pane moves, and clicking into one first changes nothing: the click moves focus,
where the wheel does not look. A window split into panes takes `--at <x> <y>` for the pane to move,
the middle standing on a divider or on another pane there.

**A card is carried with `drag <pid> <x1> <y1> <x2> <y2> [steps]`, and not out of two clicks.** Filing
work by moving its card is a road on the board, and what the screen is watching for is the run of
moves between the press and the release — a press at one place and a release at another is a click at
the second one. So the pointer is walked across with the button held, in `steps` moves rather than
one: a webview works out where the pointer is on every move it is given, and a jump straight to the
far end gives it exactly one.

**Both ends are points and not names**, unlike everything else that can be aimed by one. Where a drag
lands is decided by which side of a row's middle it is let go on, and both sides of that line are the
same row — a name says which row and cannot say which side of it. So the arithmetic is the caller's,
and `find`'s rectangle is what each end is built from.

**A file comes in from outside with `drop-file <pid> <x> <y> <path>…`, and `drag` cannot stand in for
it.** What crosses the screen when a file is dragged in is a dragging *session* — a pasteboard
travelling with the pointer — and moving a pointer carries none of it: a pane walked over that way is
told nothing at all. Nor can the moves be aimed at the file manager the file is picked up from, whose
rows nothing outside it reaches. So the tool begins the session itself, off a window of its own, and
lets it go at the point named. Several paths in one call are one hand full and not several drops,
which is the whole of what a road asks of that gesture. What is brought has to be a file on the
machine the screen is on — the same thing the instruction asks the operator for — and a drop nothing
took is refused rather than reported as done.

**A day goes in through `set-date <pid> <name> <yyyy-mm-dd>` rather than through the keys.** A date
field is one control with three numbered parts in it, and typing into it is a digit at a time — but
every digit that leaves the value a valid day makes the app commit and redraw the field, and the
redraw drops the run of digits the webview was collecting. A year is four digits and valid after each
one, so `2099` arrives as `0009`. Typing slower does not help: it is the redraw between the digits,
not the pace of them. The tool opens the field's picker, writes the day where the picker keeps it,
tabs back out so the panel is not standing over the rows beneath, and reads the field back — so a
write that reached nothing is reported here rather than as a red assert on the day.

**`--near <name>` says which row, where the field's own name does not.** A manager listing the values
of an axis draws every row the same `Start date` and `End date`, so the name reaches one field per
value and asking it to name itself more fully asks for a name that is not there. What is there is the
value's own name, on that row: `set-date <pid> "Start date" 2001-01-01 --near Ongoing` writes into the
`Start date` standing beside `Ongoing`. Same row is read off the frames the tree answers with at that
moment — no shot, no scale, nothing that a screen having moved could put out — and a `--near` that
leaves several fields, or none, is refused the same way an ambiguous name is.

### `--print` — read the road without a screen

The screen road is written in YAML, but the sentences it turns into are written in Rust, so what a
step will actually say to an operator cannot be read off the file it was written in. `--print`
answers that: it renders the road and prints the instructions, one to a line, and stops there — no
app is launched, nothing is shot, no OCR runs, and no GUI has to be built first (`--app` is not
even asked for).
The lines are the very text a run hands the operator, not a second rendering that could drift from
it, and a road carrying an op the harness has not mapped fails here exactly as it would mid-run.
A scenario with no `steps_gui` is refused by name, the same as it is for a real run.

```sh
cd verification
cargo run -p amenbo-verify-gui --bin verify-gui -- scenarios/link-a-folder.yaml --print
```

### Every step is handed over before it is shot

A screen road is walked by somebody, always. The run prints the step it is about to shoot and waits
for a line on stdin; between the two, the screen belongs to whoever is driving — carry the step out
by hand, or with the screen tool's `click-named` / `drag` / `type` / `key` / `scroll` / `set-date`, and send the line
once the screen is standing where the step says it should. There is no flag for running it any other way.
A step that names the key this machine copies or pastes with is `key 8 --cmd` and `key 9 --cmd`:
the modifier is a flag on the press rather than a key held around it, so nothing stays down between
steps. A press is held the same way — a step that adds a row to a selection with the key this machine
adds with is `click-named <pid> <name> --cmd`.

**The hand-over comes before the step, the first one included.** That is what lets a road open with a
check: a run starts on a store made for it a moment ago, so the screen a launch leaves behind is
whatever a store nobody has used yet opens on — the hooks question over an empty board — and a road
whose opening line reads the board would be judged against that. Handed the step first, the driver
stands the screen where the line says before anything is captured.

Left to itself a run would photograph one screen as many times as the scenario is long, and pass:
the verdict is a substring in what OCR read off the shot, so the screen before a step and the screen
after it are not told apart, and a line asking that something *not* be on screen passes for as long
as nothing moves. A step nobody carried out is the one thing this harness cannot judge, which is why
it never runs without somebody there.

It can, though, notice one trace of it. An `action` whose shot comes back as the picture the step
before it left is remarked on where it is shot, on a line marked `!` among the hand-overs:

```
  ! step 4: the screen is the one the step before it left — this shot and that one are the same
    picture. An action that was never going to move anything reads like this too, but so does one
    nobody carried out.
```

A remark and not a refusal, because the two it cannot tell apart are both ordinary: a step handed
over and never carried out, and a step that was never going to move anything (a face already
showing, a tree already open). The driver can tell those apart; the harness cannot. Only actions are
asked — an assert is meant to shoot the screen the step before it stood up — the app's own restart
(`store run-again`) is left out, being the harness's move rather than the driver's, and two shots of
two different windows are never held up against each other. The comparison is byte for byte, so two
shots that differ by a blinking caret are two pictures here and pass without a word: a remark
withheld costs a run the driver would have caught anyway, where a remark made about a screen that
did move would cost the driver their trust in every later one.

The wait is on a line and not on a clock on purpose. A run held for a fixed number of seconds shoots
whatever is up when the clock runs out, so a step that took a moment longer is filed as evidence of a
screen nobody stood on — a red nobody can tell from a real one, or worse, a green. For the same
reason end of input is a failure and not a nod: a run with nothing left to hold it would walk the
rest of the scenario off one screen and report it as though it had been driven.

What an assert expects OCR to find is shown with it at the hand-over. The reading is a substring
match and nothing more, so the driver — who can see the screen — is the one who can tell a check that
genuinely passed from one the words happened to satisfy, and say so when the evidence is read back.

The moves themselves are written in the scenario, not in a note beside it. Getting from one screen
to the next is an action step like any other (`folder open-existing-card`, `folder choose-project`),
and it earns the two things a note never can: the screen it arrives at is shot, so the middle of the
road is evidence rather than something taken on trust, and the assert after it cannot be reached by a
hand tidying the screen while the run is held. The screen roads are written that way: `link-a-folder`
walks the arrival screen, the card, the picker and the board it lands on,
`say-where-a-plugin-fires` walks one plugin's row through the four states one switch leaves it in,
`learn-a-plugin-reads-more-than-this-project` walks two rows on that same screen for the one thing a
declaration says and a switch cannot — a plugin declared the machine's says in words that it reads the
whole device, and the plugin beside it that declared nothing says nothing —
`put-a-device-wide-plugin-to-work` walks that first plugin's one row all the way through — the mark it
wears, the settings opened inside it, the press, and then a project's own settings offering no second
switch for it —
`put-a-plugin-to-work-from-the-project` walks the same crossing from the other face — the arrival, the
picker that draws the row, and the switch inside it —
`fill-in-what-a-plugin-cannot-fire-without` walks one row from the mark it wears through the refusal, the
settings opened inside it and the press that then goes through,
`learn-why-a-plugin-will-not-turn-on` walks the gate that turns on the author's own judgement — the two
sentences a refusing check puts on the form after a save, the value staying saved and the plugin staying on
under them, and the same check standing in front of the switch when it is pressed again —
`see-only-the-settings-that-apply` walks the same form one layer up — the candidate an author withheld,
the field that candidate gates and the operation acting on it, each absent until the answer above it is
given, and the field and its button arriving together —
`choose-from-what-a-plugin-offers` walks a settings form through the three answers a choice holds and
the button back to the author's default, `press-what-a-plugin-offers-to-do` walks the other half of that
same form — the operation its author declared, drawn but unpressable while the gate is shut, the box it
asks for coming up at the press, the author's own line coming back with what was typed in it, the
second press finding that box empty again, and the button beside it asking for a credential through a box
that draws none of it back — `finish-writing-a-task-before-anyone-takes-it` walks the one
premise a reader settles where it is reported — the card drawn while its creation is still open, and
the button inside it that ends the creation, `read-a-plugin-in-your-own-language` walks a listing
across a language change — the line one author wrote in the reader's language, the row beside it whose
author wrote none, the shelf that published no document for that language at all, and the panel whose
label came down with every language inside it — `read-a-plugins-form-in-your-own-language` walks the
same change on the other side of an install, where the words were never published anywhere and so
cannot have been fetched: the field translated, the field beside it that is not, and the candidate drawn
under one set of words while storing the value it always stored — `learn-what-a-plugin-does-before-installing-it`
walks the third of those doors, the panel an opened row is read by: the author's own description standing
there with no README beside it, the same description again after a language change, the row whose author
wrote one language keeping it unmarked, and the row nobody described at all falling back to the
repository's README — `see-a-tasks-classification-on-the-board` walks what a card says of how its task
is filed, on a board whose world was classified from a terminal: the value drawn for the axis its
project put on the card, and nothing at all from the axis beside it that did not ask —
`answer-one-category-with-several-values` walks the same board over a category one task answers twice:
both values on the one card, the category missing from the row of buttons that cut the columns while
the category beside it is still offered there, and the pair still drawn once the board is cut along
that other one —
`file-work-by-moving-its-card` walks that board as the place work is filed from rather than read on —
the column standing before anything is in it, a card carried into it and another carried elsewhere,
and the narrowing afterwards that comes back with the first and leaves out the second —
`set-an-app-up-to-reach-this-project` walks the fold that holds the other way in — the app already reaching this
project and the folder its entry names, beside the app reaching nothing, each row offering the one road its app can
walk —
`be-offered-a-start-at-login`
walks the one thing on screen nobody went and asked for — an offer that comes up on its own once the app has been come
back to, and the answer that takes it away — and the hourly check's roads close the set:
`be-offered-the-hourly-check` walks the band that arrives when a dated task, a carrier and an unanswered
device stand together, the yes read back on the settings row, and that row taking it back off;
`take-back-a-no-about-the-hourly-check` walks the way back a declined band, rightly, never offers;
`not-be-asked-about-the-hourly-check-nothing-would-carry` reads the band staying down when nothing
enabled would carry the warning a yes would start; and the pair around its third button —
`be-asked-again-after-putting-the-hourly-check-off` and
`not-be-asked-twice-in-a-day-about-the-hourly-check` — reads the one day of quiet a "later" buys from
both of its sides, on the deferral day the `tick deferred` premise stands up.

Everything the wait prints — the step about to be taken, and the prompt — goes to stderr, so `--json`
still leaves one machine-readable line on stdout. A driver that is not a person keeps its side open
through a pipe:

```sh
cd verification
mkfifo /tmp/go
cargo run -p amenbo-verify-gui --bin verify-gui -- scenarios/link-a-folder.yaml \
  --app ~/Applications/Amenbo.app --json < /tmp/go &
exec 3>/tmp/go   # hold the writing side open — otherwise the first echo closes it, which is the end
                 # of input, and the run stops rather than carrying on to the next step
# … stand the screen where the step just handed over says (the screen tool), then release its shot:
echo >&3
# … and when the last step has been shot, let it go:
exec 3>&-
```

## Scenario format

A scenario is an `id`, a human `title`, an optional `description`, an optional `given` (the world
the roads start from), and an ordered list of steps
under `steps_cli` and/or `steps_gui`. Each step is an `action` (changes state) or an
`assert` (an expected result), names the `domain` it touches (`task` / `decision` /
`comment` / `project` / `dimension` / `attachment` / `store` / `folder` / `repo` / `plugin` /
`mcp` / `tick` / `terminal` / `files`) and an
`op`, and carries named args under `with`. An action may bind its result with `as:`, and a later step
refers back to it with `target:` — an op that joins two objects names the second under its own key
(`decision link`'s `task:`), and every such key is checked back to an earlier binding, not just
`target:`.

The last four are not things filed in a store: `store` is this device's Amenbo itself — its
settings, the identity it answers `whoami` with, the build in place, and the store as a whole
(`export`, `backup`, `restore`, the integrity reads) — `folder` is a directory and the project its
`.amenbo` names, `repo` is the folder the run works in as a place with files and a git history, and
`plugin` is what is installed on the machine, whose gate is open, and what the execution log kept.

Not every object is reached by a binding. A **dimension** travels as the words a person says — its
axis and value are named in `with` (`dimension: <axis name>`, `value: <value name>`), which is what
the command takes too; a bare number there would be read as a name, not an id. Either word may also
be the **key** the row answers to, since that is what the command resolves before it
tries a name — a road writes one where being typed from outside is the point, and the name
everywhere else. The key itself is named at birth by `slug:` on `dimension create` / `value-add`,
renamed afterwards by `dimension rekey`, and read back by the `key` assert. A **folder** travels
as a plain name too (`dir: shared`), and for a different reason: a binding is answered by where a
folder sits, so the driver is the one that places it — clear of the run's own bound CWD, which a
pointer search would otherwise walk up into. One kind of folder name is answered without being
placed: a folder a road **moved** is named again afterwards — `folder rebind`'s `moved:`, `folder
vanished`'s `gone:`, `folder repointed`'s `previously:` — to say which binding is meant, and placing
it would put back the very path the move took away, which is the whole state those steps are about.
Those names are answered from what the run moved instead. A folder name also travels outside the
`folder` domain: `task update`'s `at:` and `task worked-in`'s `dir:` name which of a project's folders
a task is worked in, and are placed and read the same way the `folder` steps' are. A **plugin** is
named the way the catalog names it (`name: worktree`), which is what every one of its commands takes.

`plugin run` is the one place where a step's arguments are not Amenbo's. Everything after the
plugin's name belongs to the plugin, so `command:` is the word its own face takes, `task:` hands it
the id of a task an earlier step created, and `args:` carries anything else through verbatim. The
value that comes back is read by the `returned` assert, which has to **follow its call**: a command
face's return value is its own stdout and is deliberately not written to the execution log, so
nothing else can go and fetch it afterwards.

A **`store` action that writes a file** binds it through the same `as:` an object is bound by, and
what the name then holds is the file: `restore` names the archive it puts back the way any step names
an earlier result, so a mistyped name is a lint failure and not a driver hunting for a file nobody
wrote. The files land in the run's own throwaway space and go with it.

A **`store` action that reads a number** binds the number itself, which is the third thing a name can
hold. `sync-version` is the one: what a carrier watches is a value, not a row and not a file, and the
only thing a later step can say about it is that it moved or did not (`store version`'s `since:` and
`moved:`). Which of the three maps a name lands in follows from the op that bound it, as it does for
the other two.

One domain is not in the store at all. **`repo`** is the folder the run works in: `write-file` puts
a file there (what an attachment ingests, what the lint is pointed at), `copy-fixture` puts one
there from `fixtures/`, and `git-init` makes the folder a git repository, which is the only way the
hook slots are real enough to write into. That one takes the same `dir` the first two do, and a road
reading what git says about a folder on screen needs it: the colours are drawn on the face of the
folder a project is **bound** to, and a repository anywhere else leaves every row of it bare.
`git-commit` records everything lying in that folder, and it is there for one state nothing else
reaches: until something is committed git names the whole folder and never the paths inside it, so a
folder git is quiet about while a file in it is new — which is what a folded row on the tree answers
for — does not exist on the near side of a commit.
`wire-ai` is the same kind of stand-in one tier up: Amenbo
hands over the text that starts a folder's AI on it and writes no settings file itself, so the road
past that point exists only if someone pastes — and it pastes what the build under test handed over,
into the file that build named. All of it stays inside the run's own throwaway folder — a
path that is absolute, or that climbs out with `..`, is refused.

One more is not in the store either, and is not a command at all. **`mcp`** is Amenbo reached over a
protocol rather than typed at: `serve` starts one server for one folder and holds it up
for the rest of the road, `call` calls a tool on it with the words a caller sends, and `offers` and
`answered` read what it published and what came back. It is a domain of its own because a protocol
has a conversation where a command has an exit code — the server outlives the step that stood it, and
a tool that ran and refused comes back as a *result* marked in error rather than as a transport fault,
which is what an assert there is written against.

And one is nobody typing at all — on its command side. **`tick`** is the machine's own scheduler
starting Amenbo once an hour, and what Amenbo works out once it is awake: `woken` carries out one
turn and judges what came back, which is the whole of what a scheduler ever gets — a tick leaves a
day mark and nothing else a reader can ask for, so being woken *is* the reading. `holds` reads what
the run *did* to the registration — `changed: false` being "left the machine as it was found".
**Nothing there writes one.** A registration lands outside the throwaway store a run makes, in the
launchd, systemd or Task Scheduler of whichever machine the gate is running on, which is also why
the reading is a difference and not an absolute: the store's isolation does not reach the scheduler,
so a road asking whether a registration is held at all would read the one belonging to whoever is
running the gate. A road that installed one from the terminal and left it would leave an hourly
timer on a release box — `be-offered-a-start-at-login` draws the same line for the login
registration, and that half is walked on the real machines.

The consent in front of that wake is the one part of the tick a person meets, and it is on a
screen — so these are screen roads alone, the terminal's way in (`tick install`) asking
nothing. `banner` reads whether the band offering the hourly check is standing across the app, up
only while the device is unanswered, a dated task is open and a `task.due` subscriber is enabled
somewhere; `banner-answer` gives one of its three answers (`start` / `never` / `later`); `setting`
and `set` read and move the row in Amenbo's own settings that holds the answer afterwards — the one
way a no is taken back. A road that answers `start`, or moves the row to `on`, registers the timer
for real, so it takes that back (the row to `off`) before it ends, and what it asserts in between
stays on the answer's side of the line `holds` draws: the answer having landed, never the machine's
registration as an absolute. The last piece is a premise rather than a road: `deferred` stands up
the day the band was last put off, because "later"'s whole meaning — quiet today, back tomorrow —
spans two launches, and no single run holds both.

And the last one is about no record at all. **`terminal`** is the face an agent is run in: whether
the app is showing the ledger or the pane (`show-face`), what a reader typed into that pane
(`type-line`) and what a reader set running in it (`keep-printing`), and whether the pane is a face
of the one window or a window of its own (`split-out` / `fold-back`), with `pane` reading the line
back. `say` is the other half — the surface layer, said
with the CLI from *inside* the pane it is about (`verb` naming which of its words, `text` what was
said in it) — and `label` reads what the row above the pane carries afterwards. It is a domain of its own because a session is a
process — what is under test is *where it is drawn*, which is this machine's arrangement of one
screen and nothing the store holds. Screen roads alone, and for a reason no other domain has: the
terminal is the surface a reader is already typing in, so the question does not arise for somebody
at a shell.

What a road says *in* a pane it says to a shell. A pane comes up on whatever agent the folder starts
with, and a command handed to an agent is a request — whether it is carried out is the agent's own —
so `open-shell` takes the pane down to a plain prompt first, and every road that speaks in one takes
it. `run` is a command run there for its output: it clears the pane before it, so what a step after
it presses is one place on the screen rather than one of several, and where the command needs a
record's own number it carries `<ref>` and names the record beside it, a road having no way to spell
a number the run will mint. **The clearing is a key press and not a typed `clear`**, because the
words on a pane are what every `shows` names it by — so a step of the harness's own that typed
something there would be writing on the screen a road reads. The same rule ends a program by a press:
`open-shell` and `end-pane` both say control-D — the end of input, which is not a line — rather than
a typed `exit`, which would have put `exit` on every pane they walked through.
`press-ref` is the press itself — the ref where the output
drew it, naming the record rather than the characters — and `folded: true` asks for the same press on a ref
the pane broke across two rows. That last one is the only place the two ways a ref becomes pressable
part company: what Amenbo's own output says of itself travels beside the characters and a fold cannot
touch it, while reading them back off the screen means joining the rows a line was drawn across
before anything can be found there at all.

The face's own arrangement is in that vocabulary as well, because on this face the arrangement is
most of what there is: `set-panes` re-cuts the frames into pages of the count it names, `go-page` moves
the whole screen to another page, `go-project` moves it to another project's panes altogether, and
`open-pane` starts a terminal where there is not one yet — `from: face` at the empty frame a page with
room draws, `from: strip` at the thin strip a full page draws instead, which opens nothing itself and
moves the screen to the page with room, where that same frame is waiting. What they are
walked for is what they must not do. Every one of those moves re-cuts or replaces what is drawn, and
a pane that left the screen is a pane still running behind it.
`come-back-to-the-terminal-a-page-turn-took-away` is the road for the two that re-cut. It types a
second line into the pane once the page has come back, which is the half a reading of the first cannot
carry: what a terminal printed stays printed, so the first line says the screen was restored and only
something typed after the return says anything is still listening. And it keeps a second pane going
for the length of it — with one frame there is nothing for a new count to move, so a road with a
single pane walks `set-panes` past the only thing it is dangerous for.
`go-to-the-panes-of-another-project` is the road for the one that replaces, and it reads the half the
other cannot: the tabs down the edge are the division itself rather than a grouping laid over one
list, so what a project's press leaves on the screen is that project's panes and nothing of any
other's.

`set-orient` is the one move on that row that re-cuts nothing. At two panes and at no other count it
says which way they sit — side by side, or one above the other — and every frame stays on the page it
was on. Two is where it is asked because two is where the face's rule turns around: width is spent
before height everywhere else, and two across halves a pane's columns, which on a window with a
column beside it is under the eighty a TUI wants.
`stack-the-two-panes-so-each-keeps-the-whole-width` is the road, and what `panes-sit` reads there is
the width rather than the arrangement — a build that shuffled the boxes about without handing either
of them more room would have honoured the press and missed what it was for. Both shapes are walked,
a page read only after the press having nothing to say about what it was before, and a terminal is
kept running across the change for the reason the re-cut roads keep one: the grid is redrawn under
the panes, and a page put up again rather than re-laid would come back drawn and with nothing on it.

What none of those moves reaches is the end of the run itself, and the arrangement is two things at
once: what the reader *set* — the split, the way two panes sit, the project they were looking at —
and what they *opened*, the places and the names on them. The line between the two is drawn only when the app goes out, the
first coming back and the second not, and `open-the-app-again-on-the-split-you-set-and-no-panes` is
the road that walks it with `store run-again`. What it reads there is a count of boxes and never the words on a pane: what a terminal
printed goes with the run whether or not its place came back, so a step reading for the line it typed
would pass on both screens. A page offering one way in and a page with the last run's places standing
on it are told apart by how many boxes are drawn, and the split by what the page does with a pane
once there is one on it.
`open-the-app-again-on-the-way-you-set-two-panes-to-sit` walks the same seam for the arrangement of
two, which the split road cannot carry: it cuts the page to one on purpose, and which way two panes
sit is only readable where two are standing. So that one presses no count at all — two is where a
fresh run comes up — sets the panes to go down the page, ends the run, and opens two on the page that
comes back. Both roads are about a line where **both sides look right**: a face that forgot and a face
that remembered are each a working screen, and nothing goes red between them.

Which folder a pane works in belongs to the same seam, because a pane belongs to a project and can
work in no folder outside it. So the press that opens one does not open a picker: bound to a single
folder the project is not a question and nothing is asked, and bound to several the press opens
nothing at all — `asks: true` says so on `open-pane`, and `pick-folder` answers the question that
comes up where the pane would have been.
`open-a-pane-where-a-project-keeps-more-than-one-folder` is the road, and it presses both answers
rather than one: a face that always opened the same folder whatever was pressed would walk a single
answer green from end to end. Each pane is then asked where it is standing, by reading out a file
lying in one of the two folders and not the other — a pane draws no path of its own, and that is the
only reading on this face that says *where* a terminal is working.

The other end of that question is where a frame comes from. A pane is made when the question is
answered and not when it is asked, so the box drawn while it stands is the question and walking away
takes it with it — `leave-question` is that move, and `leave-the-question-about-where-a-pane-runs` the
road. Where the leaving is done is the driver's to say rather than the road's, the question coming
down on a press anywhere else on the face. The road ends by answering the same question, since what
the walking-away did not leave behind is exactly what an answer makes.
`asking-folder` says whether the question is standing, by a folder it offers, and its absent half is
what says the box left with the question. It is a `Review` on both halves: the question comes up only
where a project binds more than one folder, and binding more than one is exactly what puts a heading
naming each of them on the column of folders down the side — so that folder is on the shot whether the
box is standing or gone, and only an eye can say which. The instruction names the box for that reason,
and says the column is not it.

`frames` counts what is standing on the page, and it is a `Review` for the reason `dot` below is —
a box carries no words of the road's, and an empty one would carry the interface's. What it defends
is that the pane count is the most a page draws rather than slots waiting to be filled:
`find-one-way-in-rather-than-a-page-of-empty-boxes` sets the count to four with nothing open and
reads the page still empty, then grows it one frame per pane opened. A face that filled its ceiling
with boxes would be asking the same question four times over, and nothing else here would say so.

`opens-with` reads the other thing an empty frame carries: the row above its press, which is what a
pane opened there would start with. It names `shell` and nothing else — which agents are on that row
is a probe of the run machine's own `PATH`, so a road naming one would run where that tool happens to
be installed and nowhere else, while the plain shell is on every row by construction. The road is
`open-a-pane-with-what-you-opened-the-last-one-with`, and what it defends is that a choice made once
outlives the press that made it: the page is set to one so the frame read at the end is a frame drawn
again rather than the one left standing, which is the difference between a build that keeps the
answer and a button that stayed pressed. What the row comes up on *before* anybody has chosen is
`start: none`, and it is the one reading here that no machine can be relied on to give — nothing on it
where several agents were found, that one where a single agent was, no row at all where none were, all
three correct on the machine they happen to be on. So the road that reads it stands the machine up
first (`terminal can-start`, below) and reads before anything on the frame has been pressed: one press
anywhere keeps this person's answer, and the first run is over in that store for good.
`be-asked-what-to-open-with-on-the-first-run` is that road. It reads both halves of the state, since
either alone passes on a build carrying the other fault — a row with nothing lit above a press that
opens anyway is a build guessing quietly, and a press that asks with a name already lit is a build
asking about an answer it has. Then it answers, and reads the frame standing beside the pane that
opens: the row on that answer, the press live. That is what says the asking was a state and not a
wall.

`dot` is the one reading on these roads that is not text at all. The mark on a pane's label pulses
while something is coming out of that terminal, and it is the only thing on screen that says a pane
is *alive* rather than drawn — a terminal that ended leaves its last output where it was, so words on
a pane outlive the process that wrote them. Two things follow for whoever writes a road with it in.
The pane it reads has to be one the reader is not working in, the focused pane never pulsing, since
somebody looking straight at a terminal can already see it moving. And the step is watched rather
than shot: a pulse rests, twice a turn, at exactly the still dot's own step, so it is a `Review` and
the instruction says how long to watch. A machine set to play no animation is in that instruction
too, holding the mark at the bright end of the same two steps instead of moving between them: the
fact survives with the movement gone, and an operator told only to watch for a fade would fail a dot
saying exactly what the step asks about.

The third thing about the mark is what `keep-printing` is for. A pane reads as moving for a moment
and a half after its last output, so a pulse anybody can watch for a few seconds is a pane that keeps
printing — and no other step on this face starts one. The line a road types is deliberately a command
no shell knows, which prints once and is over. `keep-printing` sets something running in the pane and
leaves it running: a bounded run, a line a second for a minute and a half, ending on a line the road
chose. **What the length answers to is the road's own steps and not the one press** — a pane has to
be opened beside this one and the lamp then read, each of them a whole turn of whoever is walking the
road, and printing that runs out in between makes a lamp doing its job read as a lamp that will not
light. It is not `run` with a longer command in it — that step is waited on and is over when the
prompt is back, and this one is walked away from with the output still arriving. Bounded, because that is how the still half is reached. Every control a pane has is on the
pane and the face takes a press anywhere in one as going to work in it, so there is no way to cut the
output short by hand that does not also make that pane the one being worked in — and a dot read there
is holding still because nothing draws it moving, which is a green step proving nothing. Left to run
out, the same pane crosses from moving to still untouched, and the line it ends with is what a road
waits on: `pane` reading the road's own words rather than a summary the interface writes in whatever
language the machine is set to.
`see-a-pane-is-still-running-without-looking-into-it` is the road, and its second pane is opened for
one reason — opening one makes it the pane being worked in, which is what leaves the first drawn with
nobody in it. Nothing is ever said to the second.

What a `pane` assert reads is deliberately the reader's own words rather than the interface's. Every
other word on that face belongs to Amenbo, so a reading of one would hold the gate to whichever
language the run's machine is set to — and, more to the point, a pane showing a fresh prompt looks
exactly like a pane still drawing the session that was running until a line the road put there is on
it. A window is named separately — `window:` matches on the title bar, which carries the frame's name
where a pane has one and the app's own word where it has none. A road that needs a window told apart
by a word of its own names its pane first (`name-pane`).
`put-the-terminal-on-its-own-screen` is the road, and it folds the window back before it ends
— the shape a machine was last used in belongs to the webview rather than to the throwaway store, so
a run that walked away split would hand the next person two windows they never asked for.

`say` is the one action on any road here that a driver could not stand up as a premise even in
principle. The surface layer has no existence outside a pane — said anywhere else it is refused, on
purpose — so the pane has to be running before the words can be said at all, which is after the app
is up and after a premise's turn is over. The operator types what an agent types, and that is not a
shortcut around the seam: it is the only door there is.
`be-told-in-the-pane-that-your-turn-has-come` is the road, and what it defends is the one thing
nothing outside a pane can find out — an agent going quiet because it is waiting for a person, rather
than because it is thinking — arriving where that person reads it.

Every road that speaks *in* a pane takes `open-shell` first. A folder with an agent on it opens on
the agent, which is what a reader wants and what a road cannot use: what an agent does with a line
typed at it is the agent's own, so a gate resting on one carrying out a command rests on a promise
nothing holds it to. One op covers the three shapes the face can come up in — a pane already running
an agent, the offer of several, the notice that none was found — because a plain shell is reachable
from every one of them, and which of the three is on screen is the run's machine's business rather
than the road's. The agent it has to end first is ended by control-D and never by a typed `exit`, for
the reason `run` clears by a press: the words on a pane are what a road names it by, and a step that
typed `exit` would have added the harness's own word to them.

`face-badge` reads the one thing that crosses between the two faces: the mark the terminal's segment
wears while a turn is standing behind it. It carries no number and no words, so a road says it is
there or that it is not — and the absent half is half the goal, since being on the terminal face is
being told and crossing over spends the mark. Raising one takes `say` with `away:`, the only word on
these roads said from behind the face that reads it: the layer is spoken inside a pane and the mark
is drawn on the other side of the switch, so the operator arms the word and crosses over before it
lands. `be-told-on-the-board-that-the-terminal-wants-you` is the road.

`tab-icon` is the other reading on this face that answers with no words at all: what the tab of a
project named is drawn with, the image registered for it or the colour and the letter it falls back to.
It names the project rather than pressing its tab, every project having one whether or not the face is
drawing that project's panes. `give-a-project-an-image-of-its-own` is the road, and it walks the
settings the image is given on before it crosses here.

And the last is the column beside those panes. **`files`** is the folder a project answers for, read
from inside Amenbo: the folder itself, folded down, with what git says about each row drawn as a
colour on it. Every op takes a `section` saying which part of the column a row is being looked for
in — there is one part to name today, and the arg is kept because the panel is not finished growing.
`tree` unfolds the folder's section, `enter` opens one folder a level, `open` presses a file and
`back` leaves it; `listed`, `reading` and `says` read what a row is, what the column has open draws
— a file, or the draft page on the first of its tabs — and one of the face's standing lines.
`row-mark` reads the colour a row wears, named by what git says —
`untracked`, `added`, `modified` — rather than by the colour itself, since which colour that is
belongs to the theme. It is a `Review` and can be nothing else: a shot is read for words, and a row
wearing a colour says the same letters as the row beside it that wears none.
`show-as` puts a Markdown file in one of its two forms — `rendered` for what the text says, `source`
for the text itself — and `reading` takes an `as` saying which of them its words are standing in,
which hands that step to an eye: both forms carry the same words, and what separates them is
punctuation the fold throws away and a size no reading reports.
`reopen-with` reads an open file again as an encoding the road names, and `read-as` reads back what
the row says it was read as. The two are one control apart and stay two ops: one is about the bytes
and what they mean, the other about the screen and what it draws — a road that named an encoding to
change a form would be asking one question with the other's word. What the *guess* said is never asserted: it reads the machine's own
language out of its settings, so where it lands belongs to the box the run is on rather than to the
build. What belongs to the build is that the row follows the reader, which two namings prove and one
cannot.

Three ops make a name rather than move one. `menu-on-folder` opens the menu a folder carries — over a
folder's row, or over the heading at the top of the tree when it names none, the heading being the
folder itself and the only way to make a name at the top level. `name` presses one of the two items
that open a naming box and types into it, which is one move: the box takes a row's place, and a box
nobody typed into is a name nobody asked for. `rename` is the same box over a name already on a row.
Both refusals a name comes back with are read through `says` — `taken` for a name the folder already
holds, `unnameable` for one the machine will not take at all — and both are read with the box still
open, which is where the reader is looking when either arrives.
`name-a-file-without-leaving-amenbo` is the road.

**The box has a second door, and `press` is how the road walks it.** `press` names a key by what the
face does with it, and the vocabulary is closed in the driver rather than in the registry: a key the
face has no answer for fails on the way in instead of in front of a screen. Three keys are in it.
`escape` is the panel's and takes one layer per press. `f2` and a single letter are a row's, and do
nothing at all unless the keyboard is standing on one — a letter walks to the next row whose name
begins with it, and F2 opens the naming box on wherever that left the keyboard. `rename` then types,
under `by: key` so that the line says the box rather than a menu nothing put on the screen. The two
keys are only readable together: a letter moves nothing a shot can tell from a row already stood on,
and a box says nothing about how the keyboard reached it. The road presses a letter that walks past
the row below to a further one, and the name that comes out says where it landed.

**A rename that changes only the letters' case is not readable here.** Every screen reading is folded
to one case before the shot and the expectation meet, so a row that was never renamed answers exactly
like one that was. It is the rename most worth walking — a machine that reads the two names as one is
the machine that would refuse it — and it is held in a unit test over the rename itself instead.

`drop-in` is the row coming in: one dragged from somewhere else on the machine and let go over a
folder's row. What it puts under test is the landing rather than the carrying — the drag is
caught by the application and not by the face, so the part that can be wrong is which folder was
under the pointer when the hand opened. What is brought is named and not pathed, for the reason `task
attach` names one: a drop reads the disk the operator is sitting at, and nothing a run lays down is
anywhere a hand can reach from there. `as` says whether a file or a folder is being dragged, and for
a folder `holding` names a row that has to be inside it — what a folder's drop has to answer for is
what came with it, and a row nobody was told to bring is one no reading can look for.
`bring-a-file-in-from-the-machine` and `bring-a-folder-in-from-the-machine` are the roads: two,
because a carry that made the folder and copied nothing into it passes every reading the file road
makes.

The rest are the file going the other way — out of Amenbo, to the machine. `menu` right-clicks a row
and `menu-on-file` reaches the same menu from the file that is open, which is where a file the face
refuses to draw offers a way on and no row is under the pointer any more; `hand-over` presses one of
the three items on the menu that comes up, and `handed-over` reads what the press left. Which item is
meant travels as a `door` — `usual` for the application the machine already opens that kind of file
with, `pick` for one the reader chooses for this file alone, `manager` for the file manager — rather
than as the item's own words, which are the interface's and are drawn in whatever language the run's
machine is set to. All three are the screen's alone, and the reading stops
where Amenbo does: what a hand-over ends in belongs to the machine, so the road goes no further than
the machine having taken the file. `hand-a-file-to-the-machine` is the road.

A few ops exist to put something **wrong**, because a repair cannot be shown working over a store
where there is nothing to repair — and a sweep that sweeps nothing looks exactly like one that works.
`folder legacy-pointer` leaves a bound folder's `.amenbo` in the shape an older build wrote, which is
what `store doctor-fix` puts right. `folder foreign-pointer` leaves one claimed by another store,
which is what the guard in front of every read refuses — a build stamps its own name as it writes, so
the one store that cannot leave another's pointer is the build under test. It is the run's own
pointer with a different name on it, which is the fixture worth making: every other field agrees, and
a pointer whose id and slug both check out is the one nothing but the name can turn away.
`folder lost-pointer` leaves one naming a project this store does not have, which is what a folder is
left holding once the store that answered for it goes — a channel wiped, a throwaway store dropped.
It is made the same way and for the same reason: the run's own pointer with the number moved past
anything a run hands out, so the shape and the store's name are still this build's and the number is
the whole of what leads nowhere.
`plugin stale-manifest` leaves an installed plugin recording a
build the catalog has moved past, which is what `plugin update` puts right — the catalog publishes one
build, and an asset is trusted only by the key of the catalog that served it, so there is no second
build to install first and no way to sign one into existence. Three of the `plugin declare-…` ops put a setting into what an
installed plugin says it takes: what a plugin takes is the author's word, Amenbo never invents a field,
and **no plugin in the official catalog declares one at all** — so every road through `plugin config`
would go unwalked until one does. `declare-setting` writes the plain kind, the line a reader types and
reads back; `declare-secret` writes the flag that sends a value down the other road, which fails
silently and in plain text; `declare-choice` writes a setting whose answers the author listed, and the
default that stands until someone gives one, which is what keeps a choice made, a choice declined and a
question nobody has answered apart. Any of the three also takes `translated:` — the words that field
carries in the author's other languages, keyed by language code: the `label` a form draws it under, and
for a choice the `options` its candidates are, keyed by the value each one stores. They land where an
install puts what a catalog published, beside the manifest rather than in it, which is what a form reads
and why one follows a reader changing language with nothing fetched. No published plugin declares a
setting, so none has one translated either — both halves are out of reach for the one reason, and are
written by the one door. Any of the three takes `required: true`, the flag that says the
plugin cannot work without an answer — the fail-closed enable is refused while the crossing holds none,
and no published plugin declares that either. `declare-action` is the fourth of that family and writes what the
author offers to *do* from that same form — the button, the one value it asks for at the press, and whether
that value is one the author called a credential — which
no published plugin declares either, so the operation is a face no install reaches; `plugin press-program`
stands the program behind it in, since an operation is code being run and what the form draws is one line
of what that code said. `declare-check` writes the other half of that block — the judgement an author has
raised on the values before a gate opens on them — and `plugin check-program` answers it, `ok:` being the
scenario's to choose, since what a road about a gate wants is the same values turned away and then let
through. A plugin has one program, so those two stand-ins replace whatever stood there before, each other
included — but they do not need each other: `press-program` answers a check with a yes on the stream a
press never reads, so a settings block carrying both halves is walked by standing in that one, and
`check-program` is what a road reaches for when the verdict itself is under test. `press-program` also
takes `writes:` and `writes_value:`, which leave the press storing one of the plugin's own settings back
through `plugin config set` — the door a plugin's own value arrives by, and the only one a field its
author marked `readonly` has. A road that names neither gets the program as it was, writing nothing. `plugin declare-scope` writes the layer the author
declared — one project's rows, or the device's — which is the same kind of word and unreachable for the
same reason: a manifest saying nothing means `project`, and every published plugin says nothing, so the
road a machine-wide plugin walks (one enable, one window on the whole device) exists only once this is
written. `plugin slow-program` leaves an installed plugin
taking seconds to answer, which is the only way a queue holds anything to read: a row leaves the moment
its plugin replies, so the backlog `plugin log` reports is the window a slow plugin holds open, and
every plugin the catalog publishes answers in the time a process takes to start. `plugin echo-program`
leaves one answering with the config it was handed, which is the only witness a setting's delivery has:
it travels on the child process — as an environment variable for a secret, in the stdin document for
everything else — and the published plugins use their settings rather than report them.
`plugin read-back-program` leaves one calling Amenbo back, which is the only witness the read-back
route has: an event names a record and carries none of it, so the content is fetched by running the
binary with the store and the window Amenbo handed over — and the published plugins work everything
out from the repository they are called in, asking Amenbo nothing. `plugin unbadge` takes the catalog's badge off an installed
plugin, which is the only way a road meets a stranger's: the badge is the catalog's to grant and an
author who could write it onto themselves would be the reason it is worth nothing, so every plugin
the official catalog serves arrives with it and no install reaches the state a user is in the moment
they install from anywhere else. `plugin installed-dir` shuts what
is installed away and gives it back, which is the only way a write's delivery is left standing:
delivery rides along with the write that caused it, so a push made by hand carries something only
where that drive never ran — and Amenbo skips it exactly when the installed plugins will not read.
They are the
same idea as `repo write-file`: the
state on disk a scenario has to arrive at, and cannot reach by using Amenbo, the driver makes. Reach
for one only when the line under test is what Amenbo does about that state.

One of them puts nothing wrong — it puts time. `store worn-in` leaves the store reading as one
somebody has been coming back to: the launches this device has tallied, and the days its records were
written on. Both are what Amenbo holds an unasked offer behind, and neither is reachable by doing
anything — a launch tally is raised by the app coming up, and days are days, so a run that tried to
earn this would have to last a week. It is written straight onto the store, which is the one place a
driver reaches past the binary's face: the two scalars Amenbo tallies into, and one record moved back
per day asked for (the store has to hold at least that many). A store is a plain SQLite file — no
shipped path keys one — so this crate carries `rusqlite` for that single op and for nothing else.

`fixtures/` is for what a scenario cannot hold itself. This tree's prose rule keeps a bare Amenbo
reference out of every `.yaml`, and the lint has nothing to find unless a file really carries one —
so the file carries it and the scenario names the file. Bytes that are not text at all are the other
half of the same shelf: an image a screen road has an operator choose in a picker is a picture being
drawn, so any file of the right name would prove nothing.

```yaml
id: delegate-to-ai
title: Work handed to me-ai reaches the next session's mailbox and is held by one session alone
steps_cli:
  - { type: action, domain: task, op: create, with: { title: SEED }, as: seed }
  - { type: action, domain: task, op: assign, with: { target: seed, assignee: me-ai } }
  - { type: assert, domain: task, op: listed, with: { filter: "assignee:me-ai status:todo", target: seed, present: true } }
steps_gui:
  - { type: action, domain: task, op: create, with: { title: SEED }, as: seed }
  - { type: action, domain: task, op: assign, with: { target: seed, assignee: me-ai } }
  - { type: assert, domain: task, op: listed, with: { filter: "assignee:me-ai status:todo", target: seed, present: true } }
```

The op vocabulary is a **closed registry** in `core/src/lib.rs`: an unknown op is rejected,
so a typo never runs as a no-op. Drivers grow the registry (and their own op → driver
mapping) as new ops are needed. Each op declares the args it takes as words, and the lint
checks the value arrived as one: YAML types an unquoted scalar by its shape, so a SHA of
nothing but digits parses as a number, and a driver would only meet it at the far end of a run.

**Write sample values in the shape of the real thing** — a SHA that looks like a SHA, a title a
person would type. An extreme value (an empty string, a single character, something enormous)
belongs in a scenario only when the extreme is what the line is about, and then say so on the spot:
nothing else can tell a value chosen on purpose from one chosen carelessly.

A `field` assert names its value by a dotted path into the read it is about — an object's `show
--json`, or one of the reads the store answers about itself (`store`'s `config` / `identity` /
`update`) — so what the output nests is reachable without a new op per corner:
`placement.project.name` walks two objects,
`blocked_by.0.name` indexes an array on the way, and a path that runs off the output is a mismatch
rather than an error. The `store doctor` assert reads its verdict through `ok`, and takes an `issue`
— a kind out of doctor's own list — when what is under test is a single problem appearing or going:
most of what doctor raises is a warning, and a warning leaves `ok` alone. A `store snapshot` assert
takes `absent: <text>` when what is under test is something that must **not** have left the store: what
Amenbo handed out is read as bytes — one file for a backup, the whole folder for an export — and the
word is looked for verbatim, which needs no reading of the layout around it. A `plugin config` assert
takes `secret: true` for the same kind of question one tier up: the read says the setting is a secret,
and does not hand the value over with it. It takes `state:` for the question a value cannot answer for
itself — `chosen`, `none` or `unanswered` — since a choice answered with none of its candidates and one
nobody has answered both hold no chosen value, and only the second follows the author's default.
And it takes `holds:` on a screen road for the plainest reading of all — a typed line standing in the box a
form draws it in, which is what a road asks after something that could have taken it away. That one is the
screen's alone, `equals` being how the same value is read where there is no box.
Both it and the `plugin config-set` beside it take `project:`, the crossing the value is held at — a
setting belongs to one, and a terminal says which by standing in a folder bound to it, so the driver
stands in that project's folder before it types. Naming none is the folder the run itself works from,
which answers to the project Amenbo raised for it; a premise that leaves it out while the road reads
the value at a crossing of its own writes somewhere neither road then looks. That word is the
terminal's alone — on screen the row the settings are opened inside has already answered it, and the
GUI harness turns it away — and it travels as a word rather than as a binding, so a test over the
scenario set holds it to a project something raised, the way the one that opens a project is held.
A `dimension listed` assert takes `side: task` / `decision` for a question its plain form cannot put:
not whether the axis is defined but whether that side is offered it at all, which an axis narrowed with
`dimension applies-to` stops being while staying on every listing. It takes `target:`
beside it for the face with no listing to read the offer off — a screen reads the offer as the control a
record's own pane keeps per axis, so the road names the record whose pane is opened.
Its neighbour `filter-refused` asks the other thing a filter can come back with: not who is in the
listing but that there is no listing, the question having been turned away — which is what a `dim:`
written on the side its axis does not classify meets. It is an assert of its own rather than a
`refused:` on `listed`, since `refused:` is an action's word and an empty page and a refusal are
exactly the two answers this separates; `code:` names the error code the refusal has to carry.
A `listed` assert asks whether the task is in the listing; give it
`position: first` / `last` instead of `present:` when what is under test is the order the store
keeps, which is the only place a reorder is visible. Its neighbour `narrowed` asks the question a
screen puts instead: which of the cards drawn a moment ago are drawn still. It names no filter because
there is none to name — the narrowing is the screen's own, and what did it belongs to the move in
front of the assert. That is either of the two narrowings a board has, and one assert answers for
both: `narrow` types words over the columns, which travel as words and are matched over the whole
record including faces a card does not show, and `open-filters` / `choose-filter` / `close-filters`
walk the values on its axes — each press adding to the set that axis is narrowed to, each pair
written as the CLI writes it, since the chips carrying them are in the reader's own language.
`filters-folded` reads what the fold leaves: the values off the screen, and the count of narrowing
axes on the control they folded into.

The decisions tab has that same panel over its own list — and a box of its own beside it — and its own
entries for both: `decision narrow` / `open-filters` / `choose-filter` / `close-filters` /
`filters-folded` / `narrowed`. They are
separate from the board's rather than shared with them because a road has to say which of the two
tabs it is standing on: a step that named neither could be walked on either and would prove whichever
the operator happened to be looking at. On the decision side `narrowed` is the screen's answer to what
`listed` asks the terminal — the terminal carries the whole narrowing as one `--filter` line, where
the screen composes it press by press and reads what is left.

### `given:` — the world a road starts from

Some roads stand on records the road itself never makes: a plugin already installed, a catalog
already registered, a project that is simply there. Left unwritten, that is a precondition living in
whoever prepared the screen last — an operator reading the file cannot tell what to put in place,
and no driver can put it there for them.

`given:` is where it is written. It carries **actions**, in the same shape a road's steps have, and
what it names with `as:` is in scope on **both** roads — the world belongs to the scenario, not to
one of its roads:

```yaml
given:
  - { type: action, domain: plugin, op: install, with: { name: worktree } }
  - { type: action, domain: task, op: create, with: { title: SEED }, as: seed }
steps_gui:
  - { type: assert, domain: task, op: listed, with: { filter: "status:todo", target: seed, present: true } }
```

The driver stands it up before it walks, and the line it may not cross is the screen's own moves: an
op is allowed here only if it is one a driver can reach without the screen (a second closed list in
`core/src/lib.rs`, cut out of the registry). Opening a card, answering the question it puts, typing
words over a listing — those are what a screen road is watching, and a premise that carried them out
would leave the road proving something already done. For the same reason a premise takes no
`assert` (nothing is being proved yet) and no `refused:` (a refusal leaves nothing standing).

**A screen road needs one even where the records look like its own to make.** A board with nothing on
it draws the first loop in place of its columns, and the way to add a card is in a column head — so a
road that opens by filing a task has no way to make that move, and an untouched store has no project
to open a board for in the first place. The premise is what puts the columns there: the work the road
takes for granted, or — where filing is what the road is about — one piece of ordinary work that was
already standing on that board.

**A road that opens a project names one that is there**: a project the world raised, or `cwd` — the
project a run already stands in, which Amenbo raises for the folder the run works in and calls after
it. The name travels as a word rather than as a binding, so nothing in the lint or the render would
catch a road sending an operator to hunt a list for a project nobody made; a test over the scenario
set holds it instead. A premise carrying `store nothing-raised` leaves none of them standing, `cwd`
included, so after it a road names only what the premise raised again.

A board the road raised itself is that same empty board, and no earlier step can fill it: a project a
road makes is one the premise never saw. So a second project a road only reads across — the neighbour
a narrowing has to have something to leave out — is stood up in the premise whole, with the records
in it, and the road is left holding the narrowing alone.

The same goes for anything a screen road hands to Amenbo from outside it. Hanging a file is a move on
a record and not a way of making one, so the bytes have to be lying in the run's folder before the
step that attaches them: `repo write-file` is how they get there, and a road that asks for a file
nobody wrote stalls on the operator having nothing to pick.

A premise is not a road: a file carrying a world and no steps is refused like any other file nothing
walks.

The list holds two premises that are not records at all, and both stand up the passage of time —
the one world a road can only be given, every other premise being something somebody could have done
a moment before the run. `store worn-in` is how much Amenbo has been used on this device, which they
could only have done over days; `tick deferred` is a day having passed — or not — since the band was
put off, which no run can wait out, the band being judged once at launch.

`store nothing-raised` is on it for a third reason again: what it stands up is an **absence**. The
driver raises a project as it boots, because a store has to have somewhere to file what a premise
stands up — so the moment a road declares a world at all, there is a project on the screen it was
written without, and a road that opens on the machine a first-time reader meets could not be given
one. This takes the store back to none. It goes **first** in a premise, since losing a project
releases every folder still pointing at it: a pointer laid on the disk beforehand goes with the
projects, and what such a road wants lying in a folder is written by the steps after it.

The folder the run itself works in is one of the ones released, and the two premises that leave a
pointer no build would write (`folder foreign-pointer`, `folder lost-pointer`) copy that one. So the
driver takes its text down as it opens, before any premise walks — which is what lets a road stand
both worlds at once, and those two are exactly the ops a road on an empty device reaches for.

`folder foreign-pointer` is on the list for the neighbouring reason: not that nobody could have done
it a moment before, but that nobody under test could have done it at all. A build stamps its own name
as it writes a pointer, so the one store that cannot leave another's is the one being driven, and on
screen there is not even a command to try it with. Both roads that read the claim — the terminal's
refusal, the row the screen lists the folder as — therefore open on it.

`folder lost-pointer` stands beside it, one step further out: a build writes the number of a project
it has, so a folder naming one nothing answers for is a folder some other store wrote in and then
went away. There is no move on any face that leaves one behind, so a road that meets one opens on it.

`terminal can-start` is the one premise that stands up **the machine** rather than anything Amenbo
holds. What a frame offers to open a pane with is every agent the build could find, and it finds them
by running the pane's own login shell over the `PATH` that shell reads — so a road left to itself is
read against whatever the operator installed, which is a different row on every machine. The premise
puts a program per agent in a directory of the session's own, and the GUI harness hands that directory
to the app it launches in front of the `PATH` and to nothing else; it goes when the session does, and
nothing is installed anywhere. **The count is a floor and never a ceiling** — nothing handed to a
process can take an install off the operator's machine, and the profile that shell reads is theirs —
so what a road may ask for is a row with *more* than one thing on it, which is the one shape worth
standing up: it is where the first run has a question to put. A profile that rebuilds the `PATH` from
scratch instead of adding to it drops the directory, and that shows as the road failing to find the
row it stood up rather than as a quiet pass.

**A premise that does not stand ends that scenario, red, on the line that failed, and the road is not
walked at all.** Judging a road against a world half built says nothing about the road — every line it
then wrote, passing or failing, would be about the wrong thing. It is that scenario's failure and not
the run's, so a set keeps going and the report names the premise as what broke (its lines are numbered
in their own sequence, so a premise and a step never both call themselves the first).

### `refused:` — the step that is right to fail

Some of what Amenbo promises is a **refusal**: a reserve of a task another session holds comes back
`already_reserved`, one whose premises are unmet comes back `not_ready`, and a write outside an AI's
reach comes back `out_of_reach`. A driver that reads every non-zero exit as its own failure cannot
write that line down at all — so an action names the code it expects to be turned away with:

```yaml
- { type: action, domain: task, op: status, with: { target: held, status: in_progress, refused: already_reserved } }
```

The op and its args are the ordinary ones; what is under test is the guard standing in front of
them. The step is then judged like an assert: refused with that code passes, **going through
fails** — that is the regression it exists to catch — and being refused for some *other* reason
fails too, since a different guard is not the one the line is about. A refused operation produces
nothing, so it takes no `as:`.

A screen has no exit status to compare, so on a `steps_gui` road the word changes the **instruction**: it
tells the operator that being turned away is the step going right rather than their own hand going wrong,
and the shot they leave is the screen carrying the refusal. Which guard refused is then read by the assert
after it — a screen offers a sentence, never a code.

### `window:` — which screen a step is walked on

A step on a `steps_gui` road may name the window it happens in, by the title drawn in that window's
bar:

```yaml
steps_gui:
  - { type: assert, domain: task, op: carded, with: { target: seed, present: true }, window: "Amenbo — Terminal" }
```

A whole title wins first, and any title holding what was written answers when none does — the same
rule a name reaches an element by, and needed for the same reason: one window's title is often the
start of another's, so `"Amenbo"` has to be able to mean the shorter of `"Amenbo"` and
`"Amenbo — Terminal"`.

**Say nothing and the step means the app's one window.** That is the honest default while an app
draws one, and it stops being one the moment it draws two: the tool then refuses the step and lists
the titles that are up, rather than shooting whichever window was in front. The refusal is the point.
A shot of the wrong window is a picture of a screen nobody stood at, and on the manifest it reads
exactly like a picture of the right one — red for a reason nobody can see, or green off a name both
windows happened to carry.

A panel the app puts up — the one a file is chosen in — is a second window while it is up, and is
named the same way (`window: "Open"`). A step written under one without saying so is refused now,
where before it was answered off the window behind the panel: the tool did not count a panel as a
window at all, so what a road read was a screen it was not standing at and what it pressed was
swallowed, both without a word.

**What it names is where the step is carried out, not where the shot is taken.** The two are the
same window everywhere but one: `terminal fold-back` is pressed in the window that the press closes,
so the shot after it is of the window left standing — the app's one window, which a road says by
saying nothing. The harness works that out from the op rather than reading it off the road, for the
reason it works out a restart from `store run-again`: it is not a thing a road chooses. A road that
had to name the shot's window as well would be writing down twice what the op already says, and the
second name would have to be an absence.

Where the word is written down is where it belongs: on `given` it would name a window nothing has
drawn yet, and on `steps_cli` one that never exists, so the lint turns both away rather than reading
past a road filed under the wrong key.

### `steps_cli` / `steps_gui` — one goal, a road apiece

**Having steps is what says a driver runs this line.** There is nothing else to declare, so nothing
that can disagree with the steps: the CLI runner skips a scenario with no `steps_cli` road, and the
GUI harness refuses one with no `steps_gui` road rather than spending an eye on `Review` steps
nobody sent there. A file with neither is refused by the lint — a scenario nothing walks rots while
the set around it reports green.

Write the road the driver actually travels. A screen's road is a different shape, not a rendering
of the CLI's: linking a folder to a project is one command to type, and on screen it is a card to
open, a project to answer for and a folder to pick. Written once for both, one of the two comes out
bent. Where the two roads really are the same ground, write it in both — the duplication is small
next to a file where the screen's half quietly went stale.

Some ops exist for one road only, and the registry carries them all the same: opening a card and
answering which project it asks for are moves a screen's road is made of and a terminal's has none
of, so they are written in `steps_gui` and the CLI driver maps neither. A driver maps the ops it
meets; one it does not meet is not its to map. Cutting that board along another axis (`project
group-by`) is the same kind, and so is reading one of the columns back (`project column`): what a
terminal answers with is a listing, and a listing has no columns
to recut — the axis is a word in the filter there, not a way the answer is laid out. Answering the question an opened project puts, choosing
which tool the text is for, pressing the button that hands it over, and dropping the answer again
(`repo ai-launch-consent` / `ai-launch-pick` / `ai-launch-copy` / `ai-launch-consent-clear`), are the
same kind: a terminal asks inline and prints the text where it stands, so there is nothing there to
answer, to choose between, or to press — and the answer it writes it never reads back, so it has no
face to clear it from either. The same three moves on the project's own settings, where the way to the
text stays open after the wiring lands (`repo ai-launch-request` / `ai-launch-request-pick` /
`ai-launch-request-copy`), are ops apart from those rather than the same ones read elsewhere: what
those name is the report, and where these are walked there is none — so an instruction sending the
operator to it would be one nobody could carry out. Typing words over a listing already drawn and reading which cards they
left (`task narrow` / `task narrowed`) is the same kind: a terminal has no listing standing in front of
it, so asking a word where it is written is one command there and there is nothing to narrow. Pressing a
hit through to the record it points at, and reading which record that opened (`task open-hit` /
`task opened`), is the same kind again: a terminal prints its hits as text, and the ref is typed into
`show` rather than pressed. Reading the cross-cutting search's narrowing box as one that cannot be typed
into at all (`task narrowing-shut`), the button a consent is taken back with as one that cannot be
pressed (`repo ai-launch-consent-clear-shut`), and the button an app's row hands its road over with while
nothing is ticked on it (`repo mcp-road-shut`), are the last of them: a flag either arrives on a command
line or it does not, so only a screen holds a control a reader can see, reach and not use — which is the
shape the box takes while no side is chosen and there is no grammar to read the words in, the shape the
button takes while there is no answer left to clear, and the shape either road's button takes while the
row has no folder to hand over. All three are read off how the control is drawn, not off what it does
when used: one shut in the build and painted like a live one is one a reader still reaches for.
Whether a task's face draws one of its own controls at all (`task offers`) is the other side of those
three — not a control shut but a control gone, which is what a task still being created is read for:
its status stands as plain text, its card takes no drag, and what is left to press is the two ways out
of a creation. A terminal has none of this: what it offers is commands, and the one it would turn away
is already `task status`'s `refused:`.

Pressing a smart view open from the sidebar, reading the warning its row carries before that press, and
reading which tasks the listing behind it holds (`task open-view` / `task view-warns` / `task
view-lists`), are one road's for a reason of their own: what they are about is being told without asking.
A terminal is only ever asked. It answers the same question about days when a reader types for it, which
is what `task status-bucket` reads — but there is no row standing in front of it to carry a colour, and
nothing to press through to a listing it has already printed.

Opening the fold that offers the other way in, and reading an app's row behind it (`repo mcp-open` /
`mcp-app` / `mcp-road` / `mcp-road-shut`), are one road's for a reason of their own: what the fold is for
is the reader whose AI cannot open a folder at all, and a terminal standing in that folder is the reader
who never needed it.
There is nothing folded there, no list of apps, and no row for one to be read off — the two faces that
draw them are the screen's, and a terminal that grew one would be a road of its own rather than this one
written out again.

`repo mcp-in-app` walks off that screen and is one road's for a further reason: what it reads is another
program, opened by hand with the file the fold just handed over. It is written for the one app whose
settings Amenbo writes itself, with nobody in between — everywhere else a reader's own AI does the edit,
and what Amenbo owns there is the wording, which is held up without leaving this workspace. The step is a
`Review` whichever way it goes, since the shot a run takes is of the build under test and the window that
settles this one is not; the instruction asks the attending AI for that picture instead. It is walked once
a release, and going further — the app's own AI making a task through the server — is a round trip walked
once and not per release, what is on the far side of it being somebody else's product.

Reading what a plugin's own check said about its settings (`plugin checked`) is one road's for a reason of
its own: the verdict's sentences are the author's and are drawn on the settings form, while a terminal
meets the same refusal as an error code on the enable it turned away — which is what the `refused:` on that
step already reads.

Pressing an operation a plugin's author put on its settings form, answering what that press asks for, and
reading the line it left, the box it asked in and the button before the gate was open (`plugin press` /
`press-answer` / `press-said` / `press-asks` / `press-shut`), are one road's for a reason of their own: a
terminal reaches the same author's code through `plugin run`, which names the call itself, takes whatever
arguments are typed after it, and answers with a return value — so what is under test here, a press
choosing among the calls a manifest declared and asking for what that one needs, has no terminal to walk.

Changing the language the interface is read in, and reading the words a plugin's own text reaches a
reader as (`store set-language` / `plugin line` / `plugin asks`), are one road's for a different reason:
the setting is reachable from a terminal, but nothing a terminal prints is drawn in it. What the CLI
answers is English whatever the setting says, so a road that changed it there would be moving a value
nothing it could then read depends on — and the sentences this is about are drawn in one place. `line`
is the one under a market row's name, `asks` the one a settings form draws a field, or one of a choice's
answers, under.

Giving a project the image it shows for itself, taking that image away, and reading which of the two
the square holds (`project set-icon` / `clear-icon` / `icon`), are one road's for the plainest reason
of all: `project update` is the terminal's whole door onto these fields and it takes a name, a note, a
colour and a view. There is nowhere on it to hand over an image, so a road written for the CLI would
be naming a command that does not exist. The reading is a `Review` on both of its states — what is on
the shot is a picture, and neither the image nor the colour-and-letter a project falls back to puts a
word there. `file:` here is not `attach`'s: what is under test is the picture, so the road names a
file a premise copied off the fixtures shelf rather than one the operator brings, and the run says
where it landed before the first step is handed over. The tab that project carries down the edge of
the terminal face draws the same image (`terminal tab-icon`), and is a second op rather than an
argument on the first: it is another surface and not another way of asking, the square being the face
an image is given on and the tabs being what it was given one for. That reading is a `Review` too, and
names the project rather than pressing its tab — every project has one, and going to it would move the
whole face onto the one tab being read.

Giving each of the two faces a store writes as an image of its own, clearing one again, reading which
of the two a slot holds, and the way back onto the screen all of that is done on (`store set-avatar` /
`clear-avatar` / `avatar` / `open-settings`), are one road's for a reason a shade narrower. The setting
is reachable from a terminal — `config set human_avatar` takes the display version as a data URL — but
the original that version was baked from arrives only where a picker handed the bytes over, so a road
written for the CLI would walk past the half this one exists for. `facet:` is `human` or `ai`, the two
words the store files them under, and the line names the face rather than the display name beside it:
only one of those two names is the same in every language. Neither move waits for a button, this row
writing the store as the picker closes, and the reading is a `Review` on both of its states — an image
and the pattern drawn for a face that has none are both pictures. `open-settings` is a step of its own
because the road walks it twice: a slot redrawn under the operator's eye says the screen heard, and
only coming back to it says the store did.

One road's alone is sometimes an *argument* rather than a whole op, and then it is the driver that
cannot answer it which says so. A search hit is the case: what the row **calls** the place it points
at (`landed_on`) and the run of characters it **marks** inside its excerpt (`marked`) are drawn, not
reported — down the CLI's pipe the first arrives as a `kind` and a comment ref for the reader to put
together, and the second as a pair of offsets. Both are declared on `found` like any other argument,
the GUI harness renders them into the line an eye closes, and the CLI driver refuses a step naming
either instead of passing over it. Silence would be worse than a red: a step that asked and was never
answered reads exactly like one that was.

**A number is a query no scenario can write.** The store issues it, so a road knows which record it
means and never what that record was numbered — which is why the three steps that put something into a
search box (`task found` / `decision found`, `task narrow`, `task open-hit`) take `number_of:` naming a
binding in place of `words:`, and exactly one of the two. `spelled:` is the shape it goes in as, `bare`
(`12`) or `hash` (`#12`), both of which the box reads. The two roads fill it in differently and for the
same reason the rest of this section names: the CLI driver knows what the run created and substitutes
the number, while the GUI harness renders its lines from the YAML alone — before any world stands up —
so it names the record instead and the operator reads the number off the screen. Which box was typed
into is what settles the side a number with no type code is read as: `task narrow` types over the
columns and answers with a task, `decision narrow` types over the rows on the other tab and answers
with a decision.

What such a query is answered with is a record put at the **top** of the answer, ahead of whatever the
words matched, so `first: true` on `found` asks for that place rather than for a place anywhere in it.
The CLI driver reads the order off the hits; the GUI harness leaves it as a `Review`, a reading giving
back which words are on a shot and never which line they were on.

The words below that top row are reached the same way round. `mentions:` on `task update` and `task
comment` names a record whose number is written into the text the step writes, after the words it
wrote — the only way a road can put one record's number inside another's, since the store issues it.
That is what lets one answer hold both halves: the record the number names at the top, and the record
that merely wrote it down underneath.

An argument can also mean two different things on the two roads, and `attach`'s `file:` is the one
that does. To the CLI driver it is a path in the run's own folder, put there by a `repo write-file` a
step earlier; on screen it is a **name** and nothing more — both ways in there (the picker and the
drop) read the disk the operator is sitting at, and nothing a run lays down is anywhere either of them
is pointed, so the instruction asks for a file of that name and the operator brings one. Nothing is
given up by that: what a search reaches of an attachment is what it is called, never its bytes. A
`url:` has no way in on that face at all, so the GUI harness refuses one rather than writing a line
about a face the app does not have.

Bindings belong to the road they are made on. A `target:` in `steps_gui` resolves against
`steps_gui` alone, because the two lists are never walked in one run.

Which roads a file carries is a cost question. A CLI line costs a process and an exit code, so that
set aims wide; a GUI line costs a screenshot, an OCR pass and a human reading the `field` asserts
OCR cannot judge, so that set stays a chosen few.

## Lint

The loader checks both layers — the YAML parses into the typed model (misspelled keys are
caught), and the semantic pass (known ops, required args, each arg of the type its op takes,
every reference resolving to an earlier `as:` on the same road). Run it over the whole scenario
set:

```sh
cd verification && cargo run -p amenbo-scenario --bin lint
# or against specific files:
cargo run -p amenbo-scenario --bin lint -- scenarios/delegate-to-ai.yaml
```

Non-zero exit on any parse or validation failure, so it drops into a make target or CI.

One check in it reads the disk: a `copy-fixture` naming a file that is not on the fixtures shelf is
a lint failure, because nothing else looks — `cargo test` and `--print` both read a road without
ever fetching what it copies. The shelf it looks on is the one the run behind it will copy from, so
a harness given `--fixtures <dir>` lints against that dir rather than against the tree it was
compiled in; otherwise a run in the VM is turned away, over a shelf on a machine it is not standing
on, before it starts.

One reads across a road rather than at one step of it. A Markdown file opens as what its text says
and a rendering has no editor in it, so a road that opens a `.md` and then types, pastes or saves
without putting the text up first (`show-as`, `form: source`) leaves its operator looking for a caret
in a heading. It reads perfectly and cannot be walked, which is the one kind of fault nothing else
here catches until somebody is standing in front of the screen.

The crate's tests assert that every real scenario lints and every invalid fixture is
rejected:

```sh
cd verification && cargo test
```
