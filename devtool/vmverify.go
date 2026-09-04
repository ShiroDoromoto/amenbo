package main

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// `devtool vm verify …` — run the pre-distribution GUI harness inside the VM, rather than on the
// screen somebody is working on.
//
// The harness itself is not touched, and nothing here is a second copy of what it does. It launches
// the shipped bundle, holds the pid that launch answered with, stands up the world a scenario
// declares and shoots one screen per step — all of which goes on being true when it runs in the
// guest, which is the reason to move the harness rather than to drive the guest from outside: a pid
// held on this side would name a process on that one.
//
// What is added is the four things a run in there needs and a run here does not: the shipped build
// and the harness have to be sent and installed, the road has to be started against something that
// can still be written to a step at a time, and the evidence has to be brought back.
//
// **The steps come from a file that is appended to, not from a pipe somebody holds.** The harness
// waits for a line on stdin before every step, and a writer cannot be held open across separate
// commands — so stdin is `tail -n 0 -f` over a file, and advancing a step is one more line at the
// end of it. Nothing has to stay alive between two commands, the count of lines sent is on disk to
// be read back, and a run that has ended takes the tail down with it.
const (
	// vmVerifyBin, vmVerifySteps, vmVerifyLog and vmVerifyEvidence are where a run's four pieces
	// live in the guest. Fixed paths, because `run`, `step`, `log` and `pull` are four separate
	// commands with nothing between them but the guest's own disk.
	vmVerifyBin      = vmGuestHome + "/verify-gui"
	vmVerifySteps    = vmGuestHome + "/verify-gui-steps.txt"
	vmVerifyLog      = vmGuestHome + "/verify-gui.log"
	vmVerifyEvidence = vmGuestHome + "/verify-gui-evidence"
	// vmVerifyFixtures is where `install` lands the fixtures a premise copies from. The harness
	// resolves them from its own compile-time path when nobody says otherwise, and that path is
	// this machine's — so the run is told where they really are, the way it is told about the
	// screen tool.
	vmVerifyFixtures = vmGuestHome + "/fixtures"
	// vmScreenSource is the screen tool as source, beside the compiled one `vm screen` sends.
	// The harness runs the tool as `swift <path>`, which is a path to source; the compiled binary
	// is for the operator's own moves. One tree, two shapes, and both from the same file.
	vmScreenSource = vmGuestHome + "/screen.swift"
	// vmGuestApp is where a per-user install of the unified installer puts the bundle.
	vmGuestApp = vmGuestHome + "/Applications/Amenbo.app"
	// vmGuestCLI is the symlink that install's postinstall puts on the user's PATH — and the one
	// thing in the guest that answers with the version standing there.
	vmGuestCLI = vmGuestHome + "/.local/bin/amenbo"
	// vmGuestSystemApp and vmGuestSystemCLI are where a release from before the per-user move left
	// its copy: root-owned under /Applications, with the CLI ahead of ~/.local/bin on the stock
	// PATH. A new build's postinstall keys on exactly these two to offer the one-time migration
	// (`scripts/build-pkg-mac.sh`), which is what `seed --system-wide` leaves behind.
	vmGuestSystemApp = "/Applications/Amenbo.app"
	vmGuestSystemCLI = "/usr/local/bin/amenbo"
	// vmReleaseArtifact is the CI artifact the mac build is published as.
	vmReleaseArtifact = "dist-macos"
)

