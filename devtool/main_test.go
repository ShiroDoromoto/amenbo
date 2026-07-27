package main

import (
	"flag"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestCanonicalID pins that canonicalID accepts only the conversational number
// (optionally '#'-prefixed) and rejects ULIDs and id-prefixes: forcing the number is what
// makes one task name one checkout and one dev GUI instance, so a reference in another form
// cannot address a second app-data directory the sweep would then read as a stranger's.
// Leading zeros are kept verbatim — the rule is digits only, and amenbo resolves the number
// either way — and the ULID among the rejects is the form a session is most likely to paste.
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

// TestParseAroundID pins that the id survives whichever side of the flags it is written
// on — a leading flag must not swallow it, since reporting a missing id for an
// invocation that carried one is the failure mode this guards.
func TestParseAroundID(t *testing.T) {
	cases := []struct {
		name    string
		args    []string
		id      string
		noBuild bool
	}{
		{"bare id", []string{"2053"}, "2053", false},
		{"flags after the id", []string{"2053", "-no-build"}, "2053", true},
		{"flags before the id", []string{"-no-build", "2053"}, "2053", true},
		{"flags only", []string{"-no-build"}, "", true},
	}
	for _, c := range cases {
		fs := flag.NewFlagSet("devgui cli", flag.ContinueOnError)
		noBuild := fs.Bool("no-build", false, "")
		id, extra := parseAroundID(fs, c.args)
		if id != c.id {
			t.Fatalf("%s: id = %q, want %q", c.name, id, c.id)
		}
		if len(extra) != 0 {
			t.Fatalf("%s: unexpected extra positionals %v", c.name, extra)
		}
		if *noBuild != c.noBuild {
			t.Fatalf("%s: -no-build = %v, want %v", c.name, *noBuild, c.noBuild)
		}
	}

	// A second id is a typo, not a batch: an instance's bundle and app-data are named after one.
	fs := flag.NewFlagSet("devgui cli", flag.ContinueOnError)
	fs.Bool("no-build", false, "")
	if id, extra := parseAroundID(fs, []string{"2053", "-no-build", "2054"}); id != "2053" || len(extra) != 1 || extra[0] != "2054" {
		t.Fatalf("a second id is handed back for refusal: id=%q extra=%v", id, extra)
	}
}

// setupRepo creates a temp git repo, chdir's into it for the duration of the test, and returns the
// repo root. paths() derives the sibling worktree dir from os.Getwd, so a test that resolves one
// has to run from inside a repository.
func setupRepo(t *testing.T) string {
	t.Helper()
	root := filepath.Join(t.TempDir(), "repo")
	if err := os.MkdirAll(root, 0o755); err != nil {
		t.Fatal(err)
	}
	if _, err := run(root, "git", "init", "-b", "main"); err != nil {
		t.Fatalf("git init: %v", err)
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

// TestDevGUISeedRefusesWithoutCheckout pins that seeding an instance for a number nothing is
// checked out under is refused, rather than quietly cloning a store for a task that does not
// exist here: the app-data would then sit on the machine under a number no worktree claims,
// which is exactly what the sweep has to go and find.
func TestDevGUISeedRefusesWithoutCheckout(t *testing.T) {
	setupRepo(t)
	err := devGUISeed("1")
	if err == nil {
		t.Fatal("seeding without a checkout should be refused")
	}
	if !strings.Contains(err.Error(), "no checkout for task 1") {
		t.Fatalf("the refusal should name the missing checkout: %v", err)
	}
}
