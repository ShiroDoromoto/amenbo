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