// vmVerifyCmd dispatches `vm verify`: one command to put a build in there, and four to walk a road
// with it.
func vmVerifyCmd(args []string) {
	if len(args) == 0 {
		usage()
		os.Exit(2)
	}
	sub := args[0]
	fail := func(err error) {
		if err != nil {
			logf("devtool: %v", err)
			os.Exit(1)
		}
	}

	switch sub {
	case "seed":
		fs := flag.NewFlagSet("vm verify seed", flag.ExitOnError)
		fromRun := fs.String("from-run", "", "download the mac build from this CI run instead of taking a path")
		systemWide := fs.Bool("system-wide", false, "leave it the way a release from before the per-user move did: /Applications, root-owned, CLI at /usr/local/bin")
		pkg, extra := parseAroundID(fs, args[1:])
		if len(extra) > 0 {
			logf("devtool: vm verify seed takes one .pkg, got extra argument(s): %s", strings.Join(extra, " "))
			os.Exit(2)
		}
		fail(vmVerifySeed(pkg, *fromRun, *systemWide))
	case "install":
		fs := flag.NewFlagSet("vm verify install", flag.ExitOnError)
		fromRun := fs.String("from-run", "", "download the mac build from this CI run instead of taking a path")
		pkg, extra := parseAroundID(fs, args[1:])
		if len(extra) > 0 {
			logf("devtool: vm verify install takes one .pkg, got extra argument(s): %s", strings.Join(extra, " "))
			os.Exit(2)
		}
		fail(vmVerifyInstall(pkg, *fromRun))
	case "run":
		fs := flag.NewFlagSet("vm verify run", flag.ExitOnError)
		scenario, extra := parseAroundID(fs, args[1:])
		if scenario == "" || len(extra) > 0 {
			logf("devtool: vm verify run takes one scenario, e.g. `devtool vm verify run verification/scenarios/link-a-folder.yaml`")
			os.Exit(2)
		}
		fail(vmVerifyRun(scenario))
	case "step":
		fs := flag.NewFlagSet("vm verify step", flag.ExitOnError)
		note := fs.String("note", "walked", "the line sent — the harness reads a line, not its content, so this is a note to yourself")
		fs.Parse(args[1:])
		fail(vmVerifyStep(*note))
	case "log":
		fs := flag.NewFlagSet("vm verify log", flag.ExitOnError)
		lines := fs.Int("lines", 12, "how much of the tail to print")
		fs.Parse(args[1:])
		fail(vmVerifyLogTail(*lines))
	case "pull":
		fs := flag.NewFlagSet("vm verify pull", flag.ExitOnError)
		out := fs.String("out", "", "where the evidence lands (default: a fresh dir under the temp tree)")
		fs.Parse(args[1:])
		fail(vmVerifyPull(*out))
	default:
		logf("devtool: unknown vm verify subcommand %q", sub)
		usage()
		os.Exit(2)
	}
}

// ---------------------------------------------------------------------------
// install
// ---------------------------------------------------------------------------

// vmVerifyInstall puts everything a screen road needs into the guest: the shipped build, installed;
// the harness binary; the scenarios and the fixtures they reach for; and the screen tool.
//
// The build is taken as a path, or downloaded from a CI run with `--from-run`. **The artifact, never
// the release's download URL** — a release download is counted, and a development one cannot be
// subtracted afterwards. Where the bytes on disk are held against what the release published is the
// release procedure's own step, upstream of this: handing this command a path is what keeps that
// check between the download and the run.
//
// Nothing is built for the guest. Host and guest are the same architecture, so the harness compiled
// here runs there, and the guest needs neither Rust nor node.
func vmVerifyInstall(pkg, fromRun string) error {
	ip, err := vmEnsureUp()
	if err != nil {
		return err
	}
	root := mustTreeRoot()
	pkg, err = vmVerifyPkgForGuest(ip, root, pkg, fromRun)
	if err != nil {
		return err
	}

	// The harness, built here for there. `--release` because a road is walked many times and a
	// debug build spends that difference on every OCR read.
	logf("  verify  : building the harness")
	if _, err := run(root, "cargo", "build", "--release", "--manifest-path",
		filepath.Join(root, "verification", "Cargo.toml"), "-p", "amenbo-verify-gui", "--bin", "verify-gui"); err != nil {
		return fmt.Errorf("build verify-gui: %w", err)
	}

	send := []string{
		pkg,
		filepath.Join(root, "verification", "target", "release", "verify-gui"),
		filepath.Join(root, "verification", "scenarios"),
		filepath.Join(root, "verification", "fixtures"),
		filepath.Join(root, "scripts", "screen.swift"),
	}
	if err := vmPush(send, vmGuestHome+"/"); err != nil {
		return err
	}

	version, err := vmVerifyInstallPkg(ip, pkg)
	if err != nil {
		return err
	}

	// The first `swift <source>` on a machine builds a module cache and takes some twenty seconds;
	// every one after it is under a second. Paid here rather than inside the harness's own window
	// for the app to draw a window, which that first call would otherwise run out.
	logf("  verify  : warming the screen tool")
	if _, err := sshRun(ip, "swift "+vmScreenSource+" trusted"); err != nil {
		logf("  verify  : warning — the screen tool did not answer (%v); the first step will be slow or fail", err)
	}

	logf("✓ %s installed in %s — `devtool vm verify run <scenario.yaml>` walks a road", version, vmCloneName)
	return nil
}

