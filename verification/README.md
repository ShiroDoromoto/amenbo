# Pre-distribution verification

This subsystem verifies the **shipped / installed** amenbo build as a black box, before a
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
  fixtures/    text a scenario cannot hold itself (a file carrying an amenbo ref for the lint)
  core/        the scenario schema + validating loader (crate `amenbo-scenario`, `lint` + `emit` bins)
  cli/         CLI driver + runner — drive the shipped binary, assert via --json (crate `amenbo-verify-cli`)
  gui/         mac harness — scenario → screen checklist, shot and read by the screen tool (crate `amenbo-verify-gui`)
```

`core/`, `cli/` and `gui/` are members of this cargo workspace, outside the main workspace, so
they are never pulled into `make test`. CI has a job of its own for them, gated to changes under
`verification/` (`.github/workflows/ci.yml`), and this is the line it runs — run the same one before
you land a change here, so a red arrives at your terminal rather than at main:

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
  set's size or its names to amenbo's own capability list: that list stays the feature inventory
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
other of two keys from the same address, which is a publisher rotating theirs as amenbo sees it. The
host is `amenbo-static-host`, shared with the main workspace's own tests.

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
`manifest.json` pairing each instruction, verdict and shot). Which window was shot, and the id it
was shot by, stay inside the tool: a format nobody is handed is a format nobody parses.

**The run owns the app it shoots.** It launches the `.app` bundle named by `--app`, with
`AMENBO_HOME` pointed at a throwaway store of its own, holds the pid that launch answered with, and
takes both down when it ends. Two things follow, and both are the point of doing it this way.
Nothing separates a shipped build started for a run from the same shipped build the user keeps
open — one executable name, one bundle, no badge on screen, and nothing stopping both from running
at once — so a run that went looking for a process could shoot either, and the evidence it filed
would read the same. And a screen road creates projects, tasks and bindings, none of which belong
in the store the operator actually works in; a store the run makes and drops leaves nothing for
anyone to remember to tidy.

The executable inside the bundle is started directly rather than the bundle being `open`ed, since
the environment is what carries the store and `open` hands the launch to launchd with an
environment of its own. `AMENBO_HOME` is the product's own override, so the build under test is not
a different build for having been asked. The store follows this workspace's throwaway rules — one
parent under the temp tree, a name that does not lean on the pid, a sweep of what is over a day old
on the way in — and the app is started from a directory of its own, since a child inherits the
harness's, and the harness is run from this repository.

An app launched that way opens on an empty store, which is where a screen road stands today: what a
road needs standing before its first step is not declared anywhere yet, so the roads that assume a
world — a registered catalog, an installed plugin, a folder card waiting to be linked — are the ones
that still have to be prepared by hand.

or confirm on screen, and the shooting is the screen tool's (`scripts/screen.swift`) — the harness
names the app by pid and receives one file per step in an evidence directory (plus a
`manifest.json` pairing each instruction, verdict and shot). Which window was shot, and the id it
was shot by, stay inside the tool: a format nobody is handed is a format nobody parses.

An assert is judged by asking that same tool to read the shot back (macOS's own **Vision** behind
it): the harness derives the text the step expects on screen — for a `listed` assert, the bound
task's title — and passes when it is present in the reading (or absent, for `present: false`).
Both sides meet on their words rather than their glyphs — case, punctuation and the line a wrapped
card broke on are folded away, and so is the long vowel mark Vision returns where a title carries a
dash, which Unicode files under letters. That fold is the reader's habit, so the tool applies it to
what it read and hands back the unfolded reading as well; the harness folds its own expectation by
the same rule and matches. The reading as it came back is written next to the shot (`NN-…​.txt`),
which is what a person reads when a step comes out red.
An assert OCR cannot mechanically judge — a structured `field` value — is a `Review`: its shot is
kept for an AI/human eye and does not fail the run. tesseract stays the Linux container path
(`scripts/docker/gui-e2e.sh`); each driver walks the road written for it.

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
  --app ~/Applications/amenbo.app
```

