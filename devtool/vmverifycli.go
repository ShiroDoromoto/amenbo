package main

import (
	"fmt"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
)

// `devtool vm verify cli …` — run the pre-distribution CLI set (`verify-all`) inside the VM, against
// the CLI the release ships, with the real agents out of the way for the length of the run.
//
// **Why the guest, and why out of the way.** A road opened with `can-start` writes stand-ins for the
// catalogued agents into a directory it puts in front of the `PATH`, then asks a pane's own shell —
// login *and* interactive zsh — which program answers to each name
// (`nothing_else_answers`, `verification/cli/src/domain/workspace.rs`). A profile read after that
// directory is handed over can put a real `claude` or `codex` back in front, and the premise then
// stops, naming the program that won. That is true of the maintainer's Mac and of the clone alike:
// `vm up` seeds the real Claude Code into it for the one screen road that needs the product. For
// v32.0.0 the three programs below were moved aside by hand in the guest before `verify-all` was
// run; this is that, written down.
//
// **They come back when the run ends**, red, green or interrupted — the screen road that reads a
// picture needs the real `claude` standing where it was. A run cut off before it could put them back
// leaves them aside under the same name with vmAgentAside on the end; the next `vm verify cli` puts
// them back when it ends, and `vm verify run` puts them back before it starts a road — and then moves
// codex aside again for the length of that road (vmRoadAgents).
//
// The harness is not changed, the same bargain `vm verify install` strikes: it is built here with
// `--release`, sent, and run in there with `--bin` naming the shipped CLI.
const (
	// vmVerifyAllBin is the CLI harness in the guest, beside the screen one.
	vmVerifyAllBin = vmGuestHome + "/verify-all"
	// vmVerifyShippedCLI is the bare CLI taken out of the shipped .pkg. Bare rather than the
	// installed bundle's: a run of `verify-all` is pointed at a binary, and an Amenbo.app lying
	// about with the production bundle id is a candidate for the hourly tick (scripts/extract-shipped-cli.sh).
	vmVerifyShippedCLI = vmGuestHome + "/amenbo-shipped"
	// vmAgentAside is put on the end of each agent's path while it is out of the way. A name the
	// shell does not look up, next to where it came from, so putting it back is one `mv`.
	vmAgentAside = ".verify-cli-aside"
)

// vmGuestAgents are the programs in the guest that answer to a catalogued agent's name ahead of a
// run's stand-ins. `claudeGuestBin` is the one `vm up` seeds; the other two are the golden's own
// (`vm up` removes the first, but not when the host has no `claude` to seed in its place).
var vmGuestAgents = []string{
	claudeGoldenStandIn,
	claudeGuestBin,
	vmGuestCodex,
}

// vmGuestCodex is the golden's own `codex`.
const vmGuestCodex = "/opt/homebrew/bin/codex"

// vmRoadAgents are the ones a screen road has out of the way. Only codex: the road that reads a
// picture asks the real `claude` (`claudeGuestBin`) and opens no `can-start`, and `vm up` has already
// removed `claudeGoldenStandIn` wherever it seeded that `claude`.
var vmRoadAgents = []string{vmGuestCodex}

// vmVerifyCLI sends the shipped CLI, the harness, the scenarios and the fixtures, moves the agents
// aside, runs `verify-all` and answers with the code it ended with — which is the roll-up a release
// gate reads, so it is devtool's own code too.
func vmVerifyCLI(pkg, fromRun string, scenarios []string, asJSON bool) (int, error) {
	ip, err := vmEnsureUp()
	if err != nil {
		return 0, err
	}
	// Moving the agents aside takes the real `claude` from under the one screen road that needs it,
	// so the two are not run at once.
	if vmRoadWalking(ip) {
		return 0, fmt.Errorf("a pre-distribution road is walking in %s — `devtool vm verify cli` would move the real agents from under it (`devtool vm verify log` reads where it stands, `devtool vm verify stop` ends it)", vmCloneName)
	}
	root := mustTreeRoot()
	guestScenarios, err := vmVerifyCLIScenarios(filepath.Join(root, "verification", "scenarios"), scenarios)
	if err != nil {
		return 0, err
	}
	pkg, err = vmVerifyPkgForGuest(ip, root, pkg, fromRun)
	if err != nil {
		return 0, err
	}

	work, err := os.MkdirTemp("", "amenbo-verify-cli-")
	if err != nil {
		return 0, err
	}
	defer os.RemoveAll(work)
	shipped := filepath.Join(work, filepath.Base(vmVerifyShippedCLI))
	if _, err := run(root, filepath.Join(root, "scripts", "extract-shipped-cli.sh"), pkg, shipped); err != nil {
		return 0, fmt.Errorf("taking the CLI out of %s: %w", filepath.Base(pkg), err)
	}

	// The harness, built here for there with `--release`, the way `vm verify install` builds its own.
	// The CLI it drives is the shipped one above: the driver refuses a binary the release workflow
	// did not produce (verification/cli/src/shipped.rs).
	logf("  verify  : building the harness")
	if _, err := run(root, "cargo", "build", "--release", "--manifest-path",
		filepath.Join(root, "verification", "Cargo.toml"), "-p", "amenbo-verify-cli", "--bin", "verify-all"); err != nil {
		return 0, fmt.Errorf("build verify-all: %w", err)
	}

	send := []string{
		shipped,
		filepath.Join(root, "verification", "target", "release", "verify-all"),
		filepath.Join(root, "verification", "scenarios"),
		filepath.Join(root, "verification", "fixtures"),
	}
	if err := vmPush(send, vmGuestHome+"/"); err != nil {
		return 0, err
	}
	hostFixtures := filepath.Join(root, "verification", "fixtures")
	if _, err := sshRun(ip, vmFixturesLinkCommand(hostFixtures)); err != nil {
		return 0, fmt.Errorf("putting the fixtures where the harness looks for them: %w", err)
	}

	// Caught rather than left to kill this process: a Ctrl-C reaches the ssh below through the
	// terminal's process group and ends the run there, and what has to happen after that is the
	// deferred put-back. Caught, not ignored — an ignored signal is inherited by the ssh.
	sigs := make(chan os.Signal, 1)
	signal.Notify(sigs, os.Interrupt, syscall.SIGTERM)
	defer signal.Stop(sigs)

	logf("  verify  : moving the agents aside in %s", vmCloneName)
	if _, err := sshRun(ip, vmAgentsAsideCommand(vmGuestAgents)); err != nil {
		return 0, fmt.Errorf("moving the agents aside: %w", err)
	}
	defer func() {
		if _, err := sshRun(ip, vmAgentsBackCommand(vmGuestAgents)); err != nil {
			logf("  verify  : warning — the agents did not go back (%v); the next `devtool vm verify cli` or `vm verify run` puts them back", err)
			return
		}
		logf("  verify  : the agents are back in %s", vmCloneName)
	}()

	logf("  verify  : running verify-all in %s", vmCloneName)
	return runThrough("", nil, "ssh", sshArgs(ip, vmVerifyAllCommand(guestScenarios, asJSON))...)
}

