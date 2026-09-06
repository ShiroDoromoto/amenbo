package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"strconv"
	"strings"
)

// Putting a task's own dev GUI in the throwaway VM instead of on this machine.
//
// The build stays on the host and only the placing moves: the guest then needs neither Rust nor
// node, and the `.app` that was just baked here runs there unchanged because the two are the same
// arch and the same OS generation (measured: 43MB across in 0.96s).
//
// The host route is untouched and stays the default — `make install-gui-dev AMB-T-ID=<id>` still
// installs on this machine. This is a second destination, not a replacement: a clone or a fork with
// one Mac has no VM, and a default that needed one would leave them unable to verify anything.
//
// The guest layout mirrors the host's exactly — the bundle under /Applications, the store under
// Application Support — so everything that addresses an instance by path reads the same on both
// sides, and only the machine it is asked of changes.

// vmStagingDir is where a bundle and a store land in the guest before they are moved into place. A
// staged copy is swapped in with `mv`, so what is replaced is replaced whole: `scp` onto a directory
// that is already there merges into it, which would leave the files of an older build inside a newer
// bundle. Names under it carry no spaces, so nothing has to be quoted for scp's own remote parsing.
const vmStagingDir = "/tmp"

// guiBundleBuildDir is where tauri leaves the built macOS bundle, relative to the checkout — the
// Makefile's BUNDLE_DIR, which is the source of truth. Read here rather than installed-from, so the
// VM route puts nothing on this machine at all.
const guiBundleBuildDir = "app/src-tauri/target/release/bundle/macos"

// vmTaskDevBundle and vmTaskDevAppData are the two places an instance occupies on both machines —
// the ones the host holds (taskDevGUIPaths), read under the guest's own home. The third place is
// the guest's alone (vmTaskWorkDir).
func vmTaskDevBundle(id string) string {
	return filepath.Join(macAppsDir, taskDevBundle(id)+".app")
}

func vmTaskDevAppData(id string) string {
	return appDataDir(vmGuestHome, taskDevAppData(id))
}

// vmTaskWorkDir is the folder in the guest an instance is worked *from* — its own bound folder, and
// the one thing an instance holds in there that the host route has no counterpart for.
//
// A `.amenbo` pointer names exactly one store, so where it sits decides who is able to stand on it,
// and both of the places it could have gone are places it must not:
//
//   - **Not the guest's home.** One pointer at /Users/admin belongs to whichever instance wrote it,
//     and every other instance's CLI walks up into it and is refused for naming a store that is not
//     its own (`pointer_other_store`). That is the shape the guest had, and it is why
//     `--actor ai` reached nothing in there at all — the facet draws its reach from the pointer of
//     the folder it stands in, so `--actor human --project <n>` was standing in for it, on a road
//     no AI can walk.
//   - **Not inside the store.** A throwaway store is made by cloning the shared dev app-data whole,
//     so a pointer left in there rides into the clone and the next instance is born holding the
//     previous one's.
func vmTaskWorkDir(id string) string {
	return filepath.Join(vmGuestHome, "amenbo-work-"+id)
}

// shq quotes one word for the guest's shell. Everything sent over ssh is joined and handed to a
// shell in there, and the bundle path holds spaces and parentheses — unquoted, the shell reads them
// as three words and a subshell.
func shq(s string) string {
	return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'"
}

