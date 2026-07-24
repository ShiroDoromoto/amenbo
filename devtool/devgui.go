package main

import (
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"strings"
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

// appDataDir is where a macOS build keeps the store of one app-data name, mirroring what core
// resolves through the directories crate: `work.amenbo.<app name>` under Application Support.
func appDataDir(home, appName string) string {
	return filepath.Join(home, "Library", "Application Support", "work.amenbo."+appName)
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
	reclaim(taskDevGUIPaths(home, macAppsDir, id))
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