`--app` is the bundle itself, not a name: what is verified before a release is the `.app` the
installer put in place, and the bundle is asked whether it is one — through the CLI it ships, which
one build produces alongside the app and one installer carries with it. The executable to start is
asked of the bundle too (`CFBundleExecutable`) rather than assumed.

A screen line sometimes needs a world the app cannot be talked into from its own interface — the
browsing view reads the badge on a row a **registered** catalog served, and a plugin's row is only
there once one is installed. On a store the run makes fresh, none of that is standing, and putting
it there is the seeding step this harness does not have yet.

The screen tool is the input primitive too, called by whoever drives the screen between steps: its
`find` / `click-named` / `click` / `dblclick` / `type` / `key` carry out the action steps the
checklist names. The run holds itself at the launch until the app is up, in front, and can be shot
at all — the proof it waits for is a shot it throws away, since an app the system has taken up is
not yet an app with a window, and a walk that started between the two would fail on its first step.
An app that never draws one inside a minute is reported as that, and one that exits on the way up is
reported the moment it does. `--evidence <dir>` chooses where
the shots and manifest land (default: a fresh dir under the temp tree); `--screen <path>` points at
a tool other than the repo's own. Exit is 0 when every OCR-judged assert passed and every step was
captured, non-zero on a failed assert or a load/capture/reading failure — a `Review` step is closed
by a human from the evidence, not by the exit code.

**Name what to press rather than aim at it.** `swift scripts/screen.swift find <pid>` lists every
element on screen with the name it answers to and where it stands, and `click-named <pid> <name>`
clicks the one of that name. The screen is a webview, so both read it through the accessibility tree
the app serves once asked.
A point worked out from a shot's pixels carries two errors instead: the shot's pixels are the window's
points times the scale of the display it was on (2 on a built-in panel, 1 on an external one), and the
screen goes on moving after the shot — opening the right pane pushes a column header down by tens of
pixels. Anything wide swallows both, which is why aiming works until it is aimed at something small:
the board's `＋` and the view tabs read as unreachable elements until the arithmetic was suspected
instead.

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

### `--step` — a scenario whose screen moves

By default the run shoots its steps back to back, which photographs one prepared screen as many
times as the scenario is long: a line whose state advances — open the card, answer which project,
pick the folder, read the linked screen — cannot be written as one scenario, only prepared for
outside it and asserted at the end.

`--step` stops the run after each step's shot and waits for a line on stdin. Between the two, the
screen belongs to whoever is driving: carry out the next step by hand, or with the screen tool's
`click-named` / `type` / `key`, and send the line when the screen is standing where the scenario
says it should. The
stop is **after** the shot, never before — the evidence of where the run stood is on disk before
anyone is invited to move on — and there is no stop after the last step, which has nothing following
it to hold the screen for. Leave the flag off and nothing changes, so an unattended run stays
unattended.

The wait is on a line and not on a clock on purpose. A run held for a fixed number of seconds shoots
whatever is up when the clock runs out, so a step that took a moment longer is filed as evidence of a
screen nobody stood on — a red nobody can tell from a real one, or worse, a green. For the same
reason end of input is a failure and not a nod: a stepped run with nothing left to hold it would walk
the rest of the scenario off one screen and report it as though it had been driven.

The moves themselves are written in the scenario, not in a note beside it. Getting from one screen
to the next is an action step like any other (`folder open-existing-card`, `folder choose-project`),
and it earns the two things a note never can: the screen it arrives at is shot, so the middle of the
road is evidence rather than something taken on trust, and the assert after it cannot be reached by a
hand tidying the screen while the run is held. The screen roads are written that way: `link-a-folder`
walks the arrival screen, the card, the picker and the board it lands on,
`say-where-a-plugin-fires` walks one plugin's row through the four states one switch leaves it in,
`put-a-plugin-to-work-from-the-project` walks the same crossing from the other face — the arrival, the
picker that draws the row, and the switch inside it —
`fill-in-what-a-plugin-cannot-fire-without` walks one row from the mark it wears through the refusal, the
settings opened inside it and the press that then goes through, and
`choose-from-what-a-plugin-offers` walks a settings form through the three answers a choice holds and
the button back to the author's default.

