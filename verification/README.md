# Pre-distribution verification

This subsystem verifies the **shipped / installed** amenbo build as a black box, before a
release goes out. It is deliberately separate from `make test`, which exercises the
build-time workspace artifacts.

The single source of truth is the set of **scenarios**: declarative YAML describing a
domain procedure plus its expected results, with no command line or coordinates baked in.
Every driver reads the same scenario and maps it to its own world.

```
verification/
  scenarios/   the single source of truth (YAML). Every driver reads these.
  fixtures/    text a scenario cannot hold itself (a file carrying an amenbo ref for the lint)
  core/        the scenario schema + validating loader (crate `amenbo-scenario`, `lint` + `emit` bins)
  cli/         CLI driver + runner + coverage count — drive the shipped binary, assert via --json (crate `amenbo-verify-cli`)
  gui/         mac harness — scenario → screen checklist + screencapture + Vision OCR verdict (crate `amenbo-verify-gui`)
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
workspace black-box-drives the shipped binary, so it reads process env raw (`AMENBO_BIN`,
`AMENBO_GUI_CAPTURE_BIN`) rather than through `amenbo_core::env`, which it has no dependency on.
A plain `cargo clippy --all-targets` in this directory fails on those lines and on nothing else.

## One scenario file per capability

The scenario set is not a pile that grows by one file per feature. **Its size is pinned to the
capability list `amenbo agent --json` prints**: one file per capability, holding every line that
capability owns. Then the count only moves when amenbo's own capability list moves, and a changed
behaviour turns exactly one file red.

- **The file is named after the capability's first command**, spaces to dashes — the capability
  "Assign a task to a person or that person's AI" leads with `task assign`, so its file is
  `task-assign.yaml`. The scenario's `id` is the file's stem. The prose of a capability gets
  reworded; the command it leads with is the stable handle, and it is the handle a coverage count
  matches on.
- **A line belongs to the capability whose command it exists to prove** — the operation under test,
  not the read it is checked with. Every line ends by reading something back, so `task field` and
  `task listed` show up all over the set; they are the assert vocabulary, not the owner. A line that
  reserves a task and reads its status back belongs to `task-status.yaml`.

Adding a line to the set:

1. Find the capability it proves, and open that file.
2. **Write the steps into it.** Do not add a file — a second file for a capability that has one is
   the pile coming back.
3. Only when no file answers for it — the capability itself is new — start one.
4. When a feature goes, its file goes with it.

What is covered and what is not is counted, not eyeballed — `verify-coverage` (below) reads the
capability list out of the shipped binary and names every capability with no file to its name.

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
cargo run -p amenbo-verify-cli --bin verify-cli -- scenarios/task-assign.yaml --bin /path/to/amenbo
# or the `amenbo` on PATH (the installed CLI), with a machine-readable result:
cargo run -p amenbo-verify-cli --bin verify-cli -- scenarios/task-assign.yaml --json
```

`--bin` (and `$AMENBO_BIN`) takes a relative path as well — `--bin ../target/debug/amenbo` to point
at your own build — read from where you run the command, not from the throwaway directory the
scenario is driven in. A value with no separator in it (`--bin amenbo`) stays a `PATH` lookup.

The run is isolated by `AMENBO_HOME` pointed at a throwaway store plus a `.amenbo`-free CWD;
the real app-data is never touched, and `AMENBO_UPDATE_CHECK=0` keeps it off the
network. Exit code is the machine signal — `0`
when every assert passes, non-zero on any failed assert or execution error — so the runner reads it
directly. `--keep` leaves the throwaway store in place for inspection.

**One thing does leave the box.** `plugin install` resolves the official catalog over the network,
picks this platform's asset and verifies its signature against the key built into the binary — a
layer that exists only in a shipped build, and one no local fixture can stand in for without the
count reading "covered" over the very thing it exists to catch. The plugin scenarios therefore need
the network and an intact catalog — the ones that install one, and the one that reads the browsing
view back — and they are the only ones that do.

The loopback is the other side of that. A third-party catalog is trusted on the signing key it
publishes beside its `catalog.json`, and a key is *served*, never written down — so a scenario that
walks the pin (`plugin catalog-stand`) has the run publish a catalog of its own on a port, and names
it by the `as:` binding rather than by a URL it could not have known. `catalog-rotate-key` serves the
other of two keys from the same address, which is a publisher rotating theirs as amenbo sees it. The
host is `amenbo-static-host`, shared with the main workspace's own tests.

