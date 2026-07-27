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