// devGUIInstallVM puts task id's own dev GUI in the throwaway VM: the bundle built here, and a store
// for it to open on.
//
// It raises the clone if it is not running rather than refusing: what a person decides is when the
// VM is thrown away, never when it is raised, and there is nothing to place until one is up.
//
// **What it does refuse is a screen somebody else is driving** (vmscreen.go): the guest has one, and
// an instance placed in there while a pre-distribution road is walking is a window over the app that
// road is pressing.
func devGUIInstallVM(id string) error {
	if runtime.GOOS != "darwin" {
		return fmt.Errorf("the dev GUI and the VM it is put in are both macOS-only")
	}
	_, worktree, err := paths(id)
	if err != nil {
		return err
	}
	bundle := filepath.Join(worktree, guiBundleBuildDir, taskDevBundle(id)+".app")
	if _, err := os.Stat(bundle); err != nil {
		return fmt.Errorf("no bundle built for task %s at %s — `make install-gui-dev-vm AMB-T-ID=%s` builds it and puts it there", id, bundle, id)
	}

	release, err := vmTakeScreen("`devtool devgui install " + id + " --vm`")
	if err != nil {
		return err
	}
	defer release()

	ip, err := vmEnsureUp()
	if err != nil {
		return err
	}
	if err := vmRefuseWhileRoadWalking(ip, "putting "+taskDevBundle(id)+" in there"); err != nil {
		return err
	}
	if err := vmStopTaskDevGUI(ip, id); err != nil {
		return err
	}
	if err := vmPutBundle(ip, id, bundle); err != nil {
		return err
	}
	vmSeedAppData(ip, id)
	vmSeedWorkDir(ip, id, worktree)

	dest := vmTaskDevBundle(id)
	logf("  dev GUI : open it in there — `devtool vm exec -- open -a %s`", shq(dest))
	fmt.Printf("%s\n", dest)
	return nil
}

// vmStopTaskDevGUI asks the instance to quit in the guest and reports whether it is gone, for the
// same reason the host teardown does (stopTaskDevGUI): an app replaced underneath itself writes its
// store back on the way out, and the bundle that was just sent is the one it overwrites.
//
// Asked by executable name, not by bundle name: `pgrep -x` matches a process name exactly, and it is
// the name each dev shape is built under precisely so one instance can be addressed and not another.
func vmStopTaskDevGUI(ip, id string) error {
	proc := taskDevExecutable(id)
	// One round trip rather than a poll from here: a quit is asynchronous — the app returns from
	// the event before its process is gone — and waiting for it over ssh would be one connection
	// per look.
	script := fmt.Sprintf(
		"pgrep -x %[1]s >/dev/null 2>&1 || exit 0; "+
			"osascript -e 'quit app %[2]q' >/dev/null 2>&1; "+
			"for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do "+
			"pgrep -x %[1]s >/dev/null 2>&1 || exit 0; sleep 0.2; done; exit 1",
		shq(proc), taskDevBundle(id))
	if _, err := sshRun(ip, script); err != nil {
		return fmt.Errorf("%s would not quit in %s (%w) — a bundle replaced under a running app is written back over by it; quit it in there, then run this again", taskDevBundle(id), vmCloneName, err)
	}
	return nil
}

// vmPutBundle sends the built bundle to the guest and swaps it into /Applications there.
//
// Staged and moved rather than copied over what is there: `scp -r` onto an existing directory merges
// into it, and a bundle carrying files from an older build is the failure that reads exactly like an
// implementation that does not work.
func vmPutBundle(ip, id, bundle string) error {
	staging := filepath.Join(vmStagingDir, "amenbo-devgui-"+id+".app")
	dest := vmTaskDevBundle(id)
	if _, err := sshRun(ip, "rm -rf "+shq(staging)); err != nil {
		return fmt.Errorf("clearing %s in %s: %w", staging, vmCloneName, err)
	}
	args := append(sshOpts(), "-r", bundle, vmUser+"@"+ip+":"+staging)
	if _, err := run("", "scp", args...); err != nil {
		return fmt.Errorf("sending %s to %s: %w", filepath.Base(bundle), vmCloneName, err)
	}
	if _, err := sshRun(ip, "rm -rf "+shq(dest)+" && mv "+shq(staging)+" "+shq(dest)); err != nil {
		return fmt.Errorf("putting the bundle at %s in %s: %w", dest, vmCloneName, err)
	}
	logf("  dev GUI : %s is in %s at %s", taskDevBundle(id), vmCloneName, dest)
	return nil
}