// vmVerifyPkgForGuest resolves the build to install — the path given, or the mac artifact of a CI
// run — and refuses one the guest could install and then not start.
func vmVerifyPkgForGuest(ip, root, pkg, fromRun string) (string, error) {
	arch, err := sshRun(ip, "uname -m")
	if err != nil {
		return "", fmt.Errorf("could not ask the guest its architecture: %w", err)
	}
	arch = strings.TrimSpace(arch)
	if pkg == "" {
		if fromRun == "" {
			return "", fmt.Errorf("nothing to install — pass a .pkg, or `--from-run <run id>` to take one from that CI run's %q artifact", vmReleaseArtifact)
		}
		if pkg, err = downloadReleasePkg(root, fromRun, arch); err != nil {
			return "", err
		}
	}
	if err := refuseForeignArch(pkg, arch); err != nil {
		return "", err
	}
	return pkg, nil
}

// vmVerifyInstallPkg installs one build in the guest and answers with the version now standing
// there. The distribution enables the current user's home domain and no other, so that is the only
// target a build of ours takes.
//
// What it went on over is said first, while there is still something to say it about: a clone is cut
// from a bare macOS, so an install into one is a first install unless something was seeded ahead of
// it — and which of the two a road was walked on is read off this line afterwards.
func vmVerifyInstallPkg(ip, pkg string) (string, error) {
	guestPkg := vmGuestHome + "/" + filepath.Base(pkg)
	if standing := vmVerifyStanding(ip); standing != "" {
		logf("  verify  : over %s", standing)
	}
	logf("  verify  : installing %s", filepath.Base(pkg))
	if _, err := sshRun(ip, "installer -pkg "+guestPkg+" -target CurrentUserHomeDirectory"); err != nil {
		return "", fmt.Errorf("installing the build in the guest: %w", err)
	}
	version, err := sshRun(ip, vmGuestCLI+" --version")
	if err != nil {
		return "", fmt.Errorf("the build installed but does not answer: %w", err)
	}
	return strings.TrimSpace(version), nil
}

// vmVerifyStanding is the build the guest already carries, and where — "" for a machine carrying
// none. Both places are asked, because `seed --system-wide` leaves one in neither the per-user path
// nor on the per-user PATH.
func vmVerifyStanding(ip string) string {
	for _, cli := range []string{vmGuestCLI, vmGuestSystemCLI} {
		out, err := sshRun(ip, cli+" --version")
		if err == nil && strings.TrimSpace(out) != "" {
			return strings.TrimSpace(out) + " at " + cli
		}
	}
	return ""
}

// downloadReleasePkg takes the mac artifact of one CI run and answers with the `.pkg` in it that
// matches the guest. The repository is read off `origin` rather than written down here.
func downloadReleasePkg(root, runID, arch string) (string, error) {
	repo, err := originRepo(root)
	if err != nil {
		return "", err
	}
	dir, err := os.MkdirTemp("", "amenbo-dist-")
	if err != nil {
		return "", err
	}
	logf("  verify  : downloading %s from run %s of %s", vmReleaseArtifact, runID, repo)
	if _, err := run(root, "gh", "run", "download", runID, "--repo", repo, "--name", vmReleaseArtifact, "--dir", dir); err != nil {
		return "", fmt.Errorf("downloading the build: %w", err)
	}
	want := filepath.Join(dir, "amenbo-darwin-"+pkgArch(arch)+".pkg")
	if _, err := os.Stat(want); err != nil {
		return "", fmt.Errorf("%s carries no %s: %w", vmReleaseArtifact, filepath.Base(want), err)
	}
	return want, nil
}

