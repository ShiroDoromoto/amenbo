// Command devtool is amenbo's portable developer-support CLI: a single static
// Go binary (no runtime, no venv) that can be dropped into any project. It gives
// a task the throwaway dev GUI it is verified in, and it stands up a fake outside
// world (fixtures.go) for verifying the GUI against answers the real one will not
// give on demand.
//
// The checkout a task is written in is a git worktree outside the repo, cut by
// amenbo's official `worktree` plugin:
//
//	<repo>/../<repo-name>-worktrees/<id>/   git worktree checkout on task/<id>
//
// Outside-the-repo is what makes it a pure development environment. With no repo
// `.amenbo` in its ancestry, amenbo commands run there (e.g. running the dev
// build for debug verification) cannot reach the real backlog — they fall to an
// isolated/throwaway store. That keeps two concerns physically apart:
//
//   - Project management (status/comment/done) → the PROD `amenbo` binary
//     run from the MAIN repo, against the real backlog.
//   - Debug verification (does my code work) → the worktree's dev build against
//     a throwaway store (e.g. `make verify`), inside the outside worktree.
//
// devtool holds the half of that isolation git cannot: a GUI bundle is installed
// machine-wide, so a worktree cannot contain one. A task gets its own throwaway
// dev GUI instead — its own identifier and app-data, seeded from the shared dev
// store. devtool seeds, drives and reclaims that instance (devgui.go); the
// Makefile builds it, so only the tasks that look at a GUI pay for one.
//
// The backlog is amenbo's and git is the plugin's: devtool speaks to neither.
// Beyond the instance's app-data it provisions no amenbo store — isolation comes
// from the worktree living outside the repo plus `make verify`'s mktemp store.
package main