// vmSeedAppData gives the instance in the guest a store to open on, cloned from the shared dev store
// on the host — the same setup (plugins, catalog, projects) the host route seeds an instance from
// (provisionTaskDevGUI), carried across because the guest has no shared dev app of its own and never
// will: it is a clone thrown away at the end of a session.
//
// Every arm reports and returns, the way the host's seeding does: an instance that opens empty is a
// poorer screen to verify, never a reason to fail the placing that asked. A store already in the
// guest is left alone — it is the session's own work, and a fresh clone would throw it away.
func vmSeedAppData(ip, id string) {
	dst := vmTaskDevAppData(id)
	if out, err := sshRun(ip, "[ -d "+shq(dst)+" ] && echo yes || echo no"); err != nil {
		logf("  dev GUI : warning — asking %s about %s failed (%v); its store is left as it is", vmCloneName, filepath.Base(dst), err)
		return
	} else if strings.TrimSpace(out) == "yes" {
		logf("  dev GUI : app-data %s is already in %s — left as it is", filepath.Base(dst), vmCloneName)
		return
	}
	home, err := os.UserHomeDir()
	if err != nil {
		logf("  dev GUI : warning — no home to read the shared dev store from (%v); the instance will open empty", err)
		return
	}
	src := appDataDir(home, sharedDevAppData)
	if !dirExists(src) {
		logf("  dev GUI : no shared dev store on this machine to send — the instance will open empty")
		return
	}
	staging := filepath.Join(vmStagingDir, "amenbo-devgui-"+id+"-data")
	if _, err := sshRun(ip, "rm -rf "+shq(staging)); err != nil {
		logf("  dev GUI : warning — clearing %s in %s failed (%v); the instance will open empty", staging, vmCloneName, err)
		return
	}
	args := append(sshOpts(), "-r", src, vmUser+"@"+ip+":"+staging)
	if _, err := run("", "scp", args...); err != nil {
		logf("  dev GUI : warning — sending the shared dev store failed (%v); the instance will open empty", err)
		return
	}
	if _, err := sshRun(ip, "mkdir -p "+shq(filepath.Dir(dst))+" && mv "+shq(staging)+" "+shq(dst)); err != nil {
		logf("  dev GUI : warning — putting the store at %s failed (%v); the instance will open empty", dst, err)
		return
	}
	logf("  dev GUI : app-data %s cloned into %s from this machine's shared dev store", filepath.Base(dst), vmCloneName)
}

// vmSeedWorkDir cuts the instance's own folder in the guest and binds it to a project in that
// instance's store, so the AI facet has somewhere to stand (vmTaskWorkDir says why the folder has to
// be its own).
//
// Every arm reports and returns, the way the app-data seeding above it does: an instance whose
// folder could not be bound is a poorer one to drive — its CLI has to be told which project by hand
// — never a reason to fail the placing that asked.
//
// The CLI is not built for this: the bundle placing is not a step anyone should have to wait on a
// second toolchain run for. If this checkout has no debug build yet the folder is left cut and
// unbound, and the first `devgui cli --vm` — which rebuilds and sends one anyway — binds it.
func vmSeedWorkDir(ip, id, worktree string) {
	dir := vmTaskWorkDir(id)
	bound, err := vmCutWorkDir(ip, dir)
	if err != nil {
		logf("  dev GUI : warning — %v; its CLI will run unbound", err)
		return
	}
	if bound {
		logf("  dev GUI : %s is bound already in %s — left as it is", filepath.Base(dir), vmCloneName)
		return
	}
	bin, err := vmSendCLI(ip, id, worktree, true)
	if err != nil {
		logf("  dev GUI : %s is cut but not bound yet (%v) — the first `devtool devgui cli %s --vm` binds it", filepath.Base(dir), err, id)
		return
	}
	vmBindWorkDir(ip, id, bin, dir)
}

// vmCutWorkDir makes the instance's folder in the guest if it is not there, and answers whether it
// already holds a pointer. A folder that has one is left exactly as it is: it is the session's own
// binding, and re-pointing it under a run that only meant to place a bundle would move the store
// every command after it writes.
func vmCutWorkDir(ip, dir string) (bound bool, err error) {
	out, err := sshRun(ip, "mkdir -p "+shq(dir)+" && { [ -e "+shq(dir)+"/.amenbo ] && echo yes || echo no; }")
	if err != nil {
		return false, fmt.Errorf("cutting %s in %s: %w", dir, vmCloneName, err)
	}
	return strings.TrimSpace(out) == "yes", nil
}

