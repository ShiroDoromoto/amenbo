package main

import (
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"time"
)

// The dev GUI comes in two shapes. One is shared and permanent: a single installed bundle on the
// machine, the place a grown setup lives. The other is a throwaway a task owns, with its own bundle
// identifier, product name and app-data, so two parallel sessions look at their own work instead of
// installing over each other.
//
// devtool provisions and deletes an instance; the root Makefile builds it (`make install-gui-dev
// AMB-T-ID=<id>`), so a task that never opens a GUI never pays for a bundle. That means the names
// below are the same strings the Makefile's GUI_DEV_ block derives, and the two sides have to agree
// literally — a name that drifts leaves a 38MB bundle and a store behind on every teardown, which is
// the cost the teardown exists to reclaim.
//
// All of it is macOS-only, because that is where the dev GUI is installed at all: the Makefile's
// targets speak /Applications, osascript and codesign. Elsewhere these are no-ops.
const (
	sharedDevAppData = "amenbo-dev"
	sharedDevBundle  = "amenbo (dev)"
	macAppsDir       = "/Applications"
)

// taskDevAppData is the app-data name of the instance task id owns.
func taskDevAppData(id string) string { return sharedDevAppData + "-" + id }

// taskDevBundle is the product name — and so the bundle's file name — of that same instance.
func taskDevBundle(id string) string { return "amenbo (dev " + id + ")" }

// taskIDFromCheckout names the task a checkout belongs to, and "" for the main one. A task worktree
// sits at `<repo-name>-worktrees/<id>` (see paths), so the directory name is the id — read back
// through the same canonical form task start pinned it to, or a hand-made directory beside the real
// ones would be taken for a task.
func taskIDFromCheckout(root string) string {
	if !strings.HasSuffix(filepath.Base(filepath.Dir(root)), "-worktrees") {
		return ""
	}
	id, err := canonicalID(filepath.Base(root))
	if err != nil {
		return ""
	}
	return id
}

// devGUIBundleNames are the dev GUI bundles a checkout may launch, most specific first: a task
// worktree reaches for its own instance and falls back to the shared dev app only when it has not
// built one, and the main checkout has only the shared app to reach.
func devGUIBundleNames(root string) []string {
	if id := taskIDFromCheckout(root); id != "" {
		return []string{taskDevBundle(id), sharedDevBundle}
	}
	return []string{sharedDevBundle}
}

// devGUIBuildCommand is the command that builds the dev GUI this checkout launches: a task worktree
// builds its own throwaway instance, the main checkout the shared dev app. A message that asks for a
// build has to name this one — bare `make install-gui-dev` installs over the shared app, the one
// permanent setup a task is meant to keep its hands off, and a task session told to run it would do
// exactly the thing the per-task instance exists to make impossible.
func devGUIBuildCommand(root string) string {
	if id := taskIDFromCheckout(root); id != "" {
		return "make install-gui-dev AMB-T-ID=" + id
	}
	return "make install-gui-dev"
}

// appDataDirName is the directory one app-data name occupies, the way core's directories crate
// spells it: `work.amenbo.<app name>`.
func appDataDirName(appName string) string { return "work.amenbo." + appName }

// appSupportDir is the macOS folder every app-data directory sits in.
func appSupportDir(home string) string {
	return filepath.Join(home, "Library", "Application Support")
}

// appDataDir is where a macOS build keeps the store of one app-data name, mirroring what core
// resolves through the directories crate: `work.amenbo.<app name>` under Application Support.
func appDataDir(home, appName string) string {
	return filepath.Join(appSupportDir(home), appDataDirName(appName))
}

// taskIDFromBundleName and taskIDFromAppDataName read a task id back out of what an instance is
// called on disk — the inverse of taskDevBundle / taskDevAppData, held to them by a round-trip test.
// Reading the id off the name is the only way to find an instance whose session never came back to
// finish it: nothing else on the machine records that it was ever created.
//
// Both go through canonicalID, so only digits are taken. That is the same line taskIDFromCheckout
// draws, and it matters more here: a hand-made `amenbo (dev wip).app` is somebody's own, and a sweep
// that read it as a task instance would delete it.
func taskIDFromBundleName(name string) string {
	rest, ok := strings.CutPrefix(name, "amenbo (dev ")
	if !ok {
		return ""
	}
	rest, ok = strings.CutSuffix(rest, ").app")
	if !ok {
		return ""
	}
	return digitsOnly(rest)
}

func taskIDFromAppDataName(name string) string {
	rest, ok := strings.CutPrefix(name, appDataDirName(sharedDevAppData)+"-")
	if !ok {
		return "" // the shared dev store itself lands here too, and it is nobody's to sweep
	}
	return digitsOnly(rest)
}

func digitsOnly(id string) string {
	canon, err := canonicalID(id)
	if err != nil {
		return ""
	}
	return canon
}