Everything the wait prints — the step just captured, and the prompt — goes to stderr, so `--json`
still leaves one machine-readable line on stdout. A driver that is not a person keeps its side open
through a pipe:

```sh
cd verification
mkfifo /tmp/go
cargo run -p amenbo-verify-gui --bin verify-gui -- scenarios/link-a-folder.yaml \
  --app ~/Applications/amenbo.app --step --json < /tmp/go &
exec 3>/tmp/go   # hold the writing side open — otherwise the first echo closes it, which is the end
                 # of input, and the run stops rather than carrying on to the next step
# … drive the screen to the next step (the screen tool), then release the next shot:
echo >&3
# … and when the last step has been shot, let it go:
exec 3>&-
```

## Scenario format

A scenario is an `id`, a human `title`, an optional `description`, and an ordered list of steps
under `steps_cli` and/or `steps_gui`. Each step is an `action` (changes state) or an
`assert` (an expected result), names the `domain` it touches (`task` / `decision` /
`comment` / `project` / `dimension` / `attachment` / `store` / `folder` / `repo` / `plugin`) and an
`op`, and carries named args under `with`. An action may bind its result with `as:`, and a later step
refers back to it with `target:` — an op that joins two objects names the second under its own key
(`decision link`'s `task:`), and every such key is checked back to an earlier binding, not just
`target:`.

The last four are not things filed in a store: `store` is this device's amenbo itself — its
settings, the identity it answers `whoami` with, the build in place, and the store as a whole
(`export`, `backup`, `restore`, the integrity reads) — `folder` is a directory and the project its
`.amenbo` names, `repo` is the folder the run works in as a place with files and a git history, and
`plugin` is what is installed on the machine, whose gate is open, and what the execution log kept.

Not every object is reached by a binding. A **dimension** travels as the words a person says — its
axis and value are named in `with` (`dimension: <axis name>`, `value: <value name>`), which is what
the command takes too; a bare number there would be read as a name, not an id. A **folder** travels
as a plain name too (`dir: shared`), and for a different reason: a binding is answered by where a
folder sits, so the driver is the one that places it — clear of the run's own bound CWD, which a
pointer search would otherwise walk up into. A **plugin** is named the way the catalog names it
(`name: worktree`), which is what every one of its commands takes.

`plugin run` is the one place where a step's arguments are not amenbo's. Everything after the
plugin's name belongs to the plugin, so `command:` is the word its own face takes, `task:` hands it
the id of a task an earlier step created, and `args:` carries anything else through verbatim. The
value that comes back is read by the `returned` assert, which has to **follow its call**: a command
face's return value is its own stdout and is deliberately not written to the execution log, so
nothing else can go and fetch it afterwards.

A **`store` action that writes a file** binds it through the same `as:` an object is bound by, and
what the name then holds is the file: `restore` names the archive it puts back the way any step names
an earlier result, so a mistyped name is a lint failure and not a driver hunting for a file nobody
wrote. The files land in the run's own throwaway space and go with it.

One domain is not in the store at all. **`repo`** is the folder the run works in: `write-file` puts
a file there (what an attachment ingests, what the lint is pointed at), `copy-fixture` puts one
there from `fixtures/`, and `git-init` makes the folder a git repository, which is the only way the
hook slots are real enough to write into. `wire-ai` is the same kind of stand-in one tier up: amenbo
hands over the text that starts a folder's AI on it and writes no settings file itself, so the road
past that point exists only if someone pastes — and it pastes what the build under test handed over,
into the file that build named. All of it stays inside the run's own throwaway folder — a
path that is absolute, or that climbs out with `..`, is refused.

A few ops exist to put something **wrong**, because a repair cannot be shown working over a store
where there is nothing to repair — and a sweep that sweeps nothing looks exactly like one that works.
`folder legacy-pointer` leaves a bound folder's `.amenbo` in the shape an older build wrote, which is
what `store doctor-fix` puts right. `plugin stale-manifest` leaves an installed plugin recording a
build the catalog has moved past, which is what `plugin update` puts right — the catalog publishes one
build, and an asset is trusted only by the key of the catalog that served it, so there is no second
build to install first and no way to sign one into existence. The three `plugin declare-…` ops put a setting into what an
installed plugin says it takes: what a plugin takes is the author's word, amenbo never invents a field,
and **no plugin in the official catalog declares one at all** — so every road through `plugin config`
would go unwalked until one does. `declare-setting` writes the plain kind, the line a reader types and
reads back; `declare-secret` writes the flag that sends a value down the other road, which fails
silently and in plain text; `declare-choice` writes a setting whose answers the author listed, and the
default that stands until someone gives one, which is what keeps a choice made, a choice declined and a
question nobody has answered apart. Any of the three takes `required: true`, the flag that says the
plugin cannot work without an answer — the fail-closed enable is refused while the crossing holds none,
and no published plugin declares that either. `plugin slow-program` leaves an installed plugin
taking seconds to answer, which is the only way a queue holds anything to read: a row leaves the moment
its plugin replies, so the backlog `plugin log` reports is the window a slow plugin holds open, and
every plugin the catalog publishes answers in the time a process takes to start. `plugin echo-program`
leaves one answering with the config it was handed, which is the only witness a setting's delivery has:
it travels on the child process — as an environment variable for a secret, in the stdin document for
everything else — and the published plugins use their settings rather than report them.
`plugin read-back-program` leaves one calling amenbo back, which is the only witness the read-back
route has: an event names a record and carries none of it, so the content is fetched by running the
binary with the store and the window amenbo handed over — and the published plugins work everything
out from the repository they are called in, asking amenbo nothing. `plugin installed-dir` shuts what
is installed away and gives it back, which is the only way a write's delivery is left standing:
delivery rides along with the write that caused it, so a push made by hand carries something only
where that drive never ran — and amenbo skips it exactly when the installed plugins will not read.
They are the
same idea as `repo write-file`: the
state on disk a scenario has to arrive at, and cannot reach by using amenbo, the driver makes. Reach
for one only when the line under test is what amenbo does about that state.

`fixtures/` is for text a scenario cannot hold itself. This tree's prose rule keeps a bare amenbo
reference out of every `.yaml`, and the lint has nothing to find unless a file really carries one —
so the file carries it and the scenario names the file.

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
amenbo handed out is read as bytes — one file for a backup, the whole folder for an export — and the
word is looked for verbatim, which needs no reading of the layout around it. A `plugin config` assert
takes `secret: true` for the same kind of question one tier up: the read says the setting is a secret,
and does not hand the value over with it. It takes `state:` for the question a value cannot answer for
itself — `chosen`, `none` or `unanswered` — since a choice answered with none of its candidates and one
nobody has answered both hold no chosen value, and only the second follows the author's default.
A `listed` assert asks whether the task is in the listing; give it
`position: first` / `last` instead of `present:` when what is under test is the order the store
keeps, which is the only place a reorder is visible. Its neighbour `narrowed` asks the question a
screen puts instead: which of the cards drawn a moment ago the words typed over them left standing.
It names no filter because there is none to name — the words travel as words, and they are matched
over the whole record, including faces a card does not show.

### `refused:` — the step that is right to fail

Some of what amenbo promises is a **refusal**: a reserve of a task another session holds comes back
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
meets; one it does not meet is not its to map. Answering the question an opened project puts, choosing
which tool the text is for, pressing the button that hands it over, and dropping the answer again
(`repo ai-launch-consent` / `ai-launch-pick` / `ai-launch-copy` / `ai-launch-consent-clear`), are the
same kind: a terminal asks inline and prints the text where it stands, so there is nothing there to
answer, to choose between, or to press — and the answer it writes it never reads back, so it has no
face to clear it from either. Typing words over a listing already drawn and reading which cards they
left (`task narrow` / `task narrowed`) is the same kind: a terminal has no listing standing in front of
it, so asking a word where it is written is one command there and there is nothing to narrow. Pressing a
hit through to the record it points at, and reading which record that opened (`task open-hit` /
`task opened`), is the same kind again: a terminal prints its hits as text, and the ref is typed into
`show` rather than pressed.

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
The crate's tests assert that every real scenario lints and every invalid fixture is
rejected:

```sh
cd verification && cargo test
```