// vmBindWorkDir points the instance's folder at a project of the instance's store, with a CLI that
// is already in the guest.
//
// **Setting a folder up is a human's act**, so that is the facet the bind is done under — the same
// reading scripts/verify-cli.sh makes of its own INIT=1. `--force` goes with it because the guest's
// home may still hold a pointer from before instances had folders of their own, and a bind or an
// init under an already-managed tree is otherwise refused as one that would shadow it.
//
// What it binds to is the store's lowest-numbered project. A throwaway store is a clone of the
// shared dev app-data, so which of its projects a screen wants is not something this can know; what
// it can do is pick the same one every time and name it in the log. Re-pointing is one command:
// `devtool devgui cli <id> --vm -- --actor human bind --project <n> --force`.
//
// A store with no project at all — an instance seeded by a CLI run before any bundle was placed —
// gets one raised in the folder instead, which is the same move `make verify INIT=1` makes on its
// own throwaway store, and `init` binds the folder as it goes.
func vmBindWorkDir(ip, id, guestBin, dir string) {
	store := vmTaskDevAppData(id)
	project, err := vmGuestProject(ip, guestBin, store)
	if err != nil {
		logf("  dev GUI : warning — %v; %s is left unbound", err, filepath.Base(dir))
		return
	}
	amenbo := "--actor human bind --project " + shq(project) + " --force"
	done := fmt.Sprintf("  dev GUI : %s is bound to project %s of %s", dir, project, filepath.Base(store))
	if project == "" {
		amenbo = "init --name dev --actor human --force"
		done = fmt.Sprintf("  dev GUI : %s holds the project raised in the empty store %s", dir, filepath.Base(store))
	}
	cmd := "cd " + shq(dir) + " && " + storeEnv + "=" + shq(store) + " " + shq(guestBin) + " " + amenbo
	if _, err := sshRun(ip, cmd); err != nil {
		logf("  dev GUI : warning — binding %s in %s failed (%v); its CLI will run unbound", dir, vmCloneName, err)
		return
	}
	logf("%s", done)
}

// vmGuestProject answers with the lowest-numbered project of the instance's store, asked of the CLI
// in the guest rather than read off the database here: a project id is a per-store primary key, and
// that store is one this machine never opens.
//
// It is asked from the staging dir, which is the one place in the guest outside the home: a read run
// anywhere under /Users/admin would walk up into a pointer left there by an older instance and be
// refused for naming another store — the very failure this folder exists to end, and one the bind
// itself is exempt from because a bind is how a folder gets out of it.
func vmGuestProject(ip, guestBin, store string) (string, error) {
	cmd := "cd " + shq(vmStagingDir) + " && " + storeEnv + "=" + shq(store) + " " + shq(guestBin) +
		" --actor human project list --json"
	out, err := sshRun(ip, cmd)
	if err != nil {
		return "", fmt.Errorf("asking %s which projects the instance's store holds: %w", vmCloneName, err)
	}
	project, err := lowestProjectID(out)
	if err != nil {
		return "", fmt.Errorf("reading the projects of %s back: %w", filepath.Base(store), err)
	}
	return project, nil
}

// lowestProjectID picks the smallest id out of a `project list --json` document, as the string the
// bind is given. Smallest rather than first: the order of the listing is the CLI's to change, and a
// folder that binds somewhere else after an unrelated release is worse than one that binds to a
// project nobody wanted.
//
// A store holding no project answers with the empty string and **no error** — it is a store waiting
// for its first project, which is a shape the caller answers by raising one, not a listing that
// failed to be read.
func lowestProjectID(listing string) (string, error) {
	var doc struct {
		Projects []struct {
			ID int64 `json:"id"`
		} `json:"projects"`
	}
	if err := json.Unmarshal([]byte(listing), &doc); err != nil {
		return "", err
	}
	lowest := int64(0)
	for _, p := range doc.Projects {
		if lowest == 0 || p.ID < lowest {
			lowest = p.ID
		}
	}
	if lowest == 0 {
		return "", nil
	}
	return strconv.FormatInt(lowest, 10), nil
}