// taskDevGUIInstance is one throwaway instance found on disk, and whether a checkout still claims
// it. `live` is the whole safety of the sweep: an instance a worktree owns belongs to a session that
// may be looking at it right now.
type taskDevGUIInstance struct {
	id    string
	live  bool
	paths []string
}

// scanTaskDevGUIs finds every per-task instance present under `appsDir` and `home` and marks the
// ones `liveIDs` still claims. It reports what is actually on disk — an instance whose bundle was
// built but whose app-data was removed by hand is still an instance, and still worth reclaiming —
// so `paths` holds only the halves that exist.
//
// The two roots and the live set are arguments so the whole scan can be pointed at a temp dir; the
// caller is what binds it to /Applications and to `git worktree list`.
func scanTaskDevGUIs(home, appsDir string, liveIDs []string) []taskDevGUIInstance {
	live := make(map[string]bool, len(liveIDs))
	for _, id := range liveIDs {
		live[id] = true
	}
	found := map[string]bool{}
	for _, half := range []struct {
		dir  string
		idOf func(string) string
	}{
		{appsDir, taskIDFromBundleName},
		{appSupportDir(home), taskIDFromAppDataName},
	} {
		entries, err := os.ReadDir(half.dir)
		if err != nil {
			continue // nothing readable there is nothing to reclaim from there
		}
		for _, e := range entries {
			if id := half.idOf(e.Name()); id != "" {
				found[id] = true
			}
		}
	}
	ids := make([]string, 0, len(found))
	for id := range found {
		ids = append(ids, id)
	}
	sort.Strings(ids)

	instances := make([]taskDevGUIInstance, 0, len(ids))
	for _, id := range ids {
		inst := taskDevGUIInstance{id: id, live: live[id]}
		for _, p := range taskDevGUIPaths(home, appsDir, id) {
			if _, err := os.Lstat(p); err == nil {
				inst.paths = append(inst.paths, p)
			}
		}
		instances = append(instances, inst)
	}
	return instances
}

// taskDevGUIPaths lists everything an instance occupies on disk, in the order teardown removes it.
// Naming the set once is what keeps `task finish` and any future report reading the same disk; the
// two roots are arguments so a test can point the whole set at a temp dir.
func taskDevGUIPaths(home, appsDir, id string) []string {
	return []string{
		filepath.Join(appsDir, taskDevBundle(id)+".app"),
		appDataDir(home, taskDevAppData(id)),
	}
}

// provisionTaskDevGUI seeds the instance's app-data by cloning the shared dev store, so it opens on
// the setup grown in the shared app rather than an empty one — the same move a release rehearsal
// makes with a clone of the production store. It is best-effort in the sense the npm install is: by
// the time it runs the reservation, worktree and branch are all in place, so nothing here may fail a
// start. An app-data already sitting there is left alone; it is the session's own work from an
// earlier start, and a fresh clone would throw it away.
func provisionTaskDevGUI(worktree, id string) {
	if runtime.GOOS != "darwin" {
		return
	}
	if _, err := os.Stat(filepath.Join(worktree, "app", "package.json")); err != nil {
		return
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return
	}
	dst := appDataDir(home, taskDevAppData(id))
	src := appDataDir(home, sharedDevAppData)
	switch {
	case dirExists(dst):
		logf("  dev GUI : app-data %s is already there — left as it is", filepath.Base(dst))
	case !dirExists(src):
		logf("  dev GUI : no shared dev store to clone — the instance will open empty")
	default:
		if err := copyTree(src, dst); err != nil {
			logf("  dev GUI : warning — cloning the shared dev store failed (%v); the instance will open empty", err)
		} else {
			logf("  dev GUI : app-data %s cloned from the shared dev store", filepath.Base(dst))
		}
	}
	logf("  dev GUI : verify this task in its own app — `make install-gui-dev AMB-T-ID=%s` builds %q", id, taskDevBundle(id))
}

// removeTaskDevGUI deletes the instance: the installed bundle and its app-data both. This is the
// half of the arrangement that has to hold — an instance is ~38MB of bundle plus a store, so one
// left behind per task is how a disk fills quietly. It reports each path it removed and never fails
// the teardown, which by this point has already taken the worktree and branch.
func removeTaskDevGUI(id string) {
	if runtime.GOOS != "darwin" {
		return
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return
	}
	if !stopTaskDevGUI(id) {
		logf("  %s is still running — quit it, then `devtool devgui sweep --yes` reclaims it", taskDevBundle(id))
		return
	}
	reclaim(taskDevGUIPaths(home, macAppsDir, id))
}

// stopTaskDevGUI asks the instance to quit and reports whether it is gone, because removing the
// files under a running one does not remove the instance: it writes its store back on the way out,
// and what teardown reported as reclaimed is on disk again minutes later (observed 2026-07-24, on a
// session that left the app open). `make install-gui-dev` asks the same way before it replaces a
// bundle.
//
// It reports rather than forces, and the caller keeps its hands off an instance that says no. A
// half-removed instance is the worse outcome of the two: the quit is by **name**, so an instance
// whose bundle has already been deleted can no longer be reached — leaving both halves in place is
// what keeps the sweep able to come back for it.
func stopTaskDevGUI(id string) bool {
	if !taskDevGUIRunning(id) {
		return true
	}
	_, _ = run("", "osascript", "-e", fmt.Sprintf("quit app %q", taskDevBundle(id)))
	// Quitting is asynchronous: the app returns from the event before its process is gone.
	for range 15 {
		time.Sleep(200 * time.Millisecond)
		if !taskDevGUIRunning(id) {
			return true
		}
	}
	return false
}

