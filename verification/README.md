# Pre-distribution verification

This subsystem verifies the **shipped / installed** amenbo build as a black box, before a
release goes out. It is deliberately separate from `make test`, which exercises the
build-time workspace artifacts — see decision `AMB-D-345`.

The single source of truth is the set of **scenarios**: declarative YAML describing a
domain procedure plus its expected results, with no command line or coordinates baked in.
Every driver reads the same scenario and maps it to its own world.

```
verification/
  scenarios/   the single source of truth (YAML). Every driver reads these.
  core/        the scenario schema + validating loader (crate `amenbo-scenario`, `lint` bin)
  cli/         CLI driver — drives the shipped binary, asserts via --json   (crate `amenbo-verify-cli`)
  gui/         mac harness — scenario → screen instructions + screencapture (later task)
```

`core/` and `cli/` are members of this cargo workspace, outside the main workspace, so they
are never pulled into `make test`.

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

The run is isolated by `AMENBO_HOME` pointed at a throwaway store plus a `.amenbo`-free CWD
(`AMB-D-336`); the real app-data is never touched, and `AMENBO_UPDATE_CHECK=0` keeps it off the
network. Exit code is the machine signal — `0` when every assert passes, non-zero on any failed
assert or execution error — so a multi-scenario runner (a later task) reads it directly. `--keep`
leaves the throwaway store in place for inspection.

Each op the driver maps is a `(domain, op)` arm in `cli/src/main.rs`; an op that is in the
scenario registry but not yet mapped fails loudly rather than passing silently.

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
