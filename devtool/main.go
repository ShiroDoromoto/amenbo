// Command devtool is amenbo's portable developer-support CLI: a single static
// Go binary (no runtime, no venv) that can be dropped into any project. It stamps
// out and tears down per-task git worktrees so several implementation sessions can
// run in parallel without stepping on each other, and it stands up a fake outside
// world (fixtures.go) for verifying the GUI against answers the real one will not
// give on demand.
//
// A task's worktree lives OUTSIDE the repo, in a sibling dir:
//
//	<repo>/../<repo-name>-worktrees/<id>/   git worktree checkout on task/<id>
//
// Outside-the-repo is deliberate: it is a pure development environment. With no
// repo `.amenbo` in its ancestry, amenbo commands run there (e.g. running the
// dev build for debug verification) cannot reach the real backlog — they fall
// to an isolated/throwaway store. That keeps two concerns physically apart:
//
//   - Project management (status/comment/done) → the PROD `amenbo` binary
//     run from the MAIN repo, against the real backlog. devtool's own reservation
//     does exactly this (prod binary, anchored to the main worktree root).
//   - Debug verification (does my code work) → the worktree's dev build against
//     a throwaway store (e.g. `make verify`), inside the outside worktree.
//
// The GUI is isolated a second way, because a bundle is installed machine-wide
// and a worktree cannot contain it: a task gets its own throwaway dev GUI, with
// its own identifier and app-data, seeded from the shared dev store. devtool
// provisions and deletes it around the worktree (devgui.go); the Makefile builds
// it, so only the tasks that look at a GUI pay for one.
//
// Beyond that seed devtool provisions no amenbo store: isolation comes from the
// worktree living outside the repo plus `make verify`'s mktemp store.
package main