// ---------------------------------------------------------------------------
// operating an instance that is in there, not here
// ---------------------------------------------------------------------------

// The commands that address a task's dev GUI — its pid, a shot of its window, a CLI against its
// store, its removal, the machine-wide sweep — all read this machine by default, and all of them
// answer for the guest with `--vm`. The default is not moved: a clone or a fork with one Mac has no
// VM, and the host route has to keep working for them.
//
// Only the *machine asked* changes. The guest layout mirrors this one, so the same names, the same
// paths and the same pid lookup do the work; what differs is that a listing is an `ls` over ssh, a
// removal is an `rm -rf` over ssh, and the screen tool being driven is the copy `devtool vm screen`
// put in there.
//
// Two of them cannot be answered from inside the guest at all, which is the reason they live here
// rather than as something typed in there: the sweep needs `git worktree list`, which only this
// machine can answer, and the CLI is a build of this checkout. Both reach across.

// vmInstanceID answers which task's instance a `--vm` command addresses: the one named, or — with
// no id — the one this checkout is written in.
//
// The guest holds per-task instances and nothing else. The shared dev app is this machine's
// permanent setup and is never sent anywhere, so the host's fallback to it has no counterpart in
// there: without a number and without a task checkout, there is nothing for a name to fall back on.
func vmInstanceID(id string) (string, error) {
	if id != "" {
		return id, nil
	}
	if own := taskIDFromCheckout(mustTreeRoot()); own != "" {
		return own, nil
	}
	return "", fmt.Errorf("the VM holds a task's own instance and not the shared dev app — name one (`… <id> --vm`), or run this from a task's checkout")
}

// vmDevGUIPID finds the pid of task id's instance in the guest, and 0 when it is not running. The
// process table is read in there and matched here, by the bundle a process was executed out of —
// the same lookup the host does, on the other machine's `ps`.
func vmDevGUIPID(ip, id string) (int, error) {
	out, err := sshRun(ip, "ps -Ao pid=,args=")
	if err != nil {
		return 0, fmt.Errorf("reading the process table in %s: %w", vmCloneName, err)
	}
	return pidRunningFrom(out, vmTaskDevBundle(id)+"/"), nil
}

// vmResolveDevGUI finds the instance a `--vm` command is about: the guest it is in, and the pid its
// windows belong to. `front` brings it forward in there first — a window behind another Space
// cannot be found at all, and a shot of one nobody fronted is a shot of what is over it.
//
// Fronting is done by the guest's own copy of the screen tool, which `devtool vm screen` puts
// there. A missing copy is reported and carried past: it costs a front, not the pid that was asked
// for.
func vmResolveDevGUI(id string, front bool, window string) (ip string, target devGUITarget, err error) {
	id, err = vmInstanceID(id)
	if err != nil {
		return "", devGUITarget{}, err
	}
	ip, err = vmIP()
	if err != nil {
		return "", devGUITarget{}, err
	}
	pid, err := vmDevGUIPID(ip, id)
	if err != nil {
		return "", devGUITarget{}, err
	}
	if pid == 0 {
		return "", devGUITarget{}, fmt.Errorf("%s is not running in %s — `make install-gui-dev-vm AMB-T-ID=%s` puts it there, then open it (`devtool vm exec -- open -a %s`)",
			taskDevBundle(id), vmCloneName, id, shq(vmTaskDevBundle(id)))
	}
	if front {
		// A front is the collision itself, not a step towards it: it is what puts one window over
		// the one a road is pressing. Reading the pid or shooting the window without it disturbs
		// nothing, so the guard sits here rather than over the whole command.
		if err := vmRefuseWhileRoadWalking(ip, "bringing "+taskDevBundle(id)+" forward"); err != nil {
			return "", devGUITarget{}, err
		}
		if _, err := sshRun(ip, shq(vmScreenPath)+" front "+strconv.Itoa(pid)+vmWindowArg(window)); err != nil {
			logf("  warning: bringing %s forward in %s failed (%v) — is the screen tool in there? (`devtool vm screen`)", taskDevBundle(id), vmCloneName, err)
		}
	}
	logf("  %s is running in %s", taskDevBundle(id), vmCloneName)
	return ip, devGUITarget{bundle: taskDevBundle(id), pid: pid}, nil
}