// pkgArch maps what `uname -m` answers to the word a distributable is named by.
func pkgArch(arch string) string {
	if arch == "arm64" {
		return "arm64"
	}
	return "amd64"
}

// refuseForeignArch turns away a build for the other architecture, by name. Installing one succeeds
// and the app then refuses to start, which is a failure several steps away from its cause.
func refuseForeignArch(pkg, arch string) error {
	name := filepath.Base(pkg)
	other := "amd64"
	if pkgArch(arch) == "amd64" {
		other = "arm64"
	}
	if strings.Contains(name, other) {
		return fmt.Errorf("%s is the %s build and the guest is %s — send the %s one", name, other, arch, pkgArch(arch))
	}
	return nil
}

// ---------------------------------------------------------------------------
// what a build is installed over
// ---------------------------------------------------------------------------

// vmVerifySeed puts a build in the guest and stops there — no harness, no scenarios, no screen tool.
// It is what the next `install` goes on over.
//
// The clone is cut from a bare macOS, so every install into one is a first install, and the road
// most people actually walk — a version already there, being replaced under a running app — is the
// one the VM could not reach. Two commands walk it now: this one with the version they are on, then
// `install` with the one being shipped.
//
// **`--system-wide` leaves it the way a release from before the per-user move did**: the bundle in
// /Applications owned by root, the CLI symlinked into it from /usr/local/bin, and no per-user copy
// or PATH line anywhere. A build from that era is not obtainable any more, and it is not what the
// next install reads — the postinstall keys on those two paths and on nothing else about the old
// build, so the shape is the whole of it. That is the one path that asks for an admin password, and
// the only elevation in a per-user lifetime.
func vmVerifySeed(pkg, fromRun string, systemWide bool) error {
	ip, err := vmEnsureUp()
	if err != nil {
		return err
	}
	pkg, err = vmVerifyPkgForGuest(ip, mustTreeRoot(), pkg, fromRun)
	if err != nil {
		return err
	}
	if err := vmPush([]string{pkg}, vmGuestHome+"/"); err != nil {
		return err
	}
	if standing := vmVerifyStanding(ip); standing != "" {
		logf("  verify  : taking down %s", standing)
	}
	if _, err := sshRun(ip, vmVerifySeedClear); err != nil {
		return fmt.Errorf("taking down the build already in the guest: %w", err)
	}
	version, err := vmVerifyInstallPkg(ip, pkg)
	if err != nil {
		return err
	}
	if !systemWide {
		logf("✓ %s seeded in %s — `devtool vm verify install <pkg>` goes on over it", version, vmCloneName)
		return nil
	}
	logf("  verify  : moving it where a system-wide release left one")
	if _, err := sshRun(ip, vmSystemWideSeed); err != nil {
		return fmt.Errorf("leaving the build where a system-wide release would have: %w", err)
	}
	logf("✓ %s seeded in %s as a system-wide install (%s, %s) — the next `install` offers to retire it",
		version, vmCloneName, vmGuestSystemApp, vmGuestSystemCLI)
	return nil
}

// vmVerifySeedClear is what a seed takes down before it puts its own build in.
//
// **An installer skips a payload whose bundle is older than the one already installed** — pkgbuild
// marks the component version-checked — so a seed that only ran the installer would leave the newer
// build standing, and the overlay would then be walked against the very version the seed was meant
// to replace. Measured in the guest on 2026-09-05: 22.2.0 seeded over 22.3.0 answered 22.3.0.
//
// A seed says which version this machine is on, so what it is not on goes first — the per-user pair
// and the system-wide pair both, since either would be found by what installs next. The store is
// left alone: a machine on that version has one, and it is what a migration is rehearsed against.
const vmVerifySeedClear = `pkill -f ` + vmGuestApp + ` 2>/dev/null || true
sleep 1
rm -rf ` + vmGuestApp + ` ` + vmGuestCLI + `
sudo -n rm -rf ` + vmGuestSystemApp + `
sudo -n rm -f ` + vmGuestSystemCLI

