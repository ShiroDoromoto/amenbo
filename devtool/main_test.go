package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// setupRepo creates a temp git repo with one commit, chdir's into it for the
// duration of the test, and returns the repo root. taskFinish derives its
// worktree base dir from os.Getwd, so the test must run from inside the repo.
func setupRepo(t *testing.T) string {
	t.Helper()
	root := filepath.Join(t.TempDir(), "repo")
	if err := os.MkdirAll(root, 0o755); err != nil {
		t.Fatal(err)
	}
	for _, args := range [][]string{
		{"init", "-b", "main"},
		{"config", "user.email", "test@example.com"},
		{"config", "user.name", "Test"},
		{"commit", "--allow-empty", "-m", "init"},
	} {
		if _, err := run(root, "git", args...); err != nil {
			t.Fatalf("git %v: %v", args, err)
		}
	}

	prev, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Chdir(root); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chdir(prev) })
	return root
}

// addWorktree stamps out a per-task worktree the way taskStart would (minus the
// amenbo reservation), so taskFinish has something to tear down.
func addWorktree(t *testing.T, id string) {
	t.Helper()
	root, wtBase, worktree, err := paths(id)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(wtBase, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := worktreeAdd(root, worktree, branchName(id), "main"); err != nil {
		t.Fatalf("worktree add %s: %v", id, err)
	}
}

func TestTaskFinishRemovesEmptyBaseDir(t *testing.T) {
	setupRepo(t)
	addWorktree(t, "1")

	_, wtBase, _, err := paths("1")
	if err != nil {
		t.Fatal(err)
	}
	if err := taskFinish("1", "main", true, false); err != nil {
		t.Fatalf("taskFinish: %v", err)
	}
	if _, err := os.Stat(wtBase); !os.IsNotExist(err) {
		t.Fatalf("base dir %s should be gone, stat err = %v", wtBase, err)
	}
}

// TestEnsureAppDepsNoopWithoutAppPackageJson pins that ensureAppDeps is a silent no-op
// for a non-GUI checkout (no app/package.json) — it must not panic, error, shell out to
// npm, or create anything under a bare worktree dir, because core/CLI tasks are not to be
// slowed or broken by the dependency warm-up.
func TestEnsureAppDepsNoopWithoutAppPackageJson(t *testing.T) {
	wt := t.TempDir()
	ensureAppDeps(wt)

	entries, err := os.ReadDir(wt)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Fatalf("ensureAppDeps created %d entries under a non-GUI worktree; want none", len(entries))
	}
}

// TestCanonicalID pins that canonicalID accepts only the conversational number
// (optionally '#'-prefixed) and rejects ULIDs and id-prefixes: forcing the number is what
// keeps the worktree dir and branch name canonical, so a second `task start` for the same
// task in a different reference form cannot slip past the "already exists" guard. Leading
// zeros are kept verbatim — the rule is digits only, and amenbo resolves the number
// either way — and the ULID among the rejects is the form that would otherwise
// double-start a task.
func TestCanonicalID(t *testing.T) {
	ok := map[string]string{
		"696":   "696",
		"#696":  "696",
		"1":     "1",
		"00042": "00042",
	}
	for in, want := range ok {
		got, err := canonicalID(in)
		if err != nil {
			t.Fatalf("canonicalID(%q) unexpected error: %v", in, err)
		}
		if got != want {
			t.Fatalf("canonicalID(%q) = %q, want %q", in, got, want)
		}
	}
	bad := []string{
		"",
		"#",
		"01KWKVTAYDYPHB93SZE058PXN7",
		"01kwkv",
		"696a",
		"6-96",
		"task/696",
	}
	for _, in := range bad {
		if got, err := canonicalID(in); err == nil {
			t.Fatalf("canonicalID(%q) = %q, want error", in, got)
		}
	}
}

// TestTaskFinishKeepsBaseDirWithOtherWorktree pins that tearing one task down leaves the
// sibling base dir, and the other task's worktree in it, untouched.
func TestTaskFinishKeepsBaseDirWithOtherWorktree(t *testing.T) {
	setupRepo(t)
	addWorktree(t, "1")
	addWorktree(t, "2")

	_, wtBase, _, err := paths("1")
	if err != nil {
		t.Fatal(err)
	}
	if err := taskFinish("1", "main", true, false); err != nil {
		t.Fatalf("taskFinish: %v", err)
	}
	if _, err := os.Stat(wtBase); err != nil {
		t.Fatalf("base dir %s should remain (task 2 still lives there): %v", wtBase, err)
	}
	_, _, wt2, err := paths("2")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(wt2); err != nil {
		t.Fatalf("task 2 worktree %s should remain: %v", wt2, err)
	}
}

// TestVerifyReserved pins the check that gates `task start`: it passes for an in_progress
// task and fails otherwise, since status is the whole guard — a task still todo means the
// reservation did not take.
func TestVerifyReserved(t *testing.T) {
	if err := verifyReserved("804", task{Status: "in_progress"}); err != nil {
		t.Fatalf("in_progress task should pass: %v", err)
	}
	bad := map[string]task{
		"todo (reserve did not take)": {Status: "todo"},
		"done":                        {Status: "done"},
		"blocked":                     {Status: "blocked"},
	}
	for name, tk := range bad {
		if err := verifyReserved("804", tk); err == nil {
			t.Fatalf("%s should fail the reservation check", name)
		}
	}
}

// TestWorktreeConflictNamesTheAccident pins that the refusal names which accident a
// pre-existing worktree is: another session at work reads nothing like a worktree you
// forgot to tear down, and a refusal that cannot tell them apart is one an agent waves
// past.
func TestWorktreeConflictNamesTheAccident(t *testing.T) {
	busy := worktreeConflict("1578", "/tmp/wt/1578", "in_progress").Error()
	if !strings.Contains(busy, "ANOTHER SESSION") || !strings.Contains(busy, "hands off") {
		t.Fatalf("a worktree on an in_progress task must be called out as another session's: %s", busy)
	}
	if strings.Contains(busy, "task finish 1578`.\n") {
		t.Fatalf("tearing it down must not read as the remedy: %s", busy)
	}
	for _, status := range []string{"todo", "done", "blocked", "backlog unreadable"} {
		mine := worktreeConflict("1578", "/tmp/wt/1578", status).Error()
		if strings.Contains(mine, "ANOTHER SESSION") {
			t.Fatalf("status %q is not another session at work: %s", status, mine)
		}
		if !strings.Contains(mine, "devtool task finish 1578") {
			t.Fatalf("status %q should point at teardown: %s", status, mine)
		}
	}
}
