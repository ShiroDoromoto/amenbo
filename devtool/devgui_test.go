package main

import (
	"os"
	"path/filepath"
	"strings"
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

// TestDevGUIBundleNamesPreferTheCheckoutsOwnInstance holds the launch order: a task worktree opens
// its own app, and only a checkout that owns no instance reaches the shared one.
func TestDevGUIBundleNamesPreferTheCheckoutsOwnInstance(t *testing.T) {
	for root, want := range map[string][]string{
		"/w/amenbo-worktrees/2131": {"amenbo (dev 2131)", "amenbo (dev)"},
		"/w/amenbo":                {"amenbo (dev)"},
		"/w/amenbo-worktrees/wip":  {"amenbo (dev)"},
	} {
		got := devGUIBundleNames(root)
		if len(got) != len(want) || got[0] != want[0] {
			t.Errorf("devGUIBundleNames(%q) = %v, want %v", root, got, want)
		}
	}
}

// TestDevGUIBuildCommandNamesTheCheckoutsOwnInstance holds the advice a failed launch gives: from a
// task worktree it has to carry the id, because the command without one builds over the shared dev
// app — the permanent setup the per-task instance exists to keep a task away from.
func TestDevGUIBuildCommandNamesTheCheckoutsOwnInstance(t *testing.T) {
	for root, want := range map[string]string{
		"/w/amenbo-worktrees/2131": "make install-gui-dev AMB-T-ID=2131",
		"/w/amenbo":                "make install-gui-dev",
		"/w/amenbo-worktrees/wip":  "make install-gui-dev",
	} {
		if got := devGUIBuildCommand(root); got != want {
			t.Errorf("devGUIBuildCommand(%q) = %q, want %q", root, got, want)
		}
	}
}

// TestInstanceNamesRoundTrip holds the sweep's reader to the builder it inverts: the names are the
// only record that an instance exists, so a rename on the build side that the reader does not follow
// makes every instance invisible — and invisible is exactly what the sweep exists to end.
func TestInstanceNamesRoundTrip(t *testing.T) {
	for _, id := range []string{"1", "2131", "21310"} {
		if got := taskIDFromBundleName(taskDevBundle(id) + ".app"); got != id {
			t.Errorf("bundle name of %s reads back as %q", id, got)
		}
		if got := taskIDFromAppDataName(appDataDirName(taskDevAppData(id))); got != id {
			t.Errorf("app-data name of %s reads back as %q", id, got)
		}
	}
	// What must NOT read as an instance. The shared dev app is permanent, and a name carrying
	// anything but digits is someone's own build — the sweep deletes what it recognises, so
	// recognising too much is the expensive direction.
	for _, name := range []string{"amenbo (dev).app", "amenbo.app", "amenbo (dev wip).app", "amenbo (dev 21)", "Safari.app"} {
		if got := taskIDFromBundleName(name); got != "" {
			t.Errorf("%q was read as instance %q", name, got)
		}
	}
	for _, name := range []string{"work.amenbo.amenbo", "work.amenbo.amenbo-dev", "work.amenbo.amenbo-dev-wip", "com.other.app"} {
		if got := taskIDFromAppDataName(name); got != "" {
			t.Errorf("%q was read as instance %q", name, got)
		}
	}
}

// TestProcessMarkerIsTheBundlePath pins what a running instance is recognised by. The three builds
// share the process name `amenbo-app`, so anything shorter than the bundle path would read the
// production app — or a neighbouring instance — as this one.
func TestProcessMarkerIsTheBundlePath(t *testing.T) {
	got := taskDevGUIProcessMarker("2131")
	if want := "/Applications/amenbo (dev 2131).app/"; got != want {
		t.Errorf("marker = %q, want %q", got, want)
	}
	if strings.HasPrefix(taskDevGUIProcessMarker("21310"), got) {
		t.Error("the marker of 21310 starts with 2131's — one instance would be read as the other")
	}
}

// TestPIDRunningFromPicksTheAppProcess is the whole value of the pid lookup: the answer has to be
// the process that owns a window. The table is the real shapes of `ps -Ao pid=,args=` — the three
// builds side by side under one process name, plus the two ways the bundle path shows up without
// being the app (as an argument of something else, and as a neighbouring instance whose id starts
// with this one's).
func TestPIDRunningFromPicksTheAppProcess(t *testing.T) {
	const ps = `  501 /Users/x/Applications/amenbo.app/Contents/MacOS/amenbo-app
  777 /Applications/amenbo (dev).app/Contents/MacOS/amenbo-app
  999 /Applications/amenbo (dev 21310).app/Contents/MacOS/amenbo-app
 1234 /Applications/amenbo (dev 2131).app/Contents/MacOS/amenbo-app
 1235 /usr/bin/codesign --force /Applications/amenbo (dev 2131).app/
 1236 /Applications/amenbo (dev 2131).app/Contents/Resources/something`
	for bundle, want := range map[string]int{
		taskDevBundle("2131"):  1234,
		taskDevBundle("21310"): 999,
		sharedDevBundle:        777,
		taskDevBundle("404"):   0,
	} {
		if got := pidRunningFrom(ps, devGUIExecPrefix(bundle)); got != want {
			t.Errorf("pid of %q = %d, want %d", bundle, got, want)
		}
	}
	// The teardown's question is the wider one — anything running out of the bundle holds it back —
	// so its prefix takes the process under Resources too. What neither takes is the `codesign`:
	// naming the path is not running out of it, and a teardown held back by a build step that has
	// already finished leaves the instance for nobody to reclaim.
	if got := pidRunningFrom(ps, taskDevGUIProcessMarker("2131")); got != 1234 {
		t.Errorf("teardown's lookup = %d, want the app process 1234 (the lowest of 1234/1236)", got)
	}
	if got := pidRunningFrom(" 1235 /usr/bin/codesign --force "+taskDevGUIProcessMarker("2131"), taskDevGUIProcessMarker("2131")); got != 0 {
		t.Errorf("a codesign naming the bundle was read as the instance running (pid %d)", got)
	}
}

// TestPIDRunningFromPrefersTheOlderCopy pins which one is answered when a second copy was launched
// over the first: the older process, whose window is the one that has been on screen.
func TestPIDRunningFromPrefersTheOlderCopy(t *testing.T) {
	const ps = ` 4321 /Applications/amenbo (dev 2131).app/Contents/MacOS/amenbo-app
 1234 /Applications/amenbo (dev 2131).app/Contents/MacOS/amenbo-app`
	if got := pidRunningFrom(ps, devGUIExecPrefix(taskDevBundle("2131"))); got != 1234 {
		t.Errorf("pid = %d, want the older 1234", got)
	}
}

// TestScanTaskDevGUIsSeparatesLiveFromOrphan is the sweep's whole judgment: an instance a worktree
// still claims is in use, and only what nothing claims is offered for removal. It also pins that a
// half-removed instance is still found — one half left behind is still disk nobody will reclaim by
// hand.
func TestScanTaskDevGUIsSeparatesLiveFromOrphan(t *testing.T) {
	root := t.TempDir()
	home := filepath.Join(root, "home")
	apps := filepath.Join(root, "Applications")
	mk := func(path string) {
		if err := os.MkdirAll(path, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	mk(apps)
	mk(appSupportDir(home))
	for _, id := range []string{"2131", "2135"} {
		for _, p := range taskDevGUIPaths(home, apps, id) {
			mk(p)
		}
	}
	// Only the bundle: the app-data was removed by hand, or never opened.
	mk(taskDevGUIPaths(home, apps, "2140")[0])
	// Neither is an instance: the shared dev store is permanent, and a stranger's app is not ours.
	mk(appDataDir(home, sharedDevAppData))
	mk(filepath.Join(apps, "Safari.app"))

	got := scanTaskDevGUIs(home, apps, []string{"2135"})

	want := []taskDevGUIInstance{
		{id: "2131", live: false, paths: taskDevGUIPaths(home, apps, "2131")},
		{id: "2135", live: true, paths: taskDevGUIPaths(home, apps, "2135")},
		{id: "2140", live: false, paths: taskDevGUIPaths(home, apps, "2140")[:1]},
	}
	if len(got) != len(want) {
		t.Fatalf("scanned %+v, want %+v", got, want)
	}
	for i := range want {
		if got[i].id != want[i].id || got[i].live != want[i].live {
			t.Errorf("instance %d = %s live=%v, want %s live=%v", i, got[i].id, got[i].live, want[i].id, want[i].live)
		}
		if len(got[i].paths) != len(want[i].paths) {
			t.Errorf("instance %s paths = %v, want %v", got[i].id, got[i].paths, want[i].paths)
		}
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

// TestParseUIAutoWindowReadsTheBoundsUIAutoPrints holds the shot to uiauto's output format. The two
// sides meet on one line of text, and a mismatch does not fail loudly: the shot would be of some
// other rectangle, or the origin printed beside it would send a click somewhere nobody meant.
func TestParseUIAutoWindowReadsTheBoundsUIAutoPrints(t *testing.T) {
	windows, err := parseUIAutoWindow("12345 0 38 1512 944\n")
	if err != nil {
		t.Fatalf("parseUIAutoWindow: %v", err)
	}
	if len(windows) != 1 {
		t.Fatalf("windows = %d, want 1", len(windows))
	}
	got := windows[0]
	if got != (devGUIWindow{id: 12345, x: 0, y: 38, w: 1512, h: 944}) {
		t.Errorf("window = %+v, want the id, origin and size uiauto printed", got)
	}
}

// TestParseUIAutoWindowKeepsEveryWindowInOrder pins that a second window is not silently dropped:
// the caller shoots the first and says how many there were, which is what makes a shot of the wrong
// one visible instead of merely wrong.
func TestParseUIAutoWindowKeepsEveryWindowInOrder(t *testing.T) {
	windows, err := parseUIAutoWindow("1 0 0 800 600\n2 100 50 400 300\n")
	if err != nil {
		t.Fatalf("parseUIAutoWindow: %v", err)
	}
	if len(windows) != 2 || windows[0].id != 1 || windows[1].id != 2 {
		t.Errorf("windows = %+v, want both, in the order uiauto listed them", windows)
	}
}

// TestParseUIAutoWindowRefusesWhatItCannotRead is the other half: an answer that is not the format,
// and the empty answer uiauto gives for a window behind another Space. Both have to come back as an
// error — a zero window id would shoot the whole screen, which is the mistake this command exists to
// end.
func TestParseUIAutoWindowRefusesWhatItCannotRead(t *testing.T) {
	for name, out := range map[string]string{
		"nothing on screen": "",
		"short line":        "12345 0 38\n",
		"unnumbered id":     "win1 0 38 1512 944\n",
		"unnumbered bounds": "12345 0 38 wide 944\n",
	} {
		if _, err := parseUIAutoWindow(out); err == nil {
			t.Errorf("%s: parseUIAutoWindow(%q) = nil error, want a refusal", name, out)
		}
	}
}