// taskDevGUIRunning reports whether a process is running out of this instance's bundle. The process
// table is matched as plain text, not as a pattern: the bundle name holds parentheses, which a
// `pgrep -f` regex would read as a group and quietly match the wrong thing (or nothing).
func taskDevGUIRunning(id string) bool {
	out, err := run("", "ps", "-Ao", "args=")
	if err != nil {
		return false // unable to look is not evidence of running; the quit below is harmless either way
	}
	return strings.Contains(out, taskDevGUIProcessMarker(id))
}

// taskDevGUIProcessMarker is the substring only this instance's own processes carry: every one of
// them is executed out of its installed bundle, and all three builds share the process name
// `amenbo-app`, so the bundle path is the only thing that tells them apart.
func taskDevGUIProcessMarker(id string) string {
	return filepath.Join(macAppsDir, taskDevBundle(id)+".app") + "/"
}

// reclaim removes each path that is actually there and reports it, leaving the rest untouched — an
// instance nobody built has nothing to reclaim, and saying so would only read as noise. Split out so
// the removal can be exercised against a temp dir instead of the real /Applications.
func reclaim(paths []string) {
	for _, path := range paths {
		if _, err := os.Lstat(path); err != nil {
			continue
		}
		if err := os.RemoveAll(path); err != nil {
			logf("  warning: %s is still there (%v) — remove it by hand", path, err)
			continue
		}
		logf("  removed the task's dev GUI: %s", path)
	}
}

// devGUISweep reports every per-task instance on this machine and, with `apply`, reclaims the ones
// no worktree claims any more. It exists because `task finish` is the only thing that deletes an
// instance, and a session that dies — or one that never runs it — leaves ~38MB of bundle plus a
// store behind under a number nobody will type again.
//
// **A live instance is never touched, and never offered.** That is the same hands-off line a
// pre-existing worktree draws: the worktree is the evidence a session owns this number, and whether
// that session is "really" still working is not this command's to judge. It follows that the live
// set has to be known — if git cannot answer, the sweep refuses outright rather than treating an
// unreadable answer as "nothing is claimed", which would delete every instance on the machine.
//
// Reporting is the default and removal is opt-in (`--yes`), because what goes is irreversible and
// the report is the whole review: an instance holds a store, and a clone of the shared dev setup is
// not a thing to delete on a verb the caller only half meant.
func devGUISweep(apply bool) error {
	if runtime.GOOS != "darwin" {
		logf("  the dev GUI is only installed on macOS — nothing to sweep here")
		return nil
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return err
	}
	root, _, _, err := paths("0") // the id is unused: this only needs the main repo root
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

	instances := scanTaskDevGUIs(home, macAppsDir, liveIDs)
	if len(instances) == 0 {
		logf("  no per-task dev GUI on this machine")
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
			logf("    %s", p)
		}
	}
	if len(orphans) == 0 {
		logf("  every instance still belongs to a worktree — nothing to reclaim")
		return nil
	}
	if !apply {
		logf("  %d orphan(s) — `devtool devgui sweep --yes` reclaims them (the ones in use are never touched)", len(orphans))
		return nil
	}
	reclaimed := 0
	for _, inst := range orphans {
		if !stopTaskDevGUI(inst.id) {
			logf("  task %s is still running — quit %s and run this again", inst.id, taskDevBundle(inst.id))
			continue
		}
		reclaim(inst.paths)
		reclaimed++
	}
	logf("✓ reclaimed %d of %d orphan instance(s)", reclaimed, len(orphans))
	return nil
}

func dirExists(path string) bool {
	info, err := os.Stat(path)
	return err == nil && info.IsDir()
}

// copyTree copies a directory tree, taking directories and regular files and nothing else. A store
// holds no symlinks or devices, and something that is one is not data worth carrying into a clone —
// following it would reach outside the tree being copied. Permissions come along, so a store that
// restricts itself stays restricted in the copy.
func copyTree(src, dst string) error {
	return filepath.WalkDir(src, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		info, err := d.Info()
		if err != nil {
			return err
		}
		target := filepath.Join(dst, rel)
		switch {
		case d.IsDir():
			return os.MkdirAll(target, info.Mode().Perm())
		case info.Mode().IsRegular():
			return copyFile(path, target, info.Mode().Perm())
		default:
			return nil
		}
	})
}

func copyFile(src, dst string, mode fs.FileMode) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.OpenFile(dst, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, mode)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return err
	}
	return out.Close()
}