// vmWindowArg is windowArgs for the guest, where the tool is reached through a shell line rather
// than an argv.
func vmWindowArg(window string) string {
	if window == "" {
		return ""
	}
	return " --window " + shq(window)
}

// vmDevGUIShowPID prints on stdout the pid of an instance running in the guest. It is a pid in
// there, so what takes it is something driving that machine — the screen tool in the guest, or
// `devtool vm exec`.
func vmDevGUIShowPID(id string, front bool, window string) error {
	_, target, err := vmResolveDevGUI(id, front, window)
	if err != nil {
		return err
	}
	fmt.Printf("%d\n", target.pid)
	return nil
}

// vmDevGUIShot captures the instance's window in the guest and prints the png's path **on this
// machine**: the shot is taken in there by the guest's screen tool and brought back out, because
// what looks at it is here.
func vmDevGUIShot(id string, front bool, window string) error {
	inst, err := vmInstanceID(id)
	if err != nil {
		return err
	}
	ip, target, err := vmResolveDevGUI(inst, front, window)
	if err != nil {
		return err
	}
	guest := filepath.Join(vmStagingDir, "amenbo-devgui-"+inst+".png")
	if _, err := sshRun(ip, shq(vmScreenPath)+" shot "+strconv.Itoa(target.pid)+" "+shq(guest)+vmWindowArg(window)); err != nil {
		return fmt.Errorf("shooting the window of %s in %s failed: %w — the screen tool has to be in there (`devtool vm screen`)", target.bundle, vmCloneName, err)
	}
	file, err := os.CreateTemp("", fmt.Sprintf("amenbo-devgui-%s-*.png", inst))
	if err != nil {
		return err
	}
	path := file.Name()
	file.Close()
	args := append(sshOpts(), vmUser+"@"+ip+":"+guest, path)
	if _, err := run("", "scp", args...); err != nil {
		os.Remove(path)
		return fmt.Errorf("bringing the shot back out of %s: %w", vmCloneName, err)
	}
	logf("  shot the window of %s in %s", target.bundle, vmCloneName)
	fmt.Printf("%s\n", path)
	return nil
}

// vmTaskCLI runs an amenbo command against the store the instance **in the guest** reads, so a
// screen in there can be given something to show.
//
// The CLI is this checkout's own build, sent across: the guest holds no toolchain, and the two
// machines are the same arch, so what is built here runs there. It is pointed at the store with
// `AMENBO_HOME`, the way the host route points its own (see taskCLI).
//
// Where it runs is the one place the two routes part: the host route runs in the store's own
// directory, and this one runs in the instance's bound folder (vmTaskWorkDir). On this machine every
// instance's store is a folder of its own, so a pointer beside one is one instance's and no other's;
// in the guest that same reading put every instance on the one home directory they share.
func vmTaskCLI(id string, noBuild bool, argv []string) (int, error) {
	if len(argv) == 0 {
		return 0, fmt.Errorf("nothing to run — `devtool devgui cli %s --vm -- <amenbo args…>`", id)
	}
	_, worktree, err := paths(id)
	if err != nil {
		return 0, err
	}
	ip, err := vmIP()
	if err != nil {
		return 0, err
	}
	guestBin, err := vmSendCLI(ip, id, worktree, noBuild)
	if err != nil {
		return 0, err
	}

	store := vmTaskDevAppData(id)
	// The store is made when it is not there, the way the host route does: an instance that was
	// never seeded is an empty one to write into, not a failure to report.
	if _, err := sshRun(ip, "mkdir -p "+shq(store)); err != nil {
		return 0, fmt.Errorf("make the task's store dir in %s: %w", vmCloneName, err)
	}
	// The folder is cut here as well as at placing, because an instance can be seeded before its
	// bundle is ever put in there — and a run that cannot stand anywhere is one that fails at the
	// `cd` below rather than reporting and carrying on.
	dir := vmTaskWorkDir(id)
	bound, err := vmCutWorkDir(ip, dir)
	if err != nil {
		return 0, err
	}
	if !bound {
		vmBindWorkDir(ip, id, guestBin, dir)
	}

	quoted := make([]string, 0, len(argv))
	for _, a := range argv {
		quoted = append(quoted, shq(a))
	}
	cmd := "cd " + shq(dir) + " && " + storeEnv + "=" + shq(store) + " " + shq(guestBin) + " " + strings.Join(quoted, " ")
	logf("  store   : %s:%s", vmCloneName, store)
	logf("  folder  : %s:%s", vmCloneName, dir)
	logf("  cli     : %s %s", guestBin, strings.Join(argv, " "))
	return runThrough("", nil, "ssh", sshArgs(ip, cmd)...)
}

