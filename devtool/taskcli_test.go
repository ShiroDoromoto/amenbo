package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestSplitDoubleDashHandsTheRestOver pins the one rule that keeps devtool's flags and amenbo's
// apart: everything after the first `--` belongs to amenbo, including a second `--`, and a line
// without one is refused rather than guessed at.
func TestSplitDoubleDashHandsTheRestOver(t *testing.T) {
	for _, tc := range []struct {
		name string
		args []string
		head []string
		rest []string
		ok   bool
	}{
		{"flags then arguments", []string{"696", "--no-build", "--", "task", "list", "--json"},
			[]string{"696", "--no-build"}, []string{"task", "list", "--json"}, true},
		{"a second dash-dash is amenbo's", []string{"696", "--", "task", "add", "--", "x"},
			[]string{"696"}, []string{"task", "add", "--", "x"}, true},
		{"nothing after it", []string{"696", "--"}, []string{"696"}, []string{}, true},
		{"none at all", []string{"696", "task", "list"}, []string{"696", "task", "list"}, nil, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			head, rest, ok := splitDoubleDash(tc.args)
			if ok != tc.ok {
				t.Fatalf("ok = %v, want %v", ok, tc.ok)
			}
			if strings.Join(head, " ") != strings.Join(tc.head, " ") {
				t.Errorf("head = %v, want %v", head, tc.head)
			}
			if strings.Join(rest, " ") != strings.Join(tc.rest, " ") {
				t.Errorf("rest = %v, want %v", rest, tc.rest)
			}
		})
	}
}

// TestTaskCLIBinIsTheWorktreesOwnBuild holds the binary to the ordinary debug build a checkout
// already produces. A path of its own here would mean a second build per task — the cost this
// command exists to avoid.
func TestTaskCLIBinIsTheWorktreesOwnBuild(t *testing.T) {
	got := taskCLIBin("/tmp/amenbo-worktrees/696")
	want := filepath.Join("/tmp/amenbo-worktrees/696", "target", "debug", "amenbo")
	if got != want {
		t.Errorf("cli = %q, want %q", got, want)
	}
}

// TestTaskCLIRefusesWithoutAWorktree keeps the failure on the missing worktree, where it can be
// acted on, rather than letting a build run in a directory that is not there. It also covers the
// non-macOS refusal, since a checkout on either OS runs this test.
func TestTaskCLIRefusesWithoutAWorktree(t *testing.T) {
	setupRepo(t)
	if _, err := taskCLI("696", true, []string{"task", "list"}); err == nil {
		t.Fatal("a task with no worktree ran a CLI anyway")
	}
}

// TestRunThroughReportsTheExitCode holds the pass-through's contract: a non-zero command is an
// answer to hand back, not devtool's own failure, and only a command that cannot be run at all is
// an error.
func TestRunThroughReportsTheExitCode(t *testing.T) {
	dir := t.TempDir()
	code, err := runThrough(dir, nil, "sh", "-c", "exit 3")
	if err != nil {
		t.Fatalf("a command that ran is not an error: %v", err)
	}
	if code != 3 {
		t.Errorf("code = %d, want 3", code)
	}
	if _, err := runThrough(dir, nil, filepath.Join(dir, "not-a-program")); err == nil {
		t.Error("a command that could not be run should be an error")
	}
}

// TestRunThroughCarriesTheStoreEnv proves the whole mechanism in one line: the CLI is pointed at a
// store by the environment, so what the child sees in AMENBO_HOME is what it will open.
func TestRunThroughCarriesTheStoreEnv(t *testing.T) {
	dir := t.TempDir()
	out := filepath.Join(dir, "seen")
	code, err := runThrough(dir, []string{storeEnv + "=" + dir},
		"sh", "-c", "printf '%s' \"$"+storeEnv+"\" > "+out)
	if err != nil || code != 0 {
		t.Fatalf("run: code=%d err=%v", code, err)
	}
	seen, err := os.ReadFile(out)
	if err != nil {
		t.Fatal(err)
	}
	if string(seen) != dir {
		t.Errorf("%s = %q, want %q", storeEnv, seen, dir)
	}
}