Each op the driver maps is a `(domain, op)` arm in `cli/src/lib.rs`; an op that is in the
scenario registry but not yet mapped fails loudly rather than passing silently.

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
cargo run -p amenbo-verify-cli --bin verify-all -- scenarios/task-add.yaml scenarios/task-assign.yaml --json
```

A scenario that does not name `cli` in its `drivers` is **skipped**: printed as skipped, counted
apart, and left out of the verdict. If that leaves nothing to run, the exit is non-zero rather than
an empty green — a gate that verified nothing must not read as one that verified everything.

The exit code is the roll-up — `0` when every scenario that ran is green, non-zero when any is red
or errored — so a release gate reads it directly. The `--json` aggregate carries `total` / `passed`
/ `failed` / `skipped` / `green` plus each scenario's own report (or its error, or the drivers it
was written for).

## Coverage

`verify-coverage` counts the scenario set against the capabilities amenbo declares. The denominator
is not kept here: it is the `capabilities` list the **shipped binary** prints from `agent --json`, so
it grows the moment amenbo does and the count notices with nobody remembering to update it. The
numerator is the file names — one per capability, named after the command that capability leads with.

```sh
cd verification
# what a release's stock-take reads, against a specific binary:
cargo run -p amenbo-verify-cli --bin verify-coverage -- --bin /path/to/amenbo
# the same inventory as JSON, for splitting the gaps into tasks:
cargo run -p amenbo-verify-cli --bin verify-coverage -- --json
```

It reports three things: capabilities with no file (**uncovered**), files answering for no capability
(**unowned** — a leftover from a capability that went, or a name that never matched one), and files
whose `id` has drifted from their name (**misfiled** — the name is what the count matches on, the id
is what a report prints, and a file where they disagree is filed as one capability and reported as
another).

**A gap is not a failure**: the exit code is 0 whether or not the set is complete. An uncovered line
is work to file, not a reason to hold a release — a gate that blocked on it would only teach everyone
to skip the gate. Non-zero means the count could not be taken at all (the binary would not run, the
directory would not be read).

## GUI harness (mac)

`verify-gui` reads the same scenario as a **screen checklist**, and only a scenario whose
`drivers` name `gui` — it refuses the rest by name instead of shooting a line written for the
binary. It bakes in no command line and no pixel: each step becomes a plain-language instruction of what to do or confirm on
screen, the running GUI's window is located through `app/scripts/uiauto/uiauto.swift`, and every
step is captured with `screencapture -l <winid>` into an evidence directory (plus a
`manifest.json` pairing each instruction, verdict and shot).

An assert is judged from its shot with macOS's own **Vision** OCR (`gui/ocr.swift`):
the harness derives the text the step expects on screen — for a `listed` assert, the bound task's
title — reads the shot back, and passes when that text is present (or absent, for `present:
false`). The recognized text is written next to the shot (`NN-…​.txt`) as evidence of the reading.
An assert OCR cannot mechanically judge — a structured `field` value — is a `Review`: its shot is
kept for an AI/human eye and does not fail the run. tesseract stays the Linux container path
(`scripts/docker/gui-e2e.sh`); each driver maps the one scenario source to its own world.

The Linux container carries no toolchain, so it can't read the scenario itself. Its host launcher
(`make verify-gui-linux`) resolves the scenario through the `emit` bin and passes the card — the
`listed`/present title — into the container as `AMENBO_E2E_CARD`. tesseract reads the words but not
every glyph, so that path matches the title on its alphanumerics, not verbatim. `SCENARIO` selects
which scenario drives it (default `scenarios/task-assign.yaml`).

```sh
cd verification
# the crate's JSON face over the validated model — a shell consumer reads it through jq,
# never by reparsing YAML:
cargo run -p amenbo-scenario --bin emit -- scenarios/task-assign.yaml
```

```sh
cd verification
ID=2147   # the task whose own dev GUI you built and opened
# front the dev GUI, resolve its window via uiauto (by pid), and shoot one shot per step:
cargo run -p amenbo-verify-gui --bin verify-gui -- scenarios/task-assign.yaml \
  --app "amenbo (dev $ID)" --pid "$(devtool devgui pid "$ID")"