// vmSendCLI puts this checkout's CLI in the guest and answers with where it landed.
//
// It is sent on every run rather than once: the CLI is rebuilt first precisely because the tree it
// seeds a store for keeps moving, and a stale copy in there would write what the old code wrote.
func vmSendCLI(ip, id, worktree string, noBuild bool) (string, error) {
	if _, err := os.Stat(worktree); err != nil {
		return "", fmt.Errorf("no worktree for task %s (%s missing) — cut one with the `worktree` plugin first", id, worktree)
	}
	if !noBuild {
		if _, err := runEnv(worktree, cliBuildEnv, "cargo", "build", "-q", "-p", "amenbo-cli"); err != nil {
			return "", fmt.Errorf("build the task's CLI: %w", err)
		}
	}
	bin := taskCLIBin(worktree)
	if _, err := os.Stat(bin); err != nil {
		return "", fmt.Errorf("no CLI at %s — drop --no-build so it is built", bin)
	}
	guestBin := filepath.Join(vmGuestHome, "amenbo-cli-"+id)
	scpArgs := append(sshOpts(), bin, vmUser+"@"+ip+":"+guestBin)
	if _, err := run("", "scp", scpArgs...); err != nil {
		return "", fmt.Errorf("sending the CLI to %s: %w", vmCloneName, err)
	}
	return guestBin, nil
}

// vmRemoveTaskDevGUI deletes one instance in the guest — the two halves the host teardown takes,
// and the bound folder that only the guest has.
//
// A running instance is quit first and, if it will not go, every part is left where it is:
// removing the store under a running app does not remove it (it writes its store back on the way
// out), and a half-removed instance is the worse of the two outcomes.
func vmRemoveTaskDevGUI(id string) error {
	ip, err := vmIP()
	if err != nil {
		return err
	}
	if err := vmStopTaskDevGUI(ip, id); err != nil {
		return err
	}
	return vmReclaim(ip, vmTaskDevGUIPaths(id))
}

// vmTaskDevGUIPaths lists everything an instance occupies in the guest, in the order teardown
// removes it — the guest's own reading of taskDevGUIPaths, plus the bound folder that exists only
// in there. The folder is last because it is the smallest thing and the one whose absence is
// hardest to notice: a pointer left behind names a store that teardown has just deleted.
func vmTaskDevGUIPaths(id string) []string {
	return append(taskDevGUIPaths(vmGuestHome, macAppsDir, id), vmTaskWorkDir(id))
}

// vmReclaim removes each of `paths` in the guest that is actually there and reports it. One round
// trip for the set, and each removal says so on its own line, so what came back is the list of what
// went — an instance nobody built has nothing to reclaim and nothing to report.
func vmReclaim(ip string, paths []string) error {
	var script strings.Builder
	for _, p := range paths {
		fmt.Fprintf(&script, "if [ -e %[1]s ]; then rm -rf %[1]s && echo %[1]s; fi; ", shq(p))
	}
	out, err := sshRun(ip, script.String())
	if err != nil {
		return fmt.Errorf("removing the instance in %s: %w", vmCloneName, err)
	}
	for _, line := range strings.Split(out, "\n") {
		if line = strings.TrimSpace(line); line != "" {
			logf("  removed the task's dev GUI in %s: %s", vmCloneName, line)
		}
	}
	return nil
}

