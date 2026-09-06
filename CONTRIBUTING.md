# Contributing

**Issues are welcome. For pull requests, please open an issue to discuss first.**

Amenbo is maintained by one person. Bug reports, questions, and feature ideas are
genuinely wanted — open an issue. But please don't send a pull request out of the
blue: start with an issue so we can agree on the shape of the change before you
spend time on it. An unsolicited PR may be declined even if the code is good, simply
because it doesn't fit where the project is going. Discussing first saves that.

## Reporting a bug or asking for a feature

Open an issue. There are templates for a bug report and a feature request — filling
one in gives the maintainer what they need to act without a round-trip.

## Code of conduct

Taking part here — an issue, a pull request, a discussion — means holding to the
[Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Don't file a security problem as a public issue. See [SECURITY.md](SECURITY.md) for
how to report a vulnerability privately.

## Building

The local gate that mirrors CI is `make test`; run it before proposing a change.
`make gate` is the same gate narrowed to the layers your change touched, from the
declaration CI reads.

### Toolchain

Node and Rust versions are pinned in the repo so everyone builds with the same
toolchain (no OS-global change needed). The pin files — `.nvmrc` / `.node-version`
(Node 24, Active LTS), `rust-toolchain.toml` (Rust 1.96.0 + rustfmt/clippy), and
`.tool-versions` (both) — are read by the common per-project version managers, so
no single tool is forced.

```bash
# Recommended: mise installs Node + Rust from the pin files in one shot
mise install

# Alternative: nvm + rustup (each reads its own pin file)
nvm use            # picks up .nvmrc
rustup show        # rust-toolchain.toml is applied on the next cargo command
```

`app/package.json` declares `engines.node >= 22.12` (the build deps' floor); the
pin files select the exact version above that. There is no `package.json` at the
repository root — the JavaScript side lives entirely under `app/`.

Go appears in the tree, but it is not a third toolchain to install: it builds only
`devtool/`, the optional helper that gives a task its own throwaway dev GUI and
raises the VM that GUI is driven in (see
[devtool/README.md](devtool/README.md)). Nothing else reads it, so Amenbo builds,
tests and ships without Go — `make devtool` is the only target that asks for it.

**Building the GUI is macOS-only.** The Tauri `.app` links the native WebKit
(WKWebView), so it can't be built or run in a Linux container — Docker /
devcontainers only suit the headless side (core / CLI / tests / CI on Linux).
The `make gui` / `install-gui` targets (and their `-dev` variants) require host
macOS; the CLI builds anywhere Rust does (`make install-dev` for a local dev
build — the prod CLI ships inside the unified installer, so `make install` is
retired).

### Build and run

```bash
export AMENBO_APP_NAME=amenbo                                  # the pair below; every make
export AMENBO_LATEST_JSON_URL=http://127.0.0.1:1/latest.json   # target passes them for you
cargo build
cargo test                              # fast: unit + light integration suites
cargo test --features scale,e2e         # full: also the scaling guard and the real-binary cli_e2e_* suites
cargo run -p amenbo-cli -- agent --json   # the single source of truth for the CLI
```

The app-data name and the update endpoint are both injected into `amenbo-core` at compile time, and
neither has a default, so a bare `cargo` invocation that carries neither stops at the constant that
wants it, naming the variable. A default is what lets a forgotten variable ship as a wrong answer,
and a default that names production is that answer landing on real data. The `make` targets, the
Docker builds and CI pass the pair above: the production app-data name, because that is the channel
the suite is compiled on, and an endpoint on a loopback port nothing listens on, which nothing ever
queries because a build the release workflow did not stamp does not ask (`update_check`).

A CLI you build that way and then *run* opens the production store. For a build you mean to use,
pass `AMENBO_APP_NAME=amenbo-dev` — the dev channel, which is what `make install-dev` builds — or
point `AMENBO_HOME` at a directory of its own.

The heaviest tests are gated behind cargo features so the everyday `cargo test`
stays sub-second: the read-hotpath scaling guard behind `scale`, the real-binary
`cli_e2e_*` suites behind `e2e`. CI enables both (`--features scale,e2e`).

For the full run, [`cargo-nextest`](https://nexte.st) is faster (per-test process
isolation + parallelism) and prints per-test timings so the slow tests are easy to
spot — `make test` wraps it (and runs the doctests nextest skips). It also gates the
out-of-workspace Tauri host crate (`app/src-tauri`), the GUI front-end, and the shell
scripts that drive the release and the on-hardware checks, mirroring CI's `app-rust` +
`gui-web` + `shell` jobs, so a core change that only breaks the GUI — or a quoting slip
in a release script — is caught locally. `make gate` runs the same stages, minus the ones
your change cannot have moved: which layer a path belongs to is declared in
`.github/paths-filters.yml`, the file CI gates its own jobs on, and a path on no layer falls
back to the whole of `make test`:

```bash
cargo install cargo-nextest            # one-time (or https://get.nexte.st)
brew install shellcheck actionlint     # one-time (the shell gate; any package manager will do)
make test                              # shell gate + core/cli (scale,e2e + doctests) + app crate clippy/test + GUI typecheck/build/test
make gate                              # the same, narrowed to the layers your change touched (a path on no layer falls back to the whole of make test)
make shell-gate                        # the shell leg on its own (every tracked *.sh and git hook, plus the shell embedded in each workflow's run:)
cargo nextest run --features scale,e2e # core/cli only, without the doctest + GUI legs
```

The Tauri host crate and the verification harness each sit outside the workspace with a
lockfile of their own, but both reach core through a path dependency — so a workspace bump
moves what those locks resolve to without touching the files. CI's `out-of-workspace-locks`
job fails a pull request whose copies are behind; `make lock` re-resolves both (seconds,
nothing is compiled) and the result is yours to commit.
Dependabot's own bumps are repaired for it, ahead of the merge — see `lockstep` in
`.github/workflows/_dependabot-automerge.yml`. That repair pushes with the workflow's own
token, which starts no run, so on the bumps that do move a lock the same workflow dispatches
the full gate against the branch: the head it just made gets a verdict of its own, and the
merge it armed lands on that.

The DB layer is [rusqlite](https://docs.rs/rusqlite) and nothing else: the change feed rides
SQLite's update hook, which belongs to the connection, and reads are issued from inside the
write transaction — both are lost the moment a second library opens a connection of its own.
The schema is declared once, in the `store_engine::schema` registry, and the `CREATE TABLE`
DDL and the per-column write whitelist are derived from it, so they cannot drift apart.

A gate only ever compiles the cfg branches of the OS it runs on, so `#[cfg(target_os =
"linux")]` code (the store-watch network-FS path, for one) is invisible to a green run on a
mac. `make lint-linux` builds the tree for Linux inside the container the Linux bundles are
built in, running the same two clippy invocations as CI's `lint` + `app-rust` jobs. Cross-
compiling won't do: the Tauri gtk/glib sys crates need their system deps at build time, so
a real Linux is the only way to reach that code. It needs Docker and takes a few minutes,
which is why it is not part of `make test` — reach for it when you touch an OS-gated branch.

Plain `cargo test` still works everywhere, given the environment above; nextest is an
optional accelerator.
Thresholds and the `ci` profile live in `.config/nextest.toml`.

Data is stored under the OS-standard location (on macOS,
`~/Library/Application Support/work.amenbo.amenbo/store.sqlite`). The directory
name comes from the build-time `AMENBO_APP_NAME`, which is what keeps a dev build's
data (`work.amenbo.amenbo-dev`) off the production store — every build entrance names it,
and one that says nothing does not compile (above). `amenbo config` prints the
path this build actually opened on its first line. Set `AMENBO_HOME` to override the
location (useful for tests and explicit setups).

### The repository tree

```
crates/
  amenbo-core/        domain model, persistence, operations, queries, export
  amenbo-cli/         the `amenbo` binary (clap; human + --json output; `agent --json`)
  amenbo-scratch/     test support: the throwaway directory a test works in
  amenbo-static-host/ test support: a loopback host, for what is reached by URL
app/                  the desktop GUI (React + Vite front end; Tauri shell
                      in `app/src-tauri`, calling amenbo-core directly — no server)
assets/               the CLI demo and its script, the GUI screenshot
  brand/              the mark: the origin SVGs — the delivered drawing, and the redraw that
                      keeps a whole-pixel stroke at 32px and below — and the icons and
                      rasters `make brand` bakes from them (the only place a coordinate
                      is written)
verification/         black-box checks of an installed build, run before a release
                      (its own cargo workspace, so `make test` never pulls it in)
devtool/              optional Go helper: a throwaway dev GUI per task, the VM it is driven
                      in, and a fake outside world
guards/               one invariant apiece, asserted over the tree by `make test` and CI
scripts/              what the Makefile calls out to — build, sign, notarise, verify,
                      bake the brand set — and a few meant to be typed by hand, such as
                      watch-ci.sh, which watches one CI run and prints only what changes
  windows/            drive a real Windows desktop from here, for the roads out of the
                      app that only a physical machine can walk
```

### The comment and prose gates

Comments are audited too, against the rules declared in `esorp.yaml`: where a comment
may sit, what shape it takes, and that it is written in English. Every comment in the
tree is judged, and the tree stands at zero, so a red gate names something your change
put there. To see the same verdict before you push, install
[esorp](https://github.com/ShiroDoromoto/esorp) and run `make hooks` — without it,
committing works exactly as before.

The files that carry no code are held to the same vocabulary: the prose of every
tracked `.md`, and the values of every manifest and config file (a comment scanner
cannot see a value, so `description = "…"` answered to no one). It reads a fenced code
block as prose too, so a code span — not a fence — is where an identifier of the form
the docs describe belongs.

Source is exempt, deliberately. Its strings are half of a localized product — the GUI
dictionaries and the i18n phrasebook are meant to carry every language they cover — so
English is asked of the comments there, not the literals.

## License

By contributing, you agree that your contributions are licensed under the
[Apache License 2.0](LICENSE), the same license that covers this project.
