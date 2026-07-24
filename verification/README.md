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
  core/        the scenario schema + validating loader (crate `amenbo-scenario`, `lint` + `emit` bins)
  cli/         CLI driver + runner — drive the shipped binary, assert via --json (crate `amenbo-verify-cli`)
  gui/         mac harness — scenario → screen checklist + screencapture + Vision OCR verdict (crate `amenbo-verify-gui`)
```

`core/`, `cli/` and `gui/` are members of this cargo workspace, outside the main workspace, so
they are never pulled into `make test`. Nothing in CI builds or tests them either (it only
license/audit-scans their lockfile), so `cd verification && cargo clippy --all-targets && cargo
test` is the gate — run it before you land a change here.

## CLI driver

`verify-cli` reads one scenario, maps each step to an invocation of the **shipped / installed**
`amenbo` binary, and judges the asserts from that binary's `--json` output — a black box, so it
knows the domain vocabulary, not the build under test.

```sh
cd verification
# drive a specific binary (e.g. the CLI extracted from a release .pkg):
cargo run -p amenbo-verify-cli --bin verify-cli -- scenarios/task-appears-on-board.yaml --bin /path/to/amenbo
# or the `amenbo` on PATH (the installed CLI), with a machine-readable result:
cargo run -p amenbo-verify-cli --bin verify-cli -- scenarios/task-appears-on-board.yaml --json
```

The run is isolated by `AMENBO_HOME` pointed at a throwaway store plus a `.amenbo`-free CWD;
the real app-data is never touched, and `AMENBO_UPDATE_CHECK=0` keeps it off the
network. Exit code is the machine signal — `0` when every assert passes, non-zero on any failed
assert or execution error — so the runner reads it directly. `--keep` leaves the throwaway store
in place for inspection.

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
cargo run -p amenbo-verify-cli --bin verify-all -- scenarios/one.yaml scenarios/two.yaml --json
```

The exit code is the roll-up — `0` when every scenario is green, non-zero when any is red or
errored — so a release gate reads it directly. The `--json` aggregate carries `total` / `passed`
/ `failed` / `green` plus each scenario's own report (or its error).

## GUI harness (mac)

`verify-gui` reads the same scenario as a **screen checklist**. It bakes in no command line and
no pixel: each step becomes a plain-language instruction of what to do or confirm on
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
which scenario drives it (default `scenarios/task-appears-on-board.yaml`).

```sh
cd verification
# the crate's JSON face over the validated model — a shell consumer reads it through jq,
# never by reparsing YAML:
cargo run -p amenbo-scenario --bin emit -- scenarios/task-appears-on-board.yaml
```

```sh
cd verification
ID=2147   # the task whose own dev GUI you built and opened
# front the dev GUI, resolve its window via uiauto (by pid), and shoot one shot per step:
cargo run -p amenbo-verify-gui --bin verify-gui -- scenarios/task-appears-on-board.yaml \
  --app "amenbo (dev $ID)" --pid "$(devtool devgui pid "$ID")"
```

`--pid` comes from `devtool devgui pid` and not from `pgrep`: the production app, the shared dev
app and every per-task instance run under the one process name `amenbo-app`, so a name is not
something `pgrep` can separate them by — only the bundle they were executed out of is, which is
what devtool matches (`devtool/README.md`). Inside a task worktree the id can be dropped entirely:
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

A scenario is an `id`, a human `title`, an optional `description`, and an ordered list of
`steps`. Each step is an `action` (changes state) or an `assert` (an expected result),
names the `domain` it touches (`task` / `decision` / `comment` / `project`) and an `op`,
and carries named args under `with`. An action may bind its result with `as:`, and a later
step refers back to it with `target:`.

```yaml
id: task-appears-on-board
title: A task assigned to me-ai surfaces in the me-ai todo listing
steps:
  - { type: action, domain: task, op: create, with: { title: SEED }, as: seed }
  - { type: action, domain: task, op: assign, with: { target: seed, assignee: me-ai } }
  - { type: assert, domain: task, op: listed, with: { filter: "assignee:me-ai status:todo", target: seed, present: true } }
```

The op vocabulary is a **closed registry** in `core/src/lib.rs`: an unknown op is rejected,
so a typo never runs as a no-op. Drivers grow the registry (and their own op → driver
mapping) as new ops are needed.

## Lint

The loader checks both layers — the YAML parses into the typed model (misspelled keys are
caught), and the semantic pass (known ops, required args, every `target:` resolving to an
earlier `as:`). Run it over the whole scenario set:

```sh
cd verification && cargo run -p amenbo-scenario --bin lint
# or against specific files:
cargo run -p amenbo-scenario --bin lint -- scenarios/task-appears-on-board.yaml
```

Non-zero exit on any parse or validation failure, so it drops into a make target or CI.
The crate's tests assert that every real scenario lints and every invalid fixture is
rejected:

```sh
cd verification && cargo test
```
