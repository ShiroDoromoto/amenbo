package main

import (
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// TestSecondClaimIsTurnedAwayNamingTheFirst is the whole point of the lock: the second caller does
// not queue behind the first, it stops and says who is there. Queuing is what the collision looked
// like — two sides alive, neither moving, and a road's shots coming back with the wrong window in
// them.
func TestSecondClaimIsTurnedAwayNamingTheFirst(t *testing.T) {
	lock := filepath.Join(t.TempDir(), vmScreenLockName)

	release, err := takeScreenLock(lock, "`devtool vm verify run`")
	if err != nil {
		t.Fatalf("the first claim was refused: %v", err)
	}

	_, err = takeScreenLock(lock, "`devtool devgui install 4410 --vm`")
	if err == nil {
		t.Fatal("the second claim went through — two commands would drive the one screen")
	}
	if !strings.Contains(err.Error(), "devtool vm verify run") {
		t.Errorf("the refusal does not name the holder: %v", err)
	}
	if !strings.Contains(err.Error(), vmCloneName) {
		t.Errorf("the refusal does not say which machine's screen: %v", err)
	}

	release()
	release2, err := takeScreenLock(lock, "`devtool devgui install 4410 --vm`")
	if err != nil {
		t.Fatalf("the claim was still refused after the holder let go: %v", err)
	}
	release2()
}

// TestADriverWaitsForItsTurnRatherThanBeingTurnedAway is the other half of the asymmetry. `vm exec`
// is how the screen is driven at all, so a driver told "somebody else has it" would be the very
// command the lock exists to let through, failing.
func TestADriverWaitsForItsTurnRatherThanBeingTurnedAway(t *testing.T) {
	lock := filepath.Join(t.TempDir(), vmScreenLockName)

	release, err := takeScreenLock(lock, "`devtool devgui install 4410 --vm`")
	if err != nil {
		t.Fatalf("the first claim was refused: %v", err)
	}

	held := make(chan func())
	go func() {
		r, err := holdScreenLock(lock, "`devtool vm exec -- screen click-named …`")
		if err != nil {
			t.Errorf("the driver was refused instead of waiting: %v", err)
			close(held)
			return
		}
		held <- r
	}()

	select {
	case <-held:
		t.Fatal("the driver went in while the other claim was standing — two commands on the one screen")
	case <-time.After(100 * time.Millisecond):
	}

	release()
	select {
	case r, ok := <-held:
		if !ok {
			t.Fatal("the driver gave up rather than taking its turn")
		}
		if got := screenLockHolder(lock); !strings.Contains(got, "vm exec") {
			t.Errorf("holder once the driver went in = %q, want it named", got)
		}
		r()
	case <-time.After(5 * time.Second):
		t.Fatal("the driver never got its turn after the holder let go")
	}
}

// TestADriversLabelIsOneLine keeps the claim file readable. What drives the screen is a shell script
// of several lines, and the holder is read a line at a time.
func TestADriversLabelIsOneLine(t *testing.T) {
	label := vmExecLabel([]string{"PID=$(pgrep -f amenbo-app | head -1);\n  swift /Users/admin/screen.swift front $PID;\n  swift /Users/admin/screen.swift click-named $PID \"Link a folder\""})
	if strings.Contains(label, "\n") {
		t.Errorf("the label spans lines: %q", label)
	}
	if !strings.Contains(label, "devtool vm exec") {
		t.Errorf("the label does not say which command is holding: %q", label)
	}
	if len([]rune(label)) > 100 {
		t.Errorf("the label is %d runes, too long to read in a refusal: %q", len([]rune(label)), label)
	}
}

// TestAReleasedLockNamesNobody pins what the file says once the holder has gone. A refusal reads the
// first line, so a label left behind would name a command that ended — which is a caller told to
// wait for something that is not there.
func TestAReleasedLockNamesNobody(t *testing.T) {
	lock := filepath.Join(t.TempDir(), vmScreenLockName)

	release, err := takeScreenLock(lock, "`devtool vm verify install`")
	if err != nil {
		t.Fatalf("claiming: %v", err)
	}
	if got := screenLockHolder(lock); got != "`devtool vm verify install`" {
		t.Errorf("holder while held = %q, want the label", got)
	}
	release()
	if got := screenLockHolder(lock); got != "another command on this machine" {
		t.Errorf("holder after release = %q, want the stand-in", got)
	}
}

// TestTheHolderReadsEvenWithNoFile keeps the refusal sentence readable in the one gap the lock has:
// between a claim being taken and its label being written, there is nothing in the file to read.
func TestTheHolderReadsEvenWithNoFile(t *testing.T) {
	if got := screenLockHolder(filepath.Join(t.TempDir(), "not-there.lock")); got == "" {
		t.Error("a missing lock file left the refusal with no subject")
	}
}

// TestTheLockSitsBesideTheBuildLocks pins the directory rather than the name. ~/Library/Caches is
// where the dev GUI build lock lives, for the reason that one gives: a command that drives the
// screen runs for minutes and TMPDIR is swept out from under it.
func TestTheLockSitsBesideTheBuildLocks(t *testing.T) {
	t.Setenv("HOME", "/Users/someone")
	if got, want := vmScreenLockPath(), "/Users/someone/Library/Caches/"+vmScreenLockName; got != want {
		t.Errorf("lock path = %q, want %q", got, want)
	}
}