import (
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
	case "devgui":
		devGUICmd(args[1:])
	case "fixtures":
		fixturesCmd(args[1:])
	case "plugin":
		pluginCmd(args[1:])
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
  devtool devgui seed      <id>
  devtool devgui cli       <id> [--no-build] -- <amenbo args…>
  devtool devgui pid       [<id>] [--front]
  devtool devgui shot      [<id>] [--no-front]
  devtool devgui rm        <id>
  devtool devgui sweep     [--yes]
  devtool fixtures refresh [--catalog <url|path>] [--repo owner/name]
  devtool fixtures gui     [--fail <face>=<status|timeout>] [--port n] [--app path] [--no-launch]
  devtool plugin round     --manifest <path.json> [--program path] [--set k=v] [--events list] [--keep]

devgui seed  clone the shared dev store into the app-data of the task's own
             throwaway dev GUI, so the instance opens on the setup grown in the
             shared app rather than an empty one. 'make install-gui-dev
             AMB-T-ID=<id>' runs this and builds the bundle; an app-data already
             sitting there is left alone.
devgui cli   run an amenbo command against the store the task's own dev GUI
             reads, so a screen can be given something to show. The CLI is the
             worktree's own build (rebuilt first unless --no-build) pointed at
             that store with AMENBO_HOME, not a second CLI built for the task:
             what the app-data name fixes at build time is a directory, and
             that names the same one at run time. Arguments go after '--'.
devgui pid   print the pid of a running dev GUI, for the screen tool
             ('scripts/screen.swift') to aim at. The front window is whichever
             app is in front, which is rarely the one being verified; devtool
             matches on the bundle a process was executed out of, which names one
             instance exactly (each dev build also carries its own executable
             name -- 'amenbo-app-dev', 'amenbo-app-dev-<id>' -- against prod's
             'amenbo-app').
             Without an <id> it answers for the dev GUI this checkout launches
             (a task worktree's own instance ahead of the shared app); --front
             brings it forward first, since a window behind a Space cannot be
             found at all.
devgui shot  capture that instance's own window and print the png's path. The
             screen tool is handed the pid and hands back the file: which window
             it shot, and the id it shot by, stay in there, so nothing here can
             aim a click by a rectangle -- press what a thing is called
             ('swift scripts/screen.swift click-named <pid> <name>') instead. It
             fronts the instance first, since a window behind a Space cannot be
             found at all; --no-front leaves the front alone.
devgui rm    delete one task's instance — the installed bundle and its app-data
             both. They live outside the worktree, so removing the checkout
             leaves them behind; run this when the task is finished.
devgui sweep list every per-task dev GUI on this machine and say which ones no
             worktree claims any more (a session that ended without 'devgui rm'
             leaves ~38MB of bundle plus a store behind). Reports only; --yes
             reclaims the orphans. An instance a worktree still owns is never
             touched, and if git cannot list the worktrees the sweep refuses
             rather than guess.
fixtures     a fake outside world for GUI verification. 'refresh' captures the
             catalog, GitHub's answers and latest.json from the real world (they
             are copies, never written by hand); 'gui' serves them and starts the
             dev GUI pointed at them. --fail makes a face answer 429/500/404, or
             never answer at all — the responses the real API will not produce on
             demand, and so the branches nothing else reaches.
plugin round run one plugin through one lap of a store that is thrown away
             afterwards: raise it, install the build by hand the way a plugin
             repo's own 'make install' does, fill in what its manifest declares,
             open its gate, fire the events an AI's writes fire, empty the queues
             and then show what the plugin was handed and how each run ended.
             Nothing is asserted — the receiving side (a webhook to stand in for,
             a checkout to look at) is the plugin author's. Without --program it
             installs devtool's stand-in, which records the documents it is
             handed, so a payload can be read without writing a plugin for it.`)
}

// parseAroundID parses `fs` over args in which the task id may sit on either side of
// the flags, and returns the id it found ("" if there is none).
//
// Go's flag package stops at the first non-flag word, so one `fs.Parse` reads only the
// flags that lead: in `devgui cli <id> --no-build` the flag goes unread. Peeling the
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

// devGUICmd dispatches the `devgui` subcommands: the two that stand one task's instance up and take
// it down, the seeding CLI, the pid lookup, the window shot, and the report the machine-wide reclaim
// is the review for. `pid` and `shot` take an optional id (without one they answer for the checkout
// in hand); `sweep` takes none — the point there is the instances nobody named.
func devGUICmd(args []string) {
	if len(args) == 0 {
		usage()
		os.Exit(2)
	}
	sub := args[0]

	// One id per invocation: an instance's bundle and app-data are named after it, so a second
	// one is a typo, not a batch.
	refuseExtra := func(extra []string) {
		if len(extra) > 0 {
			logf("devtool: devgui %s takes one id, got extra argument(s): %s", sub, strings.Join(extra, " "))
			usage()
			os.Exit(2)
		}
	}

	switch sub {
	case "seed":
		fs := flag.NewFlagSet("devgui seed", flag.ExitOnError)
		id, extra := parseAroundID(fs, args[1:])
		refuseExtra(extra)
		id = mustID(id)
		if err := devGUISeed(id); err != nil {
			logf("devtool: %v", err)
			os.Exit(1)
		}
	case "cli":
		// The amenbo arguments are handed over after `--`, so a flag of theirs is never read as
		// one of ours (see splitDoubleDash).
		head, argv, ok := splitDoubleDash(args[1:])
		if !ok {
			logf("devtool: devgui cli passes its arguments to amenbo after `--`, e.g. `devtool devgui cli 696 -- task list`")
			os.Exit(2)
		}
		fs := flag.NewFlagSet("devgui cli", flag.ExitOnError)
		noBuild := fs.Bool("no-build", false, "run the CLI already built in the worktree, without rebuilding it")
		id, extra := parseAroundID(fs, head)
		refuseExtra(extra)
		id = mustID(id)
		code, err := taskCLI(id, *noBuild, argv)
		if err != nil {
			logf("devtool: %v", err)
			os.Exit(1)
		}
		os.Exit(code)
	case "rm":
		fs := flag.NewFlagSet("devgui rm", flag.ExitOnError)
		id, extra := parseAroundID(fs, args[1:])
		refuseExtra(extra)
		id = mustID(id)
		removeTaskDevGUI(id)
	case "pid":
		fs := flag.NewFlagSet("devgui pid", flag.ExitOnError)
		front := fs.Bool("front", false, "bring the instance to the front before printing its pid")
		id, extra := parseAroundID(fs, args[1:])
		if len(extra) > 0 {
			logf("devtool: devgui pid takes one id at most, got extra argument(s): %s", strings.Join(extra, " "))
			usage()
			os.Exit(2)
		}
		if id != "" {
			id = mustID(id)
		}
		if err := devGUIShowPID(id, *front); err != nil {
			logf("devtool: %v", err)
			os.Exit(1)
		}
	case "shot":
		// Fronting is the default here and opt-in for `pid`: a window behind another Space is not
		// on-screen, so it cannot be located at all, and one nobody fronted is shot with whatever
		// is over it. `--no-front` is for the caller who is capturing a state that fronting would
		// disturb.
		fs := flag.NewFlagSet("devgui shot", flag.ExitOnError)
		noFront := fs.Bool("no-front", false, "shoot the instance where it is, without bringing it to the front")
		id, extra := parseAroundID(fs, args[1:])
		if len(extra) > 0 {
			logf("devtool: devgui shot takes one id at most, got extra argument(s): %s", strings.Join(extra, " "))
			usage()
			os.Exit(2)
		}
		if id != "" {
			id = mustID(id)
		}
		if err := devGUIShot(id, !*noFront); err != nil {
			logf("devtool: %v", err)
			os.Exit(1)
		}
	case "sweep":
		fs := flag.NewFlagSet("devgui sweep", flag.ExitOnError)
		apply := fs.Bool("yes", false, "actually remove the orphans (without it, only report)")
		fs.Parse(args[1:])
		if fs.NArg() > 0 {
			logf("devtool: devgui sweep takes no arguments, got: %s", strings.Join(fs.Args(), " "))
			usage()
			os.Exit(2)
		}
		if err := devGUISweep(*apply); err != nil {
			logf("devtool: %v", err)
			os.Exit(1)
		}
	default:
		logf("devtool: unknown devgui subcommand %q", sub)
		usage()
		os.Exit(2)
	}
}

// mustID validates the task reference and returns its canonical form: the
// conversational number (digits only), with an optional leading '#' stripped.
// Requiring the number — not a ULID or an id-prefix — is what makes one task name
// exactly one instance and one checkout: paths() and the devgui names derive
// themselves verbatim from this string, so two references to the same task in
// different forms would otherwise address two differently-named app-data
// directories, and a sweep would read one of them as a stranger's. Pinned to the
// number, every route to a task lands on the same one.
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
			return "", fmt.Errorf("task ref %q must be the conversational number (digits only, e.g. 696 or #696) — not a ULID or id-prefix, so one task names one checkout and one dev GUI instance", id)
		}
	}
	return id, nil
}

// paths resolves the main repo root and the per-task worktree dir for id. The worktree lives
// OUTSIDE the repo, in a sibling `<repo-name>-worktrees/` dir, so it has no repo `.amenbo` in its
// ancestry (see the package doc). The layout is the official `worktree` plugin's, read back here
// rather than cut: an instance belongs to a checkout, and finding one means naming where it sits.
func paths(id string) (root, worktree string, err error) {
	cwd, err := os.Getwd()
	if err != nil {
		return
	}
	root, err = gitRoot(cwd)
	if err != nil {
		return "", "", fmt.Errorf("not inside a git repository: %w", err)
	}
	worktree = filepath.Join(filepath.Dir(root), filepath.Base(root)+"-worktrees", id)
	return
}