import (
	"errors"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

func main() {
	args := os.Args[1:]
	if len(args) == 0 {
		usage()
		os.Exit(2)
	}
	switch args[0] {
	case "task":
		taskCmd(args[1:])
	case "agent":
		agentCmd(args[1:])
	case "fixtures":
		fixturesCmd(args[1:])
	case "help", "-h", "--help":
		usage()
	default:
		logf("devtool: unknown command %q", args[0])
		usage()
		os.Exit(2)
	}
}

func usage() {
	logf(`devtool — amenbo developer-support CLI

Usage:
  devtool task start  <id> [--base main] [--no-reserve] [--no-deps]
  devtool task finish <id> [--base main] [--force] [--reset]
  devtool agent size       [--base main] [--json]
  devtool fixtures refresh [--catalog <url|path>] [--repo owner/name]
  devtool fixtures gui     [--fail <face>=<status|timeout>] [--port n] [--app path] [--no-launch]

task start   reserve <id> (todo→in_progress, prod amenbo from the main repo) and
             add a git worktree on branch task/<id> in a sibling dir OUTSIDE the
             repo — a pure dev env. For a GUI checkout it also runs a best-effort
             'npm ci' in app/ (skip with --no-deps) and seeds the app-data of the
             task's own throwaway dev GUI from the shared dev store — build it
             with 'make install-gui-dev AMB-T-ID=<id>'. Manage the backlog
             (comment/done) from the main repo; verify code there with
             'make verify'.
task finish  safely tear it down: refuse unless the worktree is clean and the
             branch is merged into --base (override with --force). Deletes the
             task's dev GUI too — the bundle and its app-data both.
fixtures     a fake outside world for GUI verification. 'refresh' captures the
             catalog, GitHub's answers and latest.json from the real world (they
             are copies, never written by hand); 'gui' serves them and starts the
             dev GUI pointed at them. --fail makes a face answer 429/500/404, or
             never answer at all — the responses the real API will not produce on
             demand, and so the branches nothing else reaches.
agent size   print what this tree does to the 'amenbo agent --json' entry, by
             section, against the merge-base with --base. A signal, not a gate:
             it always exits 0. The entry is read once per AI session, so what
             lands there is paid forever — the delta asks the author whether
             what grew is a spec or an argument.`)
}

// parseAroundID parses `fs` over args in which the task id may sit on either side of
// the flags, and returns the id it found ("" if there is none).
//
// Go's flag package stops at the first non-flag word, so one `fs.Parse` reads only the
// flags that lead: in `task start <id> --no-reserve` the flag goes unread. Peeling the
// leading word off as the id before parsing has the mirror failure — with a flag in
// front there is nothing to peel, the id stays empty, `fs.Parse` swallows it as a
// leftover nobody reads, and a command handed an id reports it missing. Looping takes
// both: each leftover leading word is a positional, and the rest is parsed again.
// Either order works, which is what every neighbouring tool (git, cargo, go itself)
// does. An extra positional comes back too, for the caller to refuse: two ids would
// silently start whichever came first.
func parseAroundID(fs *flag.FlagSet, args []string) (id string, extra []string) {
	for {
		fs.Parse(args)
		if fs.NArg() == 0 {
			return id, extra
		}
		if id == "" {
			id = fs.Arg(0)
		} else {
			extra = append(extra, fs.Arg(0))
		}
		args = fs.Args()[1:]
	}
}

// taskCmd dispatches the `task` subcommands. The id is a positional and the flags may
// come before or after it (see parseAroundID).
func taskCmd(args []string) {
	if len(args) == 0 {
		usage()
		os.Exit(2)
	}
	sub := args[0]
	rest := args[1:]

	// One id per invocation: the worktree and branch are named after it, so a second
	// one is a typo, not a batch.
	refuseExtra := func(extra []string) {
		if len(extra) > 0 {
			logf("devtool: task %s takes one id, got extra argument(s): %s", sub, strings.Join(extra, " "))
			usage()
			os.Exit(2)
		}
	}

	switch sub {
	case "start":
		fs := flag.NewFlagSet("task start", flag.ExitOnError)
		base := fs.String("base", "main", "branch to base the worktree on")
		noReserve := fs.Bool("no-reserve", false, "assume the task is already in_progress; only verify")
		noDeps := fs.Bool("no-deps", false, "skip the best-effort `npm ci` for GUI app/ checkouts")
		id, extra := parseAroundID(fs, rest)
		refuseExtra(extra)
		id = mustID(id)
		if err := taskStart(id, *base, *noReserve, *noDeps); err != nil {
			logf("devtool: %v", err)
			os.Exit(1)
		}
	case "finish":
		fs := flag.NewFlagSet("task finish", flag.ExitOnError)
		base := fs.String("base", "main", "branch the task must be merged into")
		force := fs.Bool("force", false, "tear down even if dirty or unmerged")
		rel := fs.Bool("reset", false, "also return the task to todo (amenbo status)")
		id, extra := parseAroundID(fs, rest)
		refuseExtra(extra)
		id = mustID(id)
		if err := taskFinish(id, *base, *force, *rel); err != nil {
			logf("devtool: %v", err)
			os.Exit(1)
		}
	default:
		logf("devtool: unknown task subcommand %q", sub)
		usage()
		os.Exit(2)
	}
}

// mustID validates the task reference and returns its canonical form: the
// conversational number (digits only), with an optional leading '#' stripped.
// Requiring the number — not a ULID or an id-prefix — is what keeps the worktree
// dir and branch name canonical (`task/<number>`), since paths()/branchName() derive
// those names verbatim from this string: two `task start` invocations naming the same
// task in different forms would otherwise produce two differently-named worktrees and
// slip past the "already exists" guard, silently double-starting it in parallel
// sessions. Pinned to the number, the second start collides on the same path and
// branch and is rejected.
func mustID(id string) string {
	canon, err := canonicalID(id)
	if err != nil {
		logf("devtool: %v", err)
		os.Exit(2)
	}
	return canon
}

// canonicalID normalizes a task reference to its conversational number (digits
// only), stripping an optional leading '#'. It rejects any other form (ULID,
// id-prefix, empty) — that rejection is the whole point (see mustID's doc).
func canonicalID(id string) (string, error) {
	id = strings.TrimPrefix(id, "#")
	if id == "" {
		return "", fmt.Errorf("missing <id>")
	}
	for _, r := range id {
		if r < '0' || r > '9' {
			return "", fmt.Errorf("task ref %q must be the conversational number (digits only, e.g. 696 or #696) — not a ULID or id-prefix, so the worktree/branch name stays canonical and a double-start is caught", id)
		}
	}
	return id, nil
}

// paths resolves the main repo root and the per-task worktree dir for id. The
// worktree lives OUTSIDE the repo, in a sibling `<repo-name>-worktrees/` dir, so
// it has no repo `.amenbo` in its ancestry (see the package doc).
func paths(id string) (root, base, worktree string, err error) {
	cwd, err := os.Getwd()
	if err != nil {
		return
	}
	root, err = gitRoot(cwd)
	if err != nil {
		return "", "", "", fmt.Errorf("not inside a git repository: %w", err)
	}
	base = filepath.Join(filepath.Dir(root), filepath.Base(root)+"-worktrees")
	worktree = filepath.Join(base, id)
	return
}

func branchName(id string) string { return "task/" + id }

// verifyReserved confirms the backlog holds the task in_progress before any work is
// spent on a worktree — double-work is guarded by `status` alone, so status ==
// in_progress is the whole check. The reservation is facet-blind: any session that
// finds the task todo can take it, and in_progress is the "someone is on it" signal.
func verifyReserved(id string, t task) error {
	if t.Status != "in_progress" {
		return fmt.Errorf("task %s is %q, not in_progress (reserve failed?)", id, t.Status)
	}
	return nil
}

// existingWorktreeError names WHICH accident a pre-existing worktree is, because the
// two look identical on disk and read the same to whoever hits the refusal. A worktree
// on a task the backlog holds in_progress is another session at work: the refusal has
// to say so, or it reads as the harmless one — the stale worktree of a task already
// handed back — and gets waved past. It is only advisory: the backlog is the authority
// on who reserved what, and a task can legitimately change hands mid-worktree.
func existingWorktreeError(root, id, worktree string) error {
	t, err := show(root, id)
	if err != nil {
		return worktreeConflict(id, worktree, "backlog unreadable")
	}
	return worktreeConflict(id, worktree, t.Status)
}

// worktreeConflict is the refusal itself, split from the lookup so both arms are
// testable. in_progress is the one status that means someone else is working here.
func worktreeConflict(id, worktree, status string) error {
	if status == "in_progress" {
		return fmt.Errorf(`ANOTHER SESSION IS PROBABLY ON TASK %s — hands off.

  %s exists and the backlog holds the task in_progress.

  Do not look inside it, do not judge whether it is stale, do not delete it, and do
  not ask whether to take it over. Take a different task (`+"`amenbo agent --json`"+`).

  Only if you know the worktree is your own leftover: `+"`devtool task finish %s`"+`.`, id, worktree, id)
	}
	return fmt.Errorf(`%s already exists, and the backlog does not hold task %s in_progress (status: %s).

  That reads as a worktree you left behind. Tear it down before starting again:
  `+"`devtool task finish %s`"+`.`, worktree, id, status, id)
}

// taskStart reserves the task (todo→in_progress, which is itself the whole
// double-work guard), verifies the backlog agrees, then adds the worktree on a fresh
// branch, warms the GUI app's node_modules for it and seeds the app-data of the task's
// own throwaway dev GUI. It ends by printing a human summary and the task's context to
// stderr and an eval-able `cd` to stdout, so a caller can
// `eval "$(devtool task start <id>)"` to enter the worktree. The context
// is front-loaded here because reserve time is the moment before coding, where it
// cannot be skipped by reading notes alone; like the npm install, it is best-effort
// and never fails a start whose worktree and branch already exist.
func taskStart(id, base string, noReserve, noDeps bool) error {
	root, wtBase, worktree, err := paths(id)
	if err != nil {
		return err
	}
	if _, err := os.Stat(worktree); err == nil {
		return existingWorktreeError(root, id, worktree)
	}

	if !noReserve {
		if _, err := setStatus(root, id, "in_progress"); err != nil {
			var ae *amErr
			if errors.As(err, &ae) && ae.Code == "already_reserved" {
				return fmt.Errorf(`ANOTHER SESSION RESERVED TASK %s FIRST — hands off.

  The reservation is a compare-and-swap: it only takes from todo, and this one
  did not. Someone else is on this task.

  Take a different task (`+"`amenbo agent --json`"+`). Do not reserve it by another
  route, and do not start work on it here.`, id)
			}
			return fmt.Errorf("reserve: %w", err)
		}
	}
	t, err := show(root, id)
	if err != nil {
		return fmt.Errorf("verify: %w", err)
	}
	if err := verifyReserved(id, t); err != nil {
		return err
	}

	if branchExists(root, branchName(id)) {
		return fmt.Errorf("branch %s already exists — finish or delete it first", branchName(id))
	}
	if err := os.MkdirAll(wtBase, 0o755); err != nil {
		return err
	}
	if err := worktreeAdd(root, worktree, branchName(id), base); err != nil {
		return fmt.Errorf("worktree add: %w", err)
	}

	if !noDeps {
		ensureAppDeps(worktree)
	}
	provisionTaskDevGUI(worktree, id)

	logf("✓ task %s ready: %s", id, t.Title)
	logf("  dev env : %s  (branch %s)", worktree, branchName(id))
	logf("            code/build/test here; debug-verify with `make verify`")
	logf("  backlog : run amenbo (status/comment/done) from the MAIN repo: %s", root)
	if ctx, err := showHuman(root, id); err == nil {
		if ctx = strings.TrimRight(ctx, "\n"); ctx != "" {
			logf("  context : read before coding (notes / linked decisions / latest comments) —")
			for _, line := range strings.Split(ctx, "\n") {
				logf("    %s", line)
			}
		}
	}
	fmt.Printf("cd %s\n", shellQuote(worktree))
	return nil
}

// taskFinish tears the worktree and its branch down, refusing unless the worktree is
// clean and the branch is merged into base (force overrides both). `git worktree
// remove` deletes the dir, but a leftover is swept defensively afterwards, and the
// sibling base dir is pruned once it is empty — a non-empty error there means another
// task's worktree still lives in it, so it is left alone. The task's throwaway dev GUI
// goes with it, bundle and app-data both: those live outside the worktree, so this is
// the only place they are ever reclaimed.
func taskFinish(id, base string, force, rel bool) error {
	root, wtBase, worktree, err := paths(id)
	if err != nil {
		return err
	}
	if _, err := os.Stat(worktree); err != nil {
		return fmt.Errorf("no worktree for task %s (%s missing)", id, worktree)
	}

	if !force {
		clean, err := isClean(worktree)
		if err != nil {
			return fmt.Errorf("check worktree: %w (use --force to override)", err)
		}
		if !clean {
			return fmt.Errorf("worktree %s has uncommitted changes — commit them or use --force", worktree)
		}
		merged, err := isMerged(root, branchName(id), base)
		if err != nil {
			return err
		}
		if !merged {
			return fmt.Errorf("branch %s is not merged into %s — merge it or use --force", branchName(id), base)
		}
	}

	if err := worktreeRemove(root, worktree, force); err != nil {
		return fmt.Errorf("worktree remove: %w", err)
	}
	if err := branchDelete(root, branchName(id), force); err != nil {
		return fmt.Errorf("branch delete: %w", err)
	}
	if rel {
		if err := unreserve(root, id); err != nil {
			logf("devtool: warning: returning the task to todo failed: %v", err)
		}
	}
	if err := os.RemoveAll(worktree); err != nil {
		return fmt.Errorf("remove %s: %w", worktree, err)
	}
	_ = os.Remove(wtBase)

	logf("✓ torn down task %s (worktree + branch %s removed)", id, branchName(id))
	removeTaskDevGUI(id)
	if !rel {
		t, err := show(root, id)
		status := t.Status
		if err != nil {
			status = ""
		}
		if note := leftoverReservationNote(id, status); note != "" {
			logf("%s", note)
		}
	}
	return nil
}

// leftoverReservationNote is the teardown's parting note, split from the lookup so both
// arms are testable. Teardown removes the worktree, never the reservation — but the usual
// route here is a task already closed with `amenbo task done`, and telling that session
// its status "was left as-is" states the opposite of what the backlog holds. So the note
// fires on in_progress only, which is the one status that is genuinely left dangling: a
// reservation no worktree points at any more, out of every mailbox and visible to no one.
// An unreadable status ("") keeps the note — not knowing is a reason to say something.
func leftoverReservationNote(id, status string) string {
	if status != "" && status != "in_progress" {
		return ""
	}
	return fmt.Sprintf("  note: the task's in_progress status was left as-is (use --reset, or `amenbo task done %s`)", id)
}

// shellQuote single-quotes a path for safe eval in a POSIX shell.
func shellQuote(s string) string {
	return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'"
}
