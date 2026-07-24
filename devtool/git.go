package main

import (
	"path/filepath"
	"strings"
)

// gitRoot returns the MAIN worktree root for the repo containing dir — not the
// current linked worktree. This is what makes devtool behave the same whether
// it is invoked from the main checkout or from inside a per-task worktree
// (where the naive --show-toplevel would point at the worktree itself and the
// .worktrees/<id> layout would resolve wrong). --git-common-dir resolves to the
// main repo's `.git`, whose parent is the main worktree root.
func gitRoot(dir string) (string, error) {
	common, err := run(dir, "git", "rev-parse", "--path-format=absolute", "--git-common-dir")
	if err != nil {
		return "", err
	}
	return filepath.Dir(common), nil
}

// worktreeAdd creates a new worktree at path checked out to a fresh branch
// branched from base.
func worktreeAdd(root, path, branch, base string) error {
	_, err := run(root, "git", "worktree", "add", path, "-b", branch, base)
	return err
}

// worktreeRemove detaches the worktree at path. force discards local changes.
func worktreeRemove(root, path string, force bool) error {
	args := []string{"worktree", "remove", path}
	if force {
		args = append(args, "--force")
	}
	_, err := run(root, "git", args...)
	return err
}

// worktreeCheckouts lists the checkouts git knows about, the main one included. The porcelain form
// is parsed rather than the human one because it is the stable contract: one `worktree <path>` line
// opens each record, and nothing else starts with that word.
//
// An error here is not "no worktrees": the caller must be able to tell "git could not answer" from
// "git says none", because the second reads as "everything on disk is an orphan".
func worktreeCheckouts(root string) ([]string, error) {
	out, err := run(root, "git", "worktree", "list", "--porcelain")
	if err != nil {
		return nil, err
	}
	var paths []string
	for _, line := range strings.Split(out, "\n") {
		if p, ok := strings.CutPrefix(line, "worktree "); ok {
			paths = append(paths, strings.TrimSpace(p))
		}
	}
	return paths, nil
}

// branchExists reports whether refs/heads/branch is present.
func branchExists(root, branch string) bool {
	_, err := run(root, "git", "show-ref", "--verify", "--quiet", "refs/heads/"+branch)
	return err == nil
}

// branchDelete deletes branch. force allows deleting an unmerged branch.
func branchDelete(root, branch string, force bool) error {
	flag := "-d"
	if force {
		flag = "-D"
	}
	_, err := run(root, "git", "branch", flag, branch)
	return err
}

// isClean reports whether the worktree at path has no uncommitted changes.
func isClean(path string) (bool, error) {
	out, err := run(path, "git", "status", "--porcelain")
	if err != nil {
		return false, err
	}
	return strings.TrimSpace(out) == "", nil
}

// isMerged reports whether branch is fully contained in base (its tip is an ancestor
// of base), i.e. safe to delete — `--is-ancestor` exits 1 (non-fatal) when it is not
// one, which reads as "not merged" rather than a hard error.
func isMerged(root, branch, base string) (bool, error) {
	_, err := run(root, "git", "merge-base", "--is-ancestor", branch, base)
	if err == nil {
		return true, nil
	}
	return false, nil
}