// vmSystemWideSeed turns the per-user install just made into what a machine that never took one
// looks like. Every line is something that machine had, or did not:
//
//   - the app is quit first — the seed's own postinstall launched it, and it is about to be moved
//     out from under itself
//   - the bundle sits in /Applications owned by root, which is why retiring it needs a password
//   - /usr/local/bin/amenbo resolves into THAT bundle: the postinstall retires the link only when it
//     does, never a sibling channel's
//   - neither the per-user bundle nor the per-user CLI is left, because that machine had neither
//   - the installer's PATH lines are taken back out, so the next install writes them for the first
//     time — they carry a marker, and a second run appends nothing over it
//
// It ends by asking the system copy its version: a seed that left something that cannot run is a
// road walked against nothing.
const vmSystemWideSeed = `set -e
pkill -f ` + vmGuestApp + ` 2>/dev/null || true
sleep 1
sudo -n rm -rf ` + vmGuestSystemApp + `
sudo -n mv ` + vmGuestApp + ` ` + vmGuestSystemApp + `
sudo -n chown -R root:wheel ` + vmGuestSystemApp + `
sudo -n mkdir -p /usr/local/bin
sudo -n ln -sf ` + vmGuestSystemApp + `/Contents/MacOS/amenbo ` + vmGuestSystemCLI + `
rm -f ` + vmGuestCLI + `
for rc in ` + vmGuestHome + `/.zprofile ` + vmGuestHome + `/.bash_profile; do
  [ -f "$rc" ] || continue
  awk '/# added by amenbo installer/{drop=2} drop>0{drop--; next} {print}' "$rc" > "$rc.seeded" && mv "$rc.seeded" "$rc"
done
` + vmGuestSystemCLI + ` --version`

// ---------------------------------------------------------------------------
// walking a road
// ---------------------------------------------------------------------------

// vmVerifyRun starts one scenario in the guest and returns once the harness has handed over its
// first step. It does not wait for the run: a road is walked by somebody, and that somebody is the
// caller of `step` between one hand-over and the next.
//
// **`--screen` and `--fixtures` are passed explicitly.** The harness resolves both relative to its
// own executable, which in the guest is a path on this side of the machine. Without the first a run
// fails a minute in, having launched an app and photographed nothing; without the second a road that
// copies a fixture fails before that, standing up its world.
func vmVerifyRun(scenario string) error {
	ip, err := vmIP()
	if err != nil {
		return err
	}
	if _, err := os.Stat(scenario); err != nil {
		return fmt.Errorf("no scenario at %s: %w", scenario, err)
	}
	guestScenario := vmGuestHome + "/scenarios/" + filepath.Base(scenario)
	if _, err := sshRun(ip, "test -f "+guestScenario); err != nil {
		return fmt.Errorf("%s is not in the guest — `devtool vm verify install` sends the scenarios", filepath.Base(scenario))
	}
	if _, err := sshRun(ip, "test -d "+vmGuestApp); err != nil {
		return fmt.Errorf("no build installed in the guest — `devtool vm verify install <pkg>` puts one there")
	}

	// A previous run's app is taken down first. The harness takes its own down when it ends, and
	// the one case it cannot is the one that matters here: a run somebody stopped part-way leaves a
	// window on screen that the next run's shots would have in front of them.
	_, _ = sshRun(ip, "pkill -f "+vmVerifyBin+" || true; pkill -f "+vmGuestApp+" || true")

	start := fmt.Sprintf(
		": > %s && rm -rf %s && nohup sh -c 'tail -n 0 -f %s | %s %s --app %s --evidence %s --screen %s --fixtures %s' > %s 2>&1 &",
		vmVerifySteps, vmVerifyEvidence, vmVerifySteps, vmVerifyBin, guestScenario,
		vmGuestApp, vmVerifyEvidence, vmScreenSource, vmVerifyFixtures, vmVerifyLog)
	if _, err := sshRun(ip, start); err != nil {
		return fmt.Errorf("starting the run: %w", err)
	}
	logf("  verify  : %s walking in %s", filepath.Base(scenario), vmCloneName)

	// The first hand-over is a way off: the world is stood up, the app is launched, and the harness
	// holds until it can be shot at all.
	if err := vmVerifyAwait(ip, 0, 180*time.Second); err != nil {
		return err
	}
	return vmVerifyLogTail(20)
}