// vmDevGUISweep reports every per-task instance **in the guest** and, with `apply`, reclaims the
// ones no worktree claims any more.
//
// This is the one command that cannot be typed in the guest at all, and the reason the whole
// destination flag exists rather than "ssh in and run devtool there": what makes an instance live
// is a checkout, and the checkouts are on this machine. Asked in there, git has nothing to answer
// with — and a sweep that cannot tell live from orphan refuses rather than guess, so it would
// simply never run.
func vmDevGUISweep(apply bool) error {
	root, _, err := paths("0") // the id is unused: this only needs the main repo root
	if err != nil {
		return err
	}
	checkouts, err := worktreeCheckouts(root)
	if err != nil {
		return fmt.Errorf("git worktree list: %w — without it a live instance cannot be told from an orphan, and this refuses to guess", err)
	}
	var liveIDs []string
	for _, c := range checkouts {
		if id := taskIDFromCheckout(c); id != "" {
			liveIDs = append(liveIDs, id)
		}
	}
	ip, err := vmIP()
	if err != nil {
		return err
	}
	bundles, err := vmListDir(ip, macAppsDir)
	if err != nil {
		return err
	}
	stores, err := vmListDir(ip, appSupportDir(vmGuestHome))
	if err != nil {
		return err
	}

	instances := instancesFrom(bundles, stores, vmGuestHome, macAppsDir, liveIDs)
	if len(instances) == 0 {
		logf("  no per-task dev GUI in %s", vmCloneName)
		return nil
	}
	var orphans []taskDevGUIInstance
	for _, inst := range instances {
		state := "orphan"
		if inst.live {
			state = "in use"
		} else {
			orphans = append(orphans, inst)
		}
		logf("  task %-6s %s", inst.id, state)
		for _, p := range inst.paths {
			logf("    %s:%s", vmCloneName, p)
		}
	}
	if len(orphans) == 0 {
		logf("  every instance in %s still belongs to a worktree — nothing to reclaim", vmCloneName)
		return nil
	}
	if !apply {
		logf("  %d orphan(s) — `devtool devgui sweep --vm --yes` reclaims them (the ones in use are never touched)", len(orphans))
		return nil
	}
	reclaimed := 0
	for _, inst := range orphans {
		if err := vmStopTaskDevGUI(ip, inst.id); err != nil {
			logf("  task %s: %v", inst.id, err)
			continue
		}
		// The bound folder rides along with the halves the listing found: it is not what makes an
		// instance visible (a folder holding one JSON file is not evidence of a dev GUI), but it is
		// this instance's, and a sweep that took the store and left the pointer to it is a sweep
		// that leaves a name for something gone.
		if err := vmReclaim(ip, append(slices.Clone(inst.paths), vmTaskWorkDir(inst.id))); err != nil {
			logf("  task %s: %v", inst.id, err)
			continue
		}
		reclaimed++
	}
	logf("✓ reclaimed %d of %d orphan instance(s) in %s", reclaimed, len(orphans), vmCloneName)
	return nil
}

// vmListDir lists what is in one directory of the guest. A directory that is not there lists as
// nothing rather than as an error — that is what an applications folder holding no instance looks
// like, and it is the same answer the host reading gives.
func vmListDir(ip, dir string) ([]string, error) {
	out, err := sshRun(ip, "ls -1 "+shq(dir)+" 2>/dev/null || true")
	if err != nil {
		return nil, fmt.Errorf("listing %s in %s: %w", dir, vmCloneName, err)
	}
	var names []string
	for _, line := range strings.Split(out, "\n") {
		if line = strings.TrimSpace(line); line != "" {
			names = append(names, line)
		}
	}
	return names, nil
}
