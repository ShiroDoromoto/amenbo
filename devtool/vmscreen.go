package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"syscall"
)

// The guest has one screen, and two roles reach for it.
//
// A pre-distribution road walks it a step at a time (`vm verify …`), pressing the shipped build by
// coordinates. An implementation session puts its own dev GUI in there (`devgui install --vm`) and
// opens it. On 2026-09-05 the two met: six task instances went into the clone while a road was
// walking, and the road's shots came back with somebody else's window in front of the app it was
// pressing — a run that had stopped working, several steps away from the reason.
//
// Two claims settle it, because the two roles are held by different things.
//
//   - **A command on this machine** holds the screen for as long as it runs. That is a lock on this
//     side, taken by everything that drives the guest's screen — the flock form the dev GUI build
//     lock uses (`scripts/devgui-build-lock.sh`), for the reason that one gives: the kernel drops
//     it when the last descriptor closes, so a session killed part-way leaves nothing a later one
//     has to break by hand.
//   - **A road being walked** holds it for as long as the run lasts, which outlives the command
//     that started it — a road is walked from separate commands with nothing alive between them.
//     Nothing is written for this one: the harness process in the guest *is* the claim, and it
//     clears itself by ending.
//
// The two are read asymmetrically, on purpose. A road being walked turns a dev GUI away, and does
// not turn away the road's own next command: `vm verify run` already takes a stopped run's app down
// and starts over, and whoever types it is the one walking that road.
//
// The command claim is taken two ways, and which one a command takes follows from how long it holds.
//
//   - **Turned away** (vmTakeScreen) is for a command that holds the screen for minutes — a dev GUI
//     placed in the guest. Queuing behind one of those is indistinguishable from a hang.
//   - **Waited for** (vmHoldScreen) is for `vm exec`, which is how the screen is driven at all: one
//     line brings a window to the front and presses it, and the whole of it is under a second. What
//     that line has to be protected from is somebody fronting another window between the two halves
//     — a press that lands on the wrong app and still exits 0. Turning the second driver away there
//     would break the very command the lock exists to let through, so it waits its turn instead.

// vmScreenLockName is the lock file's name, beside the per-id build locks in the same directory.
const vmScreenLockName = "amenbo-vm-screen.lock"

// vmScreenLockPath is where the claim is held. ~/Library/Caches rather than TMPDIR, the same choice
// the build lock makes: a command that drives the screen runs for minutes, and TMPDIR is swept —
// a lock file swept out from under one is a lock the next caller does not see.
func vmScreenLockPath() string {
	home, err := os.UserHomeDir()
	if err != nil {
		return filepath.Join(os.TempDir(), vmScreenLockName)
	}
	return filepath.Join(home, "Library", "Caches", vmScreenLockName)
}

// vmTakeScreen claims the guest's screen for the caller and answers with what releases it. The
// holder standing is named rather than waited for: waiting is what the collision looked like from
// the outside, both sides alive and neither moving.
func vmTakeScreen(label string) (func(), error) {
	return takeScreenLock(vmScreenLockPath(), label)
}

// vmHoldScreen claims the guest's screen for a command that drives it and waits for its turn when
// somebody else is there. See the asymmetry at the top of this file: `vm exec` is the driving, and a
// driver turned away is the collision the lock was put in to stop.
func vmHoldScreen(label string) (func(), error) {
	return holdScreenLock(vmScreenLockPath(), label)
}

// takeScreenLock is vmTakeScreen against a named file, so the refusal can be exercised without a VM.
func takeScreenLock(path, label string) (func(), error) {
	f, err := openScreenLock(path)
	if err != nil {
		return nil, err
	}
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		holder := screenLockHolder(path)
		f.Close()
		return nil, fmt.Errorf("%s has the screen in %s — wait for it, or stop it, and run this again", holder, vmCloneName)
	}
	return markScreenLock(f, label), nil
}

// holdScreenLock is vmHoldScreen against a named file, so the waiting can be exercised without a VM.
//
// The non-blocking claim is tried first only so the holder can be named: a caller that waits with
// nothing said reads as a command that has stopped responding, and the one line it prints is what
// tells the reader that it has not.
func holdScreenLock(path, label string) (func(), error) {
	f, err := openScreenLock(path)
	if err != nil {
		return nil, err
	}
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		logf("devtool: %s has the screen in %s — waiting for it", screenLockHolder(path), vmCloneName)
		if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX); err != nil {
			f.Close()
			return nil, fmt.Errorf("waiting for the screen in %s: %w", vmCloneName, err)
		}
	}
	return markScreenLock(f, label), nil
}

// openScreenLock opens the claim file, making its directory when this is the first claim ever taken
// on this machine.
func openScreenLock(path string) (*os.File, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, fmt.Errorf("making room for %s: %w", path, err)
	}
	f, err := os.OpenFile(path, os.O_RDWR|os.O_CREATE, 0o644)
	if err != nil {
		return nil, fmt.Errorf("opening %s: %w", path, err)
	}
	return f, nil
}

// markScreenLock writes who is holding and answers with what lets go. The label goes in once the
// lock is held, and the file is emptied again on the way out, so what a later caller reads is the
// holder standing now and never the one before it.
func markScreenLock(f *os.File, label string) func() {
	_ = f.Truncate(0)
	_, _ = f.WriteAt([]byte(label+"\n"), 0)
	return func() {
		_ = f.Truncate(0)
		_ = f.Close()
	}
}

// screenLockHolder is the line the holder left, or a stand-in when it left none. A caller is being
// turned away either way, so the sentence has to read even when the file is empty — which it is for
// the moment between a lock being taken and the label being written.
func screenLockHolder(path string) string {
	b, err := os.ReadFile(path)
	if err != nil {
		return "another command on this machine"
	}
	line := strings.TrimSpace(strings.SplitN(string(b), "\n", 2)[0])
	if line == "" {
		return "another command on this machine"
	}
	return line
}

// vmRoadWalking reports whether the pre-distribution harness is walking a road in the guest. Its own
// process is what says so: it is started detached and outlives the command that started it, so a
// file written by that command would say nothing about whether the road is still going.
func vmRoadWalking(ip string) bool {
	_, err := sshRun(ip, "pgrep -f "+vmVerifyBin+" > /dev/null")
	return err == nil
}

// vmRefuseWhileRoadWalking turns `what` away while a road is being walked in there. It is the guard
// the dev GUI side takes and the road's own commands do not.
func vmRefuseWhileRoadWalking(ip, what string) error {
	if !vmRoadWalking(ip) {
		return nil
	}
	return fmt.Errorf("a pre-distribution road is walking in %s — %s would put a window in front of the app it is pressing (`devtool vm verify log` reads where it stands)", vmCloneName, what)
}
