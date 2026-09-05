package main

import (
	"path/filepath"
	"strings"
	"testing"
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
