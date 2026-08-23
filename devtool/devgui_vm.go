package main

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
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

// vmHome is the guest account's home. It is a constant rather than something asked of the guest:
// the account is the image's own (vmUser), and every path here is composed against it.
const vmHome = "/Users/" + vmUser

// vmStagingDir is where a bundle and a store land in the guest before they are moved into place. A
// staged copy is swapped in with `mv`, so what is replaced is replaced whole: `scp` onto a directory
// that is already there merges into it, which would leave the files of an older build inside a newer
// bundle. Names under it carry no spaces, so nothing has to be quoted for scp's own remote parsing.
const vmStagingDir = "/tmp"

// guiBundleBuildDir is where tauri leaves the built macOS bundle, relative to the checkout — the
// Makefile's BUNDLE_DIR, which is the source of truth. Read here rather than installed-from, so the
// VM route puts nothing on this machine at all.
const guiBundleBuildDir = "app/src-tauri/target/release/bundle/macos"

// vmTaskDevBundle and vmTaskDevAppData are the two places an instance occupies in the guest — the
// same two the host holds (taskDevGUIPaths), under the guest's own home.
func vmTaskDevBundle(id string) string {
	return filepath.Join(macAppsDir, taskDevBundle(id)+".app")
}

func vmTaskDevAppData(id string) string {
	return appDataDir(vmHome, taskDevAppData(id))
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

	ip, err := vmEnsureUp()
	if err != nil {
		return err
	}
	if err := vmStopTaskDevGUI(ip, id); err != nil {
		return err
	}
	if err := vmPutBundle(ip, id, bundle); err != nil {
		return err
	}
	vmSeedAppData(ip, id)

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
