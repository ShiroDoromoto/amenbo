# worktree — amenbo's official per-task git worktree plugin

Give each task its own git worktree, and take it away again when it is done.

Several implementation sessions can then run at once without stepping on each other:
one task, one checkout, one branch. The checkout lives **outside** the repository, so
it is a pure development environment — nothing in it is bound to amenbo, and nothing
about it can be mistaken for part of the project.

```
<repo>/../<repo-name>-worktrees/<id>/    git worktree checkout on task/<id>
```

This is also the **reference implementation** of the amenbo plugin contract. It is
deliberately small, and it exercises everything an author has to get right: both plugin
faces, the payload on stdin, a declared setting, and the split between the return value
and the diagnostics.

## Use

Run these from the repository the task belongs to — the folder amenbo is bound to,
which is where it finds the project this plugin is enabled for.

```sh
# reserve the task with amenbo; the hook then suggests the command below
amenbo task status 123 in_progress

# enter a worktree of its own
eval "$(amenbo plugin run worktree start 123)"

# …work, commit, merge…

# and take it away again (from the main repo)
amenbo plugin run worktree finish 123
```

| | |
|---|---|
| `start <id> [--base <branch>]` | Add a worktree for the task on a fresh branch `task/<id>`, and return the `cd` into it. |
| `finish <id> [--base <branch>] [--force]` | Remove that worktree and its branch. Refused while the worktree is dirty or the branch has commits `<branch>` does not; `--force` overrides both. |

**The backlog stays with amenbo.** This plugin touches git and nothing else: reserving a
task, commenting on it and closing it are `amenbo task …`, run from the main repo. That
is the whole division of labour — amenbo owns the task's state, the plugin owns the
checkout.

### Settings

| key | what it does |
|---|---|
| `base` | The branch a worktree is cut from when `--base` is not given. Defaults to `main`. |

```sh
amenbo plugin config set worktree base develop
```

## Install

```sh
amenbo plugin install worktree     # fetch and verify the released asset
amenbo plugin enable worktree      # installing never runs anything; this is the consent
```

`enable` is per project, because whether tasks in *this* folder want their own checkouts
is a per-project answer.

## The contract, as this plugin reads it

A plugin is just an executable. amenbo starts it, writes one JSON document to its stdin,
and reads back what it wrote and how it exited. Which of its two faces is being called is
told by the arguments:

**No arguments — the observation face.** An event fired and amenbo moved on; nobody is
waiting. The document on stdin describes the event:

```json
{ "v": 1, "event": "task.status_changed", "id": 123, "actor": "ai", "at": "…", "new": "in_progress" }
```

This plugin subscribes to `task.status_changed` alone, and on a reservation it writes one
suggestion to stderr — nothing more. It does **not** create the worktree here. Whether
*this* task wants a checkout of its own is a judgement, and the judgement belongs to
whoever reads the suggestion; a hook that acted on it would be a side effect on a write
path that had no chance to refuse. What the plugin does settle by itself is the fact
underneath: outside a git repository the suggestion would be noise, so nothing is
written. See [`hook.go`](hook.go).

**Arguments — the command face.** Someone invoked the plugin on purpose and is waiting
for an answer. Here the three channels each have exactly one job, and the whole contract
rests on keeping them apart:

- **stdout is the machine return value.** amenbo relays it to the caller verbatim, which
  is why `start` prints one `cd` line and why `eval "$(…)"` works at all. A summary
  leaking into stdout would be evaluated as a shell command.
- **stderr is for humans.** Summaries, refusals, context.
- **the exit code is the verdict.** Non-zero means the return value is broken; amenbo
  discards it and reports a failed call.

The stdin document is smaller on this face — no event fired — and carries the plugin's
own non-secret settings:

```json
{ "v": 1, "config": { "base": "main" } }
```

**`v` is the contract version**, and it is first on the wire. New fields are added
silently, so a plugin ignores what it does not recognise; `v` moves only when an existing
field's meaning breaks. That is why the hook here refuses to act on any other version
rather than guessing — see [`main.go`](main.go).

## Build

A single static Go binary, no runtime and no dependencies beyond `git`:

```sh
make build     # → ./worktree
make test      # gofmt, go vet, go test
```

To try a build before there is a release to install from, hand-install it into a
throwaway amenbo base:

```sh
make install AMENBO_BASE="$AMENBO_HOME"
amenbo plugin enable worktree
```

That lays the binary down beside [`dev/manifest.json`](dev/manifest.json), a stand-in for
the entry the catalog holds: it carries the same fields, released assets and digests, so
the two can be read against each other — but nothing about a hand-install is verified.
The real install route resolves the catalog entry and checks the asset's provenance
before anything lands on disk.

To cut a release, `make dist` builds the assets the catalog entry points at — one
universal build for every Mac, one per architecture on Linux — and prints their digests:

```sh
make dist                                    # → dist/worktree-v1-*.tar.gz + sha256 digests
gh release create v1 dist/*.tar.gz           # publish them
```

## Requirements

- git 2.31 or newer (`git rev-parse --path-format`)
- macOS or Linux

## License

Apache-2.0. See [LICENSE](LICENSE).
