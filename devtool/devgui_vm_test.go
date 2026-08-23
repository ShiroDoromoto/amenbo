package main

import (
	"os"
	"strings"
	"testing"
)

// TestGuestPathsMirrorTheHost pins the guest layout to the host's. Everything that addresses an
// instance does it by path — the bundle to replace, the store to seed, the process executed out of
// the bundle — so the two sides differing by a directory is a command that finds nothing in the
// guest and reports the instance missing.
func TestGuestPathsMirrorTheHost(t *testing.T) {
	if got, want := vmTaskDevBundle("2131"), "/Applications/amenbo (dev 2131).app"; got != want {
		t.Errorf("guest bundle = %q, want %q", got, want)
	}
	if got, want := vmTaskDevAppData("2131"),
		"/Users/admin/Library/Application Support/work.amenbo.amenbo-dev-2131"; got != want {
		t.Errorf("guest app-data = %q, want %q", got, want)
	}
	// The same two the host holds, under the host's own home: same names, different machine.
	host := taskDevGUIPaths("/Users/admin", macAppsDir, "2131")
	if host[0] != vmTaskDevBundle("2131") || host[1] != vmTaskDevAppData("2131") {
		t.Errorf("host paths %v do not mirror the guest's (%q, %q)", host, vmTaskDevBundle("2131"), vmTaskDevAppData("2131"))
	}
}

// TestTaskDevExecutableMatchesTheBuild pins the executable name literally, the way the bundle and
// app-data names are pinned: it is the Makefile's GUI_DEV_BIN, and it is what `pgrep -x` is asked
// in the guest. A rename on one side leaves the quit-before-replace looking at nothing, which is
// silent — the app is replaced under itself and writes its store back over the new bundle.
func TestTaskDevExecutableMatchesTheBuild(t *testing.T) {
	if got, want := taskDevExecutable("2131"), "amenbo-app-dev-2131"; got != want {
		t.Errorf("executable name = %q, want %q", got, want)
	}
}

// TestGUIBundleBuildDirIsTheMakefiles reads the one path this file does not own. The bundle sent to
// the guest is the one tauri just wrote, and where that is is the Makefile's BUNDLE_DIR — a move
// there would otherwise be found only as "no bundle built for task <id>" after a full build.
func TestGUIBundleBuildDirIsTheMakefiles(t *testing.T) {
	mk, err := os.ReadFile("../Makefile")
	if err != nil {
		t.Skipf("no Makefile beside devtool to read: %v", err)
	}
	want := ""
	for _, line := range strings.Split(string(mk), "\n") {
		if rest, ok := strings.CutPrefix(line, "BUNDLE_DIR :="); ok {
			want = strings.TrimSpace(rest)
			break
		}
	}
	if want == "" {
		t.Fatal("no BUNDLE_DIR in the Makefile — the build's bundle path is no longer where this reads it")
	}
	if guiBundleBuildDir != want {
		t.Errorf("guiBundleBuildDir = %q, Makefile BUNDLE_DIR = %q", guiBundleBuildDir, want)
	}
}

// TestShqSurvivesTheNamesABundleCarries covers the quoting the whole guest side rests on: what is
// sent over ssh is joined and handed to a shell in there, and a dev bundle's name holds spaces and
// parentheses — unquoted, the shell reads them as separate words and a subshell.
func TestShqSurvivesTheNamesABundleCarries(t *testing.T) {
	if got, want := shq("/Applications/amenbo (dev 2131).app"), `'/Applications/amenbo (dev 2131).app'`; got != want {
		t.Errorf("shq(bundle) = %s, want %s", got, want)
	}
	// A quote of its own closes the quoting and would hand the rest to the shell as code.
	if got, want := shq("it's"), `'it'\''s'`; got != want {
		t.Errorf("shq(apostrophe) = %s, want %s", got, want)
	}
}

// TestInstancesFromReadsListingsAlone holds the reading the guest sweep depends on: what is there
// comes from the two listings and nothing else. Nothing in the guest can be stat'ed from here, so a
// scan that reached for the disk would have no answer to give on that machine at all.
func TestInstancesFromReadsListingsAlone(t *testing.T) {
	got := instancesFrom(
		[]string{"amenbo (dev 2131).app", "amenbo (dev 2140).app", "Amenbo.app", "amenbo (dev wip).app"},
		[]string{"work.amenbo.amenbo-dev-2131", "work.amenbo.amenbo-dev-2135", "work.amenbo.amenbo-dev"},
		"/Users/admin", macAppsDir, []string{"2135"})

	want := []struct {
		id    string
		live  bool
		paths []string
	}{
		// Both halves there.
		{"2131", false, []string{"/Applications/amenbo (dev 2131).app", "/Users/admin/Library/Application Support/work.amenbo.amenbo-dev-2131"}},
		// A store whose bundle was already taken is still an instance to reclaim.
		{"2135", true, []string{"/Users/admin/Library/Application Support/work.amenbo.amenbo-dev-2135"}},
		// A bundle whose store was removed by hand, likewise.
		{"2140", false, []string{"/Applications/amenbo (dev 2140).app"}},
	}
	if len(got) != len(want) {
		t.Fatalf("scanned %+v, want %d instances", got, len(want))
	}
	for i := range want {
		if got[i].id != want[i].id || got[i].live != want[i].live {
			t.Errorf("instance %d = %s live=%v, want %s live=%v", i, got[i].id, got[i].live, want[i].id, want[i].live)
		}
		if len(got[i].paths) != len(want[i].paths) {
			t.Fatalf("instance %s paths = %v, want %v", got[i].id, got[i].paths, want[i].paths)
		}
		for j := range want[i].paths {
			if got[i].paths[j] != want[i].paths[j] {
				t.Errorf("instance %s path %d = %q, want %q", got[i].id, j, got[i].paths[j], want[i].paths[j])
			}
		}
	}
}

// TestVMTaskDevGUIPathsTakeBothHalvesInOrder pins the guest teardown to the same two halves, in the
// same order, the host teardown takes: the bundle first, the store second.
func TestVMTaskDevGUIPathsTakeBothHalvesInOrder(t *testing.T) {
	got := vmTaskDevGUIPaths("2131")
	want := taskDevGUIPaths(vmGuestHome, macAppsDir, "2131")
	if len(got) != len(want) {
		t.Fatalf("guest paths = %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("guest path %d = %q, want %q", i, got[i], want[i])
		}
	}
}
