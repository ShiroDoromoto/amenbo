package main

import (
	"os"
	"os/exec"
	"path/filepath"
)

// ensureAppDeps installs the GUI app's node_modules in a freshly created worktree so
// `cd app && npm run typecheck/build/test` works without a manual `npm ci`, keeping a
// real (gitignored) node_modules per worktree — no symlink — so parallel sessions stay
// isolated. It is strictly best-effort: a non-GUI checkout (no app/package.json), a
// missing npm, or a failed install must never fail `task start`, since the
// worktree/branch/reservation are already in place by the time this runs — so on any
// problem it only warns and leaves `npm ci` to the developer.
func ensureAppDeps(worktree string) {
	app := filepath.Join(worktree, "app")
	if _, err := os.Stat(filepath.Join(app, "package.json")); err != nil {
		return
	}
	if _, err := exec.LookPath("npm"); err != nil {
		logf("  deps    : skipped — npm not found; run `cd app && npm ci` by hand")
		return
	}
	logf("  deps    : installing app/node_modules (npm ci)…")
	if _, err := run(app, "npm", "ci"); err != nil {
		logf("  deps    : warning — npm ci failed (%v); run `cd app && npm ci` by hand", err)
		return
	}
	logf("  deps    : app/node_modules ready")
}