// vmVerifyCLIArgs splits the words after `vm verify cli` into the build and the scenarios. The build
// is the one that ends in `.pkg`, the way every other `vm verify` command takes it; the rest are
// scenarios.
func vmVerifyCLIArgs(words []string) (pkg string, scenarios []string, err error) {
	for _, w := range words {
		switch {
		case w == "":
		case strings.HasSuffix(w, ".pkg") && pkg != "":
			return "", nil, fmt.Errorf("vm verify cli takes one .pkg, got %s and %s", pkg, w)
		case strings.HasSuffix(w, ".pkg"):
			pkg = w
		default:
			scenarios = append(scenarios, w)
		}
	}
	return pkg, scenarios, nil
}

// vmVerifyCLIScenarios turns the scenarios a caller named into their paths in the guest. A path in
// the tree or a bare id both name the file of that name under `scenarios/`, which is the one place
// they are sent to; one that is not there is refused here rather than a minute later in the guest.
// None is none: `verify-all` then reads the whole directory, which is what a release runs.
func vmVerifyCLIScenarios(dir string, named []string) ([]string, error) {
	guest := make([]string, 0, len(named))
	for _, n := range named {
		base := filepath.Base(n)
		if ext := filepath.Ext(base); ext != ".yaml" && ext != ".yml" {
			base += ".yaml"
		}
		if _, err := os.Stat(filepath.Join(dir, base)); err != nil {
			return nil, fmt.Errorf("no scenario %s in %s", base, dir)
		}
		guest = append(guest, vmGuestHome+"/scenarios/"+base)
	}
	return guest, nil
}

// vmFixturesLinkCommand puts the fixtures where the harness will look for them. The CLI driver takes
// them from the path it was compiled at — this checkout's — and `verify-all` has nothing to say
// otherwise, so that path is made in the guest and pointed at the fixtures sent. `sudo` because it
// is a path under somebody else's home; the clone is thrown away with it.
func vmFixturesLinkCommand(hostFixtures string) string {
	return fmt.Sprintf("sudo -n mkdir -p %s && sudo -n ln -sfn %s %s",
		shq(filepath.Dir(hostFixtures)), shq(vmVerifyFixtures), shq(hostFixtures))
}

// vmAgentsAsideCommand moves each agent to its aside name. One already aside is left there — a run
// cut off before it put them back — and one that is not there is nothing to move.
func vmAgentsAsideCommand(agents []string) string {
	return vmAgentsMoveCommand(agents, false)
}

// vmAgentsBackCommand is the mirror: each aside name goes back to where it came from, unless
// something has been put there since.
func vmAgentsBackCommand(agents []string) string {
	return vmAgentsMoveCommand(agents, true)
}

func vmAgentsMoveCommand(agents []string, back bool) string {
	lines := []string{"set -e"}
	for _, a := range agents {
		from, to := shq(a), shq(a+vmAgentAside)
		if back {
			from, to = to, from
		}
		// `-L` beside `-e`: the seeded `claude` is a symlink, and one whose target is gone answers
		// false to `-e` while still being a file to move and to put back.
		lines = append(lines, fmt.Sprintf(
			"if { [ -e %s ] || [ -L %s ]; } && [ ! -e %s ] && [ ! -L %s ]; then sudo -n mv %s %s; fi",
			from, from, to, to, from, to))
	}
	return strings.Join(lines, "\n")
}

// vmVerifyAllCommand is the line the guest runs. From the home directory, because `verify-all` with
// no scenario named reads `scenarios/` under where it stands.
func vmVerifyAllCommand(scenarios []string, asJSON bool) string {
	words := []string{"cd", shq(vmGuestHome), "&&", shq(vmVerifyAllBin)}
	for _, s := range scenarios {
		words = append(words, shq(s))
	}
	words = append(words, "--bin", shq(vmVerifyShippedCLI))
	if asJSON {
		words = append(words, "--json")
	}
	return strings.Join(words, " ")
}
