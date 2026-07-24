package main

import (
	"os"
	"path/filepath"
	"testing"
)

// TestTaskDevGUINamesMatchTheBuild pins the instance's names literally, because that is what they
// are: the contract with the Makefile that builds it. A rename on one side and not the other leaves
// a bundle and a store nobody ever deletes.
func TestTaskDevGUINamesMatchTheBuild(t *testing.T) {
	if got, want := taskDevAppData("2131"), "amenbo-dev-2131"; got != want {
		t.Errorf("app-data name = %q, want %q", got, want)
	}
	if got, want := taskDevBundle("2131"), "amenbo (dev 2131)"; got != want {
		t.Errorf("bundle name = %q, want %q", got, want)
	}
	if got, want := appDataDir("/Users/x", "amenbo-dev-2131"),
		"/Users/x/Library/Application Support/work.amenbo.amenbo-dev-2131"; got != want {
		t.Errorf("app-data dir = %q, want %q", got, want)
	}
}

// TestTaskDevGUIPathsCoverBundleAndStore holds teardown to both halves of an instance. The bundle
// is the larger one, but a store left behind is what would carry a finished task's data into the
// next one that reuses the number.
func TestTaskDevGUIPathsCoverBundleAndStore(t *testing.T) {
	got := taskDevGUIPaths("/Users/x", macAppsDir, "2131")
	want := []string{
		"/Applications/amenbo (dev 2131).app",
		"/Users/x/Library/Application Support/work.amenbo.amenbo-dev-2131",
	}
	if len(got) != len(want) {
		t.Fatalf("paths = %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("paths[%d] = %q, want %q", i, got[i], want[i])
		}
	}
}

// TestReclaimTakesBothHalvesAndSpareTheRest exercises the removal itself, since that is the half
// that touches disk: an instance's bundle and store go, and a path that was never there is not an
// error to report.
func TestReclaimTakesBothHalvesAndSpareTheRest(t *testing.T) {
	root := t.TempDir()
	home := filepath.Join(root, "home")
	apps := filepath.Join(root, "Applications")
	paths := taskDevGUIPaths(home, apps, "2131")
	for _, p := range paths {
		if err := os.MkdirAll(filepath.Join(p, "inner"), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	// A neighbouring instance must survive: teardown is per task, and the numbers are prefixes
	// of one another the moment a task id gains a digit.
	neighbour := taskDevGUIPaths(home, apps, "21310")[1]
	if err := os.MkdirAll(neighbour, 0o755); err != nil {
		t.Fatal(err)
	}

	reclaim(append(paths, filepath.Join(apps, "never-built.app")))

	for _, p := range paths {
		if _, err := os.Lstat(p); err == nil {
			t.Errorf("%s survived teardown", p)
		}
	}
	if _, err := os.Lstat(neighbour); err != nil {
		t.Errorf("the neighbouring instance was taken too: %v", err)
	}
}

func TestCopyTreeCarriesTheContentsAndSkipsWhatIsNotAFile(t *testing.T) {
	src := filepath.Join(t.TempDir(), "src")
	if err := os.MkdirAll(filepath.Join(src, "plugins", "hello"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(src, "store.sqlite"), []byte("store"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(src, "plugins", "hello", "manifest.json"), []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
	// A symlink is the one entry a store should never carry into a clone: following it would
	// copy from outside the tree, and keeping it would point the clone back at the original.
	if err := os.Symlink(filepath.Join(src, "store.sqlite"), filepath.Join(src, "elsewhere")); err != nil {
		t.Fatal(err)
	}

	dst := filepath.Join(t.TempDir(), "dst")
	if err := copyTree(src, dst); err != nil {
		t.Fatalf("copyTree: %v", err)
	}

	body, err := os.ReadFile(filepath.Join(dst, "plugins", "hello", "manifest.json"))
	if err != nil || string(body) != "{}" {
		t.Fatalf("nested file = %q, %v", body, err)
	}
	info, err := os.Stat(filepath.Join(dst, "store.sqlite"))
	if err != nil {
		t.Fatalf("store: %v", err)
	}
	if info.Mode().Perm() != 0o600 {
		t.Errorf("store mode = %v, want 0600 — a restricted store stays restricted", info.Mode().Perm())
	}
	if _, err := os.Lstat(filepath.Join(dst, "elsewhere")); err == nil {
		t.Error("the symlink was copied; it should have been skipped")
	}
}
