package main

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

// `devtool devgui cli <id> -- …` — an amenbo CLI pointed at the store one task's dev GUI reads.
//
// A task's dev GUI opens the setup it was seeded with, and nothing else: whatever a screen needs in
// order to show anything (a rejected task, a card with a due date, a plugin in a given state) has to
// be *in* that store before the screen can be looked at. Until this existed there was no way to put
// it there. The app-data name is fixed at build time (`AMENBO_APP_NAME`), so no CLI on the machine
// is pointed at `amenbo-dev-<id>`, and building one with that name set costs two minutes for a
// binary only that one task can use.
//
// **What is baked at build time is a name; what it selects is a directory.** `AMENBO_HOME` names
// that directory at run time — the same seam `make verify` isolates through — so the CLI that runs
// here is the worktree's own `cargo build -p amenbo-cli`, and the only thing this adds is where it
// looks. Nothing is built per task that was not being built anyway, and the binary that seeds the
// store is the one the task is written in.
//
// Two consequences of pointing a build at a store rather than building it for one, both deliberate:
//
//   - **The binary still introduces itself by its own channel.** Production is the
//     `AMENBO_APP_NAME` devtool builds it with, so anything keyed to the channel rather than to the
//     store — the command name guidance words, the perf log's default — reads as production. It
//     writes the right store; it says the wrong name while doing it.
//   - **It will migrate that store's format if the tree is ahead of it.** A store named by the
//     environment is an isolated one, which is an arm of the gate that otherwise holds an unreleased
//     build back from migrating. That is the wanted answer here: the task's own GUI is built from
//     the same tree and would carry the store forward the moment it opened.

// storeEnv is what amenbo takes its base directory from, and so how a build is pointed at a store it
// was not built for. Named here rather than spelled inline because it is a contract with core
// (`env::HOME_VAR`), not a string this file invented.
const storeEnv = "AMENBO_HOME"

// taskCLIBin is the CLI a task's checkout builds — the ordinary debug build, which is the one its
// tests and `make verify` already produce, so seeding a store costs no build of its own.
func taskCLIBin(worktree string) string {
	return filepath.Join(worktree, "target", "debug", "amenbo")
}

// splitDoubleDash cuts `args` at the first `--`, returning what came before it and what came after.
// `ok` is false when there is none — the amenbo arguments must be handed over explicitly, or the
// first `--json` in them would be read as a flag of devtool's own (Go's flag package refuses an
// unknown one, so the failure is a confusing message rather than a wrong store).
func splitDoubleDash(args []string) (head, rest []string, ok bool) {
	for i, a := range args {
		if a == "--" {
			return args[:i], args[i+1:], true
		}
	}
	return args, nil, false
}

// taskCLI runs `argv` as an amenbo command against task id's own dev store, and returns the exit
// code it ended with so the caller can end the same way — a seeding step that failed has to be
// visible as a failure to whatever ran it.
//
// The CLI comes from the task's worktree and is rebuilt first (skip with `--no-build`), because a
// store is usually being seeded *for* a change in that tree: seeding it with yesterday's binary is
// how a screen ends up showing what the old code wrote.
//
// It runs **in the store's own directory**, not in the worktree. Relative paths in `argv` resolve
// there, and a `bind` writes its `.amenbo` pointer beside the store it points into — which the
// instance's teardown reclaims along with everything else. In the worktree that same pointer would
// be a live one for *any* amenbo run there, including the production binary, which is exactly the
// reach the worktree is kept outside the repo to deny.
func taskCLI(id string, noBuild bool, argv []string) (int, error) {
	if runtime.GOOS != "darwin" {
		return 0, fmt.Errorf("the per-task dev store lives where the dev GUI is installed, which is macOS only")
	}
	if len(argv) == 0 {
		return 0, fmt.Errorf("nothing to run — `devtool devgui cli %s -- <amenbo args…>`", id)
	}
	_, worktree, err := paths(id)
	if err != nil {
		return 0, err
	}
	if _, err := os.Stat(worktree); err != nil {
		return 0, fmt.Errorf("no worktree for task %s (%s missing) — cut one with the `worktree` plugin first", id, worktree)
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return 0, err
	}

	store := appDataDir(home, taskDevAppData(id))
	if !dirExists(store) {
		// `devgui seed` clones the shared dev store into this; a checkout with no app/ gets none.
		// Making it here is what lets the first command create a store rather than fail on a
		// missing directory — an empty instance is a usable one.
		if err := os.MkdirAll(store, 0o755); err != nil {
			return 0, fmt.Errorf("make the task's store dir: %w", err)
		}
		logf("  store   : %s was not there — this run starts it empty", store)
	}
	if !noBuild {
		if _, err := runEnv(worktree, cliBuildEnv, "cargo", "build", "-q", "-p", "amenbo-cli"); err != nil {
			return 0, fmt.Errorf("build the task's CLI: %w", err)
		}
	}
	bin := taskCLIBin(worktree)
	if _, err := os.Stat(bin); err != nil {
		return 0, fmt.Errorf("no CLI at %s — drop --no-build so it is built", bin)
	}

	logf("  store   : %s", store)
	logf("  cli     : %s %s", bin, strings.Join(argv, " "))
	return runThrough(store, []string{storeEnv + "=" + store}, bin, argv...)
}