// vmVerifyStep sends one line and waits for the harness to say something new — which is either the
// next step handed over, or the verdict. The wait is what makes this usable a step at a time: a
// command that returned the moment the line was written would hand back the same screen the caller
// had already read.
func vmVerifyStep(note string) error {
	ip, err := vmIP()
	if err != nil {
		return err
	}
	before, err := vmVerifyLogSize(ip)
	if err != nil {
		return err
	}
	// Appended, not piped: the harness's stdin is a `tail -f` over this file, so nothing has to be
	// held open between one command and the next, and what was sent stays on disk to be counted.
	if _, err := sshRun(ip, fmt.Sprintf("printf '%%s\\n' %q >> %s", note, vmVerifySteps)); err != nil {
		return fmt.Errorf("sending the line: %w", err)
	}
	if err := vmVerifyAwait(ip, before, 120*time.Second); err != nil {
		return err
	}
	return vmVerifyLogTail(12)
}

// vmVerifyAwait holds until the log has grown past `from`, or the run has ended. An ended run is not
// a failure here — the verdict is in the log the caller is about to read — so it comes back the same
// way a hand-over does.
func vmVerifyAwait(ip string, from int, budget time.Duration) error {
	deadline := time.Now().Add(budget)
	for {
		size, err := vmVerifyLogSize(ip)
		if err == nil && size > from {
			return nil
		}
		if _, err := sshRun(ip, "pgrep -f "+vmVerifyBin+" > /dev/null"); err != nil {
			return nil // the run is over; whatever it ended on is in the log
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("the harness said nothing new for %s and is still running — `devtool vm verify log` reads where it stands", budget)
		}
		time.Sleep(2 * time.Second)
	}
}

// vmVerifyLogSize is how far the log has got, in bytes — the one thing that says a harness waiting
// on stdin has moved.
func vmVerifyLogSize(ip string) (int, error) {
	out, err := sshRun(ip, "wc -c < "+vmVerifyLog+" 2>/dev/null || echo 0")
	if err != nil {
		return 0, err
	}
	var n int
	if _, err := fmt.Sscan(strings.TrimSpace(out), &n); err != nil {
		return 0, err
	}
	return n, nil
}

// vmVerifyLogTail prints the end of the run's log — what the harness last said, which is the step it
// is holding on or the verdict it ended with.
func vmVerifyLogTail(lines int) error {
	ip, err := vmIP()
	if err != nil {
		return err
	}
	out, err := sshRun(ip, fmt.Sprintf("tail -n %d %s", lines, vmVerifyLog))
	if err != nil {
		return fmt.Errorf("reading the run's log: %w", err)
	}
	fmt.Printf("%s\n", out)
	return nil
}

// vmVerifyPull brings the evidence out of the guest and prints where it landed. The shots and the
// manifest are what a `Review` step is closed from and what a red one is read by, and they are of no
// use inside a machine that is thrown away.
func vmVerifyPull(out string) error {
	ip, err := vmIP()
	if err != nil {
		return err
	}
	if _, err := sshRun(ip, "test -d "+vmVerifyEvidence); err != nil {
		return fmt.Errorf("no evidence in the guest — a run leaves it at %s", vmVerifyEvidence)
	}
	if out == "" {
		if out, err = os.MkdirTemp("", "amenbo-verify-gui-"); err != nil {
			return err
		}
	} else if err := os.MkdirAll(out, 0o755); err != nil {
		return err
	}
	args := append(sshOpts(), "-r", vmUser+"@"+ip+":"+vmVerifyEvidence+"/.", out)
	if _, err := run("", "scp", args...); err != nil {
		return err
	}
	logf("  verify  : evidence out of %s", vmCloneName)
	fmt.Printf("%s\n", out)
	return nil
}
