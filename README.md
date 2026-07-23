# amenbo

[![License: Apache-2.0](https://img.shields.io/github/license/ShiroDoromoto/amenbo?color=blue)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/ShiroDoromoto/amenbo/ci.yml?branch=main&label=CI)](https://github.com/ShiroDoromoto/amenbo/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/ShiroDoromoto/amenbo?label=release)](https://github.com/ShiroDoromoto/amenbo/releases)
![OS: macOS | Windows | Linux](https://img.shields.io/badge/OS-macOS%20%7C%20Windows%20%7C%20Linux-informational)

**[amenbo.work](https://amenbo.work/)** — downloads, and what this is for.

**A task & project manager where an AI and a human collaborate on one machine.**

- **Local, single-store** — your data lives on your machine in a single SQLite store, and can be exported at any time.
- **CLI-first & AI-agent-friendly** — a Rust core library does the domain work; the CLI is a thin shell on top of it.

> Status: the core, the CLI, and a desktop GUI are implemented. The store is a
> local SQLite database — the single source of truth. There is no server and
> nothing leaves your machine.

## Contents

- [Layout](#layout)
- [Toolchain](#toolchain)
- [Build and run](#build-and-run)
- [Installing and updating](#installing-and-updating)
- [Commands](#commands)
- [Making your AI agent read the spec](#making-your-ai-agent-read-the-spec-optional)
- [Encryption at rest](#encryption-at-rest)
- [Contributing](#contributing)
- [License](#license)

## Layout

```
crates/
  amenbo-core/        domain model, persistence, operations, queries, export
  amenbo-cli/         the `amenbo` binary (clap; human + --json output; `agent --json`)
  amenbo-scratch/     test support: the throwaway directory a test works in
```

Design points:

- **Persistence**: the local store is a single SQLite file — the single source of
  truth the queries run against. JSON is an export format you can produce anytime
  (`amenbo export`).
- **IDs**: the id **is** the conversational number — a task whose id is `<n>` is shown
  as `AMB-T-<n>`, a decision whose id is `<n>` as `AMB-D-<n>`. Numbers are device-global
  (one number names one task anywhere, no project context needed) and come from two
  sibling spaces, which the kind code disjoins. Every ref amenbo shows carries the
  `AMB-` namespace, so it is self-declaring: another tracker's `T-<n>` is never
  mistaken for one of these. Reading is looser than writing — the bare forms
  (`<n>` / `#<n>` / `T-<n>` / `D-<n>`) still parse, so a ref pastes straight back.
- **Ordering**: fractional index; a task's placement (project, order) lives on the
  task itself. Classification (categories, phases, or any axis you define) is
  expressed with user-defined **dimensions**, not a fixed hierarchy.
- **Relationships**: a single dependency edge (`task depend`). There are no
  subtasks — decompose larger work into separate tasks linked by dependencies.
- **Deletes**: physical and irreversible. Deleting a project deletes the tasks,
  decisions and dimensions under it. To keep something without seeing it, archive
  it (`project archive`) instead.
- **One task, one project**: a task belongs to exactly one project; `task move`
  re-homes it — a row changing hands inside one database, not a copy.

## Toolchain

<details>
<summary>Pinned Node + Rust, one-shot setup, and the macOS-only GUI note</summary>

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

`package.json` declares `engines.node >= 22.12` (the build deps' floor); the pin
files select the exact version above that.

Go appears in the tree, but it is not a third toolchain to install: it builds only
`devtool/`, the optional helper that stamps out a git worktree per task (see
[devtool/README.md](devtool/README.md)). Nothing else reads it, so amenbo builds,
tests and ships without Go — `make devtool` is the only target that asks for it.

**Building the GUI is macOS-only.** The Tauri `.app` links the native WebKit
(WKWebView), so it can't be built or run in a Linux container — Docker /
devcontainers only suit the headless side (core / CLI / tests / CI on Linux).
The `make gui` / `install-gui` targets (and their `-dev` variants) require host
macOS; the CLI builds anywhere Rust does (`make install-dev` for a local dev
build — the prod CLI ships inside the unified installer, so `make install` is
retired).

</details>

## Build and run

<details>
<summary>Build, the fast/full test split, the nextest + shell gates, and where data lives</summary>

```bash
cargo build
cargo test                              # fast: unit + light integration suites
cargo test --features scale,e2e         # full: also the scaling guard and real-binary cli_e2e
cargo run -p amenbo-cli -- agent --json   # the single source of truth for the CLI
```

The heaviest tests are gated behind cargo features so the everyday `cargo test`
stays sub-second: the read-hotpath scaling guard behind `scale`, the real-binary
`cli_e2e` suite behind `e2e`. CI enables both (`--features scale,e2e`).

For the full run, [`cargo-nextest`](https://nexte.st) is faster (per-test process
isolation + parallelism) and prints per-test timings so the slow tests are easy to
spot — `make test` wraps it (and runs the doctests nextest skips). It also gates the
out-of-workspace Tauri host crate (`app/src-tauri`), the GUI front-end, and the shell
scripts that drive the release and the on-hardware checks, mirroring CI's `app-rust` +
`gui-web` + `shell` jobs, so a core change that only breaks the GUI — or a quoting slip
in a release script — is caught locally:

```bash
cargo install cargo-nextest            # one-time (or https://get.nexte.st)
brew install shellcheck actionlint     # one-time (the shell gate; any package manager will do)
make test                              # shell gate + core/cli (scale,e2e + doctests) + app crate clippy/test + GUI typecheck/build/test
make shell-gate                        # the shell leg on its own (every tracked *.sh and git hook, plus the shell embedded in each workflow's run:)
cargo nextest run --features scale,e2e # core/cli only, without the doctest + GUI legs
```

The DB layer is [rusqlite](https://docs.rs/rusqlite) and nothing else: the change feed rides
SQLite's update hook, which belongs to the connection, and reads are issued from inside the
write transaction — both are lost the moment a second library opens a connection of its own.
The schema is declared once, in the `store_engine::schema` registry, and the `CREATE TABLE`
DDL and the per-column write whitelist are derived from it, so they cannot drift apart.

A gate only ever compiles the cfg branches of the OS it runs on, so `#[cfg(target_os =
"linux")]` code (the store-watch network-FS path, for one) is invisible to a green run on a
mac. `make lint-linux` builds the tree for Linux inside the container the Linux bundles are
built in, running the same two clippy invocations as CI's `rust` + `app-rust` jobs. Cross-
compiling won't do: the Tauri gtk/glib sys crates need their system deps at build time, so
a real Linux is the only way to reach that code. It needs Docker and takes a few minutes,
which is why it is not part of `make test` — reach for it when you touch an OS-gated branch.

Plain `cargo test` still works everywhere; nextest is an optional accelerator.
Thresholds and the `ci` profile live in `.config/nextest.toml`.

Data is stored under the OS-standard location (on macOS,
`~/Library/Application Support/amenbo/store.sqlite`). Set `AMENBO_HOME` to
override the location (useful for tests and explicit setups).

</details>

## Installing and updating

<details>
<summary>Per-OS installers, in-place CLI self-update, and verifying a download</summary>

amenbo ships as a single per-OS installer that carries both the GUI and the CLI
(macOS `.pkg`, Windows NSIS, Linux `.deb`/`.rpm`), published on this repository's
[Releases](https://github.com/ShiroDoromoto/amenbo/releases). Download the one for
your platform and run it — it installs the desktop app and puts the `amenbo` CLI on
your PATH in one step. The installers are self-signed rather than notarized, so the
first launch shows the operating system's unsigned-app prompt (on macOS,
right-click → **Open**; on Windows, SmartScreen's **More info** → **Run anyway**);
once approved it does not recur.

On Linux the `.deb`/`.rpm` are the system-wide route — they install to `/usr/bin`
and put both the GUI and the CLI on PATH. A Linux GUI **AppImage** is published
alongside as well: a single self-contained file that needs no root — place it on
your PATH yourself (e.g. `~/.local/bin`) and run it. It carries the GUI only (use
the CLI installer or the `.deb`/`.rpm` for the `amenbo` command).

If you move from the `.deb`/`.rpm` to the per-user AppImage/CLI, retire the old
system-wide copy yourself — the per-user build cannot (it is not root): `sudo apt
remove amenbo` on Debian/Ubuntu, or `dpkg -r amenbo` / `rpm -e amenbo` elsewhere.
The CLI reminds you once while a `/usr/bin` copy is still present; on the stock PATH
`~/.local/bin` wins, so a leftover copy is harmless beyond the version skew.

To update, download the latest installer and run it again — it replaces both the
desktop app and the CLI at once. amenbo notices when a newer version is out (see the
update check below) and points you at that installer. A **standalone CLI** (installed
without the desktop app) can also update itself in place: `amenbo update --apply`
downloads the new CLI over TLS and swaps this binary — no installer, no elevation. A
CLI installed alongside the desktop app is managed by it and is updated with the app,
not on its own (`--apply` there points you back to the installer). An `--apply` keeps the
binary it replaces beside the new one, so `amenbo update --rollback` undoes the last update
offline — no download — if the new version misbehaves. Either way, applying an update is
always your explicit call — amenbo never updates itself in the background.

**Verifying a download (optional).** Every release is built by this repository's
public CI, and each asset carries a build attestation signed with the release
workflow's GitHub OIDC identity — not a key on anyone's laptop. With the
[GitHub CLI](https://cli.github.com) you can confirm an installer was built from
this source at a known commit, and not tampered with or produced elsewhere:

```bash
gh attestation verify amenbo-darwin-arm64.pkg --repo ShiroDoromoto/amenbo
```

A pass means the bytes you are about to run came from this repo's release workflow.
Substitute your platform's asset name.

On the network, amenbo keeps two layers apart. **Feature-side communication stays
at zero**: no telemetry, no phone-home, no central server — your task data never
leaves your device. The one exception is an **infra-side update check**: amenbo
reads a small static `latest.json` from this repository's latest release to notice when a
newer version is out. That request carries no task data, is on by default, and can
be turned off (`amenbo config set update_check false`, or `AMENBO_UPDATE_CHECK=0`).
The check only reads that static file; amenbo never updates itself in the background —
downloading and applying a new version is always something you set off yourself
(`amenbo update`, or `amenbo update --apply` for the standalone CLI).

</details>

## Commands

<details>
<summary>The full command tour — projects, tasks, dimensions, decisions, attachments, backup/restore, hooks</summary>

The CLI surface is self-documenting: `amenbo <cmd> --help` and `amenbo agent --json`
are the authoritative spec (there is no separate command reference to drift out of date).

```bash
amenbo                       # today's tasks + suggested next actions (discover)
amenbo agent --json          # how to work here + an index of the commands (the AI's entry point)
amenbo agent --command "task add"   # one command's full spec — flags, args, examples
amenbo agent --full          # every command's full spec inline

# Projects, tasks
amenbo project add --name "Website refresh" --view board
amenbo task add --title "Wireframes" --project "Website refresh" --due tomorrow --priority high
amenbo task add --title "Pick colors" --project "Website refresh"
# Every id is the number amenbo shows you: task AMB-T-<n> is `<n>`, decision AMB-D-<n> is `<n>`
amenbo task depend <n> --on <m>            # <n> waits on <m> (dependency, not a subtask)
amenbo task done <n>

# Dimensions: user-defined classification axes (categories, phases, or anything
# you need). New projects seed none — create the axes you want. --ordered gives
# the values an order; --time-axis marks the ordered time lane (roadmap stages),
# with ordering ("don't start later work yet") enforced by dependencies, not the axis.
amenbo dimension add --project "Website refresh" --name "Area" --ordered
amenbo dimension value-add Area --name "Design"
amenbo dimension set 12 Area Design           # assign a task a value on the axis
# On a time-axis, each value spans a period; an open end means it is ongoing.
# A new task starts on whichever value covers today — never forced, always yours
# to change with `dimension set` / `unset`.
amenbo dimension value-add Era --name "Beta" --start 2026-07-08
amenbo dimension value-update Era Beta --end 2026-12-31
amenbo dimension list --project "Website refresh" --json   # axes + their values
# Slice tasks by any axis. `dim:` repeats (the parts AND); `=none` = unassigned on
# that axis. `time_axis:` is sugar for whichever axis you marked --time-axis.
amenbo task list --filter "dim:Area=Design dim:Kind=none" --json
amenbo task list --filter "time_axis:v2 done:false" --json

# Comments, assignees. There are exactly two: you (`me`) and your AI (`me-ai`).
amenbo config set human_name "Alice"        # change your own display name (ai_name renames your AI)
amenbo task assign 12 --to me
amenbo task add --title "Triage" --project "Website refresh" --to me --ai  # create + delegate (here to your AI) in one step
amenbo comment add 12 --text "waiting on client"
amenbo comment list 12                      # oldest first; each line starts with the comment's id
amenbo comment edit 42 --text "corrected"    # rewrite one in place (id, timeline slot and attachments stay)
amenbo comment rm 42 --yes                  # delete one posted by mistake (permanent, attachments go too)

# Attachments: ingest a file as a content-addressed blob (bytes kept out of the
# truth source), or attach an external link. Works on tasks, decisions, and
# individual comments (a comment's attachments are kept separate from its parent's).
amenbo task attach 12 ./design.png              # ingest a file (blob; mime from extension)
amenbo task attach 12 https://example.com/spec --url --name spec   # external link
amenbo decision attach AMB-D-<n> ./benchmark.csv
amenbo comment attach 42 ./note.png             # attach to one task comment (id from `comment list`)
amenbo decision comment attach 7 ./note.png     # attach to one decision comment (id from `decision comment list`)
amenbo attach ls AMB-T-<n>                       # list a task's or decision's attachments (the kind code names the space)
amenbo attach ls --task-comment 42              # a comment is named by a flag: the two comment tables number apart
amenbo attach open 3                            # open a blob (OS opener) or the URL (id from `attach ls`)
amenbo attach save 3 --out ./spec.pdf           # write a blob's bytes to a file (or a dir → its own filename); --force to overwrite
amenbo attach rm 3 --yes                        # remove (confirms without --yes; the file's bytes go with it once nothing else references them)

# Re-home a task to another project (a task belongs to exactly one project)
amenbo task move 12 --project "Backlog"

# Commit SHAs: anchor a task to the git commits that implemented it (1 task : many).
# amenbo stores each SHA opaquely — it never reads git or knows which forge it lives on;
# the chain runs history -> task, since a public commit carries no store-local reference.
amenbo task commit add 12 0123456789abcdef0123456789abcdef01234567   # full-length hex only
amenbo task commit list 12                   # oldest first (git show <sha> goes the other way)
amenbo task commit rm 12 <sha> --yes         # forget one (permanent)
amenbo task list --filter "commit:<full-sha>" --json # walk history -> task inside amenbo: which task(s) recorded this commit (an unknown SHA is empty, not an error)

# Dependencies: this task must wait for a blocker to be done first
amenbo task depend 13 --on 12                # 13 is blocked until 12 is done
amenbo task undepend 13 --on 12
# A task is ready when no blocker is open, every decision linked to it is accepted, and
# its declared start day has arrived; ready:yes hides what is not ready, ready:no lists
# what's waiting — and every task says which of the three is holding it back. Reserving a
# task that is not ready is refused (not_ready) — resolve the premise; there is no --force
amenbo task list --filter "ready:yes" --json
# start:future is the waiting queue on its own — what a start day still ahead holds back
# (start:today = the day has come, start:none = no start day declared)
amenbo task list --filter "start:future" --json

# Decision records: durable "why we chose X" (a Task sibling, not a task —
# no mailbox workflow, its own device-global number space)
amenbo decision add --title "SQLite as the source of truth" \
  --body "the local SQLite store is the single truth source" --project "Website refresh"
amenbo decision accept AMB-D-<n>              # proposed -> accepted
amenbo decision accept AMB-D-<n> --reason "agreed after the perf review" # ...and note why (reason lands as a decision comment)
amenbo decision reject AMB-D-<n> --reason "the simpler one covers it" # proposed -> rejected, with a reason comment
amenbo decision edit AMB-D-<n> --body "…refined rationale…" # edit title/body in place — proposed or accepted alike (supersede to overturn; rejected is terminal)
amenbo decision comment add AMB-D-<n> --text "revisited after the 10k benchmark — still holds" # discuss on the timeline (comments are the discussion around the body)
amenbo decision comment list AMB-D-<n> --json # oldest first; --limit/--offset page
amenbo decision comment edit 7 --text "corrected" # rewrite one in place (this edits a comment, not the decision's own body)
amenbo decision comment rm 7 --yes           # delete one posted by mistake (permanent, attachments go too)
amenbo decision reopen AMB-D-<n>              # accepted -> proposed: un-settle a too-hasty acceptance (editing needs no reopen)
amenbo decision supersede AMB-D-<n> --replaces AMB-D-<m> # record a replacement (chain)
amenbo decision amend AMB-D-<n> --amends AMB-D-<m> # partial revision (target stays current, not superseded)
amenbo decision builds-on AMB-D-<n> --on AMB-D-<m> # the premise: read it first, and revisit this one if it is overturned
amenbo decision delete AMB-D-<n> --yes        # retire a decision (permanent; supersede keeps it instead)
amenbo decision link AMB-D-<n> AMB-T-<n>       # cross-link a decision and its task
amenbo task list --filter "decision:AMB-D-<n> status:todo" --json # walk the link: the open work a decision produced
amenbo decision list --filter "task:AMB-T-<n>" --json          # ...and the other way: the decisions a task rests on
amenbo decision list --filter "status:accepted" --json
amenbo decision list --filter "status:accepted text:backup" --with-body --limit 20 --json # bodies too (projection; composes with filter/paging) — read a bounded, keyword-narrowed slice to scan for semantic contradictions (propose only; a human confirms as supersede/amend)

# Status and data ownership
amenbo status                               # overdue / today / in-progress summary
amenbo task list --filter "done:false due:today priority:high" --json
amenbo export --out ./amenbo-export         # everything: a directory — export.json plus every attachment's bytes
amenbo export > ./amenbo-export.json        # ...or the same JSON on stdout (records only — a stream cannot carry files)
amenbo backup ./everything.amenbo-backup      # archive: the store plus its attachments (disaster recovery)
amenbo restore ./everything.amenbo-backup --yes # destructively restore this device from the archive

# Keep amenbo's refs out of what leaves the store — an id resolves only for
# someone holding it, so `AMB-` refs are noise in a commit, a diff or a PR body.
# Read-only: it reports `path:line` and exits non-zero, and never edits anything.
amenbo lint                                 # the staged diff: what the commit is about to add
amenbo lint .git/COMMIT_EDITMSG             # ...or a file — what git hands a commit-msg hook
amenbo lint --stdin < message.txt           # ...or piped text. Needs no store, so CI answers the same

# Run that lint on every commit: `pre-commit` reads the staged diff, `commit-msg` reads
# the message (git offers it nowhere else). Installing writes into your git plumbing, so
# amenbo asks once — for the lint as a feature — and that one answer covers every repository
# it works in, the ones you bind later included. It owns only its own marked block: where a
# hook from husky, lefthook or your own hand already sits, amenbo slips its block in after the
# shebang and both run, leaving that hook untouched; uninstall takes only the block back out.
amenbo hooks install                        # wire the lint hooks here (`git commit --no-verify` bypasses them)
amenbo hooks status                         # what is in each hook slot, and what this device answered
amenbo hooks uninstall                      # remove amenbo's hooks here, and opt this repository out

# Identity
amenbo whoami                               # this store's identity
amenbo init --name Alice                    # create the store (genesis)
```

Output and conventions:

- Read commands (`status`/`list`/`show`/`config`/`doctor`/`validate`/`lint`/
  `project`/`dimension`/`user`/`comment`/`decision list`/`decision show`/
  `hooks status`) support `--json`.
- Write commands return a common envelope (`ok`/`action`/`noop`/`changed` + the
  resulting resource) under `--json`.
- Destructive operations (`delete`, …) prompt by default; pass `--yes`/`-y` for
  non-interactive use. Exit codes: `0` success, `1` error, `2` bad arguments.
- `export` is one way: amenbo writes your data out, and has no command that reads
  it back in. Putting your own data back is `restore` from a `backup` archive.

**Crash & corruption safety.** The store is a SQLite database in WAL mode, so a
crash mid-write never leaves a half-written store — an interrupted write rolls back
to the last consistent state. Two complementary nets sit on top, with distinct roles:

- **`amenbo export`** — a portable, human-readable dump you can run anytime. This is
  the format for *migration, inspection, and data ownership* (no lock-in), and it is
  **one way**: it hands your data to whatever you move to next, and nothing reads it
  back into amenbo (the way back is a `backup` archive and `restore`). It covers
  **everything** — this device's database, every project in it — and there is no
  narrower shape: an excerpt or a human-readable table would not get you moved. With
  `--out <dir>` it writes a directory: `export.json` (the records) next to
  `attachments/`, holding every attachment's actual file, laid out under the task or
  decision it hangs on — a one-way export that left the files behind would not be
  taking your data with you. With no `--out` the same JSON streams to stdout for
  piping (records only — a stream has nowhere to put the files).
- **`amenbo backup [path]`** — a byte-faithful physical snapshot for *disaster
  recovery*. It bundles everything on this device — one database, holding every project —
  into one verified `.amenbo-backup` archive at `path`: the database is snapshotted via
  `VACUUM INTO` (no torn DB+WAL of a hand-copied file), bounded-verified (integrity check +
  a COUNT probe, never a full load, so a huge store won't stall it), and stamped with its
  format generation in a `manifest.json`.
  Attachment bytes ride along: every blob is bundled beside the snapshot, so a
  restore on another machine brings the files back, not just the rows that point at them.
  To recover, `amenbo restore <path>` destructively replaces the database with the archive's;
  the snapshot is gated on its generation before anything
  is swapped in (all-or-nothing), the previous truth source is set aside with a timestamp, and
  a mid-swap failure rolls back — so a bad restore can never leave you worse off. An archive
  from a *newer* amenbo is refused (update first), and so is one written before the store became a
  single database (restore it with the amenbo that wrote it). Both prompt and allow cancellation.
  Save the archive wherever your own backup regime keeps files — an external drive, an iCloud
  folder — so it survives a lost PC; recovery needs no key or passphrase on any machine.
- **Older archives** — an archive written before the store consolidation (several stores plus a
  root overview store) is refused rather than partially applied: restore it with the build that
  wrote it. That shape is the only place several stores still appear — a device this build opens
  holds one database, and its archives carry one snapshot.

Neither carries key material: amenbo holds no secrets or encryption keys at all, so a
backup or export has nothing sensitive to include (machine-local identity — the display
name and hardware binding — is likewise left out of an archive, so a restore never
overwrites the destination machine's identity). The store itself is plaintext (see *Encryption at rest*
below), so an archive or snapshot is self-contained — recovery needs no key or
passphrase on any machine.

On every open amenbo also runs a read-only integrity check of the store and prints a
warning if anything looks off — it never repairs automatically (use `amenbo doctor --fix`
for that, which also reclaims attachment files nothing references any more). Turn it off
with `amenbo config set startup_integrity_check false`. The app warns on launch too, and
adds the bound folders whose `.amenbo` pointer is gone or still in a legacy format — an AI
started there would not reach the project, and nothing else would have told you. The full
face is under *Settings → Integrity*: it lists everything `amenbo doctor` finds — the store
and this machine's bound folders — and runs the same repairs, so the CLI is never required
to fix what the app shows you.

</details>

## Making your AI agent read the spec (optional)

<details>
<summary>Point your AI at the agent spec, and how the binding bounds its reach</summary>

`amenbo init` writes a small managed block into the folder's `CLAUDE.md` /
`AGENTS.md` whose one job is to tell the AI: *before you work here, run `amenbo
agent --json` and follow it.* That block is a thin, frozen pointer — the actual
workflow and rules live in `amenbo agent --json` (in the binary, so an update
ships them immediately), not duplicated in the block.

**The binding is also the AI's reach.** An AI (`--actor ai` / `AMENBO_ACTOR=ai`)
started in a bound folder operates that folder's project and nothing else: it
cannot name another project (`--project` and the `project:` filter are yours, not
its), and reading or writing another project's tasks, decisions or comments is
refused with `out_of_reach`. So one machine can hold every project you
have while an agent you start in one of them only ever sees that one — the folder
you launch it in is the boundary, and you draw it with `init` / `bind`.

Whether the agent actually runs `amenbo agent --json` still depends on the agent
reading that block. If you want a **hard guarantee** rather than relying on the
prompt, and you use Claude Code, add an **opt-in** [SessionStart
hook](https://docs.claude.com/en/docs/claude-code/hooks) that injects the spec at
the start of every session — its stdout is added to the session context:

```jsonc
// .claude/settings.json (or ~/.claude/settings.json to apply everywhere)
{
  "env": { "AMENBO_ACTOR": "ai" },
  "hooks": {
    "SessionStart": [
      { "hooks": [ { "type": "command", "command": "amenbo agent --json" } ] }
    ]
  }
}
```

This is entirely opt-in — amenbo never installs it for you and does not require
it; it is just the deterministic way to be sure the spec is in front of the agent
every session, closing the gap where `init` writes the block mid-session (so it
does not bind until the *next* one). Use a `UserPromptSubmit` hook instead if you
would rather re-inject it on each prompt.

</details>

## Encryption at rest

<details>
<summary>Plaintext store; on-device secrecy via full-disk encryption</summary>

The truth source is **plaintext** SQLite. amenbo does not encrypt the store at the
application layer — on-device secrecy is delegated to the operating system's
full-disk encryption (FileVault on macOS, BitLocker on Windows), the standard
whole-disk protection for a single-machine, local-only tool. There is nothing to
turn on.

A store that a much earlier version wrote encrypted (SQLCipher) is no longer migrated
automatically: open it once with a transitional build (the one that shipped the
decrypt-on-open step) to convert it to plaintext before using this build. A new or
already-plaintext store needs nothing.

Concurrent writers to the same store are serialized by an exclusive file lock, so
nothing is lost. Different stores (different `AMENBO_HOME`) never contend, which is
what makes single-machine multi-store isolation safe.

</details>

## Contributing

Issues — bug reports, questions, feature ideas — are welcome. For pull requests,
please open an issue to discuss first; see [CONTRIBUTING.md](CONTRIBUTING.md). Taking
part means holding to the [Code of Conduct](CODE_OF_CONDUCT.md). To report a security
vulnerability, don't use a public issue — follow [SECURITY.md](SECURITY.md).

## License

Apache License 2.0 — see [LICENSE](LICENSE). Copyright the amenbo authors.

The license covers the code. It does **not** grant any right in the **amenbo name or
the logo** (the water strider): Apache-2.0 grants no trademark rights, so a fork that
redistributes this code must carry its own name and mark.