```

`--pid` comes from `devtool devgui pid` and not from `pgrep`: devtool matches on the bundle a
process was executed out of, which names one instance exactly — a dev build's own executable name
(`amenbo-app-dev`, `amenbo-app-dev-<id>`, against prod's `amenbo-app`) reaches the right app too,
but a pid is what uiauto takes (`devtool/README.md`). Inside a task worktree the id can be dropped entirely:
with no argument the command answers for the dev GUI that checkout launches.

uiauto is the input primitive, called here, never moved: `window <pid>` yields the id
`screencapture -l` needs and the window bounds (in the manifest) an operator uses to turn a shot's
pixel into a click point, and its `click` / `type` / `key` carry out the action steps the
checklist names. Bring the app to the front first (`--app`, or by hand) — uiauto skips a window
behind another Space. `--winid <id>` shoots a window directly, skipping uiauto; `--evidence <dir>`
chooses where the shots and manifest land (default: a fresh dir under the temp tree); `--ocr
<path>` overrides `ocr.swift`. Exit is 0 when every OCR-judged assert passed and every step was
captured, non-zero on a failed assert or a load/capture/OCR failure — a `Review` step is closed by
a human from the evidence, not by the exit code.

## Scenario format

A scenario is an `id`, a human `title`, an optional `description`, an optional `drivers`
list, and an ordered list of `steps`. Each step is an `action` (changes state) or an
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
hook slots are real enough to write into. All of it stays inside the run's own throwaway folder — a
path that is absolute, or that climbs out with `..`, is refused.

A few ops exist to put something **wrong**, because a repair cannot be shown working over a store
where there is nothing to repair — and a sweep that sweeps nothing looks exactly like one that works.
`folder legacy-pointer` leaves a bound folder's `.amenbo` in the shape an older build wrote, which is
what `store doctor-fix` puts right. `plugin stale-manifest` leaves an installed plugin recording a
build the catalog has moved past, which is what `plugin update` puts right — the catalog publishes one
build, and an asset is trusted only by the key of the catalog that served it, so there is no second
build to install first and no way to sign one into existence. `plugin declare-secret` puts a secret setting into what an
installed plugin says it takes: what is secret is the author's word, amenbo never invents a field, and
no plugin in the official catalog declares one — so the secret route, which fails silently and in plain
text, would otherwise go unwalked until one does. `plugin slow-program` leaves an installed plugin
taking seconds to answer, which is the only way a queue holds anything to read: a row leaves the moment
its plugin replies, so the backlog `plugin log` reports is the window a slow plugin holds open, and
every plugin the catalog publishes answers in the time a process takes to start. `plugin echo-program`
leaves one answering with the config it was handed, which is the only witness a secret's delivery has:
it travels as an environment variable on the child process, and the published plugins use their
settings rather than report them. They are the same idea as `repo write-file`: the
state on disk a scenario has to arrive at, and cannot reach by using amenbo, the driver makes. Reach
for one only when the line under test is what amenbo does about that state.

`fixtures/` is for text a scenario cannot hold itself. This tree's prose rule keeps a bare amenbo
reference out of every `.yaml`, and the lint has nothing to find unless a file really carries one —
so the file carries it and the scenario names the file.

```yaml
id: task-assign
title: A task handed to me-ai is stamped as the AI's and surfaces in the me-ai todo listing
drivers: [cli, gui]
steps:
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
and does not hand the value over with it. A `listed` assert asks
whether the task is in the listing; give it
`position: first` / `last` instead of `present:` when what is under test is the order the store
keeps, which is the only place a reorder is visible.

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

### `drivers` — which harnesses run this line

`drivers` says which harnesses a scenario is written for. **Omit it and the scenario is
CLI-only**, which is where a line belongs unless the screen is the only place it can break.
The set is one source of truth, but a driver only carries what it was given: the CLI runner
skips a scenario that does not name `cli`, and the GUI harness refuses one that does not
name `gui` rather than spending an eye on `Review` steps nobody sent there.

The asymmetry is what the field is for. A CLI line costs a process and an exit code, so that
set aims at the whole capability list; a GUI line costs a screenshot, an OCR pass and a
human reading the `field` asserts OCR cannot judge, so that set stays a chosen few. An empty
list is refused — a scenario nothing runs rots while the set around it reports green.

## Lint

The loader checks both layers — the YAML parses into the typed model (misspelled keys are
caught), and the semantic pass (known ops, required args, each arg of the type its op takes,
every reference resolving to an earlier `as:`). Run it over the whole scenario set:

```sh
cd verification && cargo run -p amenbo-scenario --bin lint
# or against specific files:
cargo run -p amenbo-scenario --bin lint -- scenarios/task-assign.yaml
```

Non-zero exit on any parse or validation failure, so it drops into a make target or CI.
The crate's tests assert that every real scenario lints and every invalid fixture is
rejected:

```sh
cd verification && cargo test
```
