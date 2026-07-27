package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// sizes maps a top-level key of the agent JSON to the byte length of its value.
type sizes map[string]int

// measurement is what gets cached per base commit.
type measurement struct {
	Sections sizes `json:"sections"`
	Total    int   `json:"total"`
}

// sizesFromJSON splits one agent --json document into per-section byte counts.
// The total is the whole document, so it also carries the keys and punctuation
// the sections themselves do not — section sums never quite reach it, by design.
func sizesFromJSON(doc []byte) (measurement, error) {
	var top map[string]json.RawMessage
	if err := json.Unmarshal(doc, &top); err != nil {
		return measurement{}, fmt.Errorf("agent --json did not parse as an object: %v", err)
	}
	s := make(sizes, len(top))
	for k, v := range top {
		s[k] = len(v)
	}
	return measurement{Sections: s, Total: len(doc)}, nil
}

// measure builds the CLI in tree and runs `agent --json` against a throwaway store,
// going through `make verify` because verify is the one contract that pins both halves
// of the isolation (a throwaway AMENBO_HOME and a CWD with no .amenbo ancestor) —
// reaching past it would risk measuring against the real store, whose size is not the
// subject. -s is required rather than tidy: verify's build line is not @-silenced, so
// make would staple a `cargo build …` line onto the front of the JSON. It declares no
// facet, because the document uses none: it is the spec of the tool rather than a view
// of anyone's data, so it is byte-identical whoever asks. That is also why verify's CWD
// is left unbound — binding it would hand the measurement a store to read, and the
// document would not change for it.
func measure(tree string) (measurement, error) {
	out, err := run(tree, "make", "-s", "verify", "ARGS=agent --json")
	if err != nil {
		return measurement{}, err
	}
	return sizesFromJSON([]byte(out))
}

// rigDir is the persistent base-measuring worktree. It lives under the user cache dir,
// not under the repo's sibling worktrees dir: that dir is the per-task space `task
// finish` prunes, and a permanent resident there would keep it from ever emptying.
func rigDir() (string, error) {
	cache, err := os.UserCacheDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(cache, "devtool", "agent-size", "rig"), nil
}

// ensureRig parks the rig on sha, creating it if absent. A rig that git no longer
// accepts (removed by hand, pruned, left mid-operation) is rebuilt rather than
// diagnosed — it holds no work of its own, so there is nothing to lose by replacing it.
// The add is --force because the rig lives in a cache dir, which the OS and its owner
// delete: git's registration outlives the directory, and a plain `add` onto a path that
// is missing yet still registered is refused, which would break the signal for good the
// first time the cache was swept. It is safe to lean on for the same reason the rig is
// detached — there is no branch to fight over.
func ensureRig(root, sha string) (string, error) {
	rig, err := rigDir()
	if err != nil {
		return "", err
	}
	if _, statErr := os.Stat(rig); statErr == nil {
		if _, err := run(rig, "git", "checkout", "--detach", sha); err == nil {
			return rig, nil
		}
		logf("devtool: the agent-size rig would not check out %s — rebuilding it", short(sha))
		_, _ = run(root, "git", "worktree", "remove", "--force", rig)
		_ = os.RemoveAll(rig)
	}
	if err := os.MkdirAll(filepath.Dir(rig), 0o755); err != nil {
		return "", err
	}
	if _, err := run(root, "git", "worktree", "add", "--detach", "--force", rig, sha); err != nil {
		return "", fmt.Errorf("could not create the agent-size rig: %v", err)
	}
	return rig, nil
}

// baseMeasurement returns the sizes at sha, building the rig only on a cache miss.
// A commit's entry size never changes, so the cache needs no invalidation.
func baseMeasurement(root, sha string) (measurement, error) {
	if cache, err := os.UserCacheDir(); err == nil {
		if raw, readErr := os.ReadFile(filepath.Join(cache, "devtool", "agent-size", sha+".json")); readErr == nil {
			var m measurement
			if json.Unmarshal(raw, &m) == nil {
				return m, nil
			}
		}
	}
	rig, err := ensureRig(root, sha)
	if err != nil {
		return measurement{}, err
	}
	logf("devtool: measuring the base at %s (rig: %s)", short(sha), rig)
	m, err := measure(rig)
	if err != nil {
		return measurement{}, err
	}
	if cache, cErr := os.UserCacheDir(); cErr == nil {
		if raw, mErr := json.Marshal(m); mErr == nil {
			_ = os.WriteFile(filepath.Join(cache, "devtool", "agent-size", sha+".json"), raw, 0o644)
		}
	}
	return m, nil
}

// row is one section's before/after.
type row struct {
	Section string
	Base    int
	Head    int
}

func (r row) delta() int { return r.Head - r.Base }

// deltaRows pairs the two sides, keeping sections that exist on either one (a section
// added or dropped by the diff is exactly what the reader wants to see). Biggest growth
// first: the ordering answers "what did this diff put in the entry" without scanning.
func deltaRows(base, head measurement) []row {
	seen := map[string]bool{}
	for k := range base.Sections {
		seen[k] = true
	}
	for k := range head.Sections {
		seen[k] = true
	}
	rows := make([]row, 0, len(seen))
	for k := range seen {
		rows = append(rows, row{Section: k, Base: base.Sections[k], Head: head.Sections[k]})
	}
	sort.Slice(rows, func(i, j int) bool {
		if rows[i].delta() != rows[j].delta() {
			return rows[i].delta() > rows[j].delta()
		}
		return rows[i].Section < rows[j].Section
	})
	return rows
}

// render lays the delta out for a human. Sections that did not move are kept: their
// silence is part of the picture (it locates the growth).
func render(base, head measurement, baseRef, sha string) string {
	var b strings.Builder
	fmt.Fprintf(&b, "agent --json entry size — this tree vs merge-base with %s (%s)\n\n", baseRef, short(sha))
	fmt.Fprintf(&b, "  %-18s %9s %9s %9s\n", "section", "base", "head", "delta")
	for _, r := range deltaRows(base, head) {
		fmt.Fprintf(&b, "  %-18s %9s %9s %9s\n", r.Section, comma(r.Base), comma(r.Head), signed(r.delta()))
	}
	fmt.Fprintf(&b, "  %-18s %9s %9s %9s\n", "TOTAL", comma(base.Total), comma(head.Total), signed(head.Total-base.Total))
	switch d := head.Total - base.Total; {
	case d > 0:
		fmt.Fprintf(&b, "\n  +%s bytes. Every session reads this. Is what you added a spec, or an argument?\n", comma(d))
		b.WriteString("  An argument belongs in a decision record; a spec detail belongs in the --command layer.\n")
	case d < 0:
		fmt.Fprintf(&b, "\n  %s bytes.\n", comma(d))
	default:
		b.WriteString("\n  Unchanged.\n")
	}
	return b.String()
}

func short(sha string) string {
	if len(sha) > 12 {
		return sha[:12]
	}
	return sha
}

func signed(n int) string {
	if n > 0 {
		return "+" + comma(n)
	}
	return comma(n)
}

// comma groups digits so five-figure byte counts can be compared at a glance.
func comma(n int) string {
	s := fmt.Sprint(n)
	neg := strings.HasPrefix(s, "-")
	s = strings.TrimPrefix(s, "-")
	var out []byte
	for i, c := range []byte(s) {
		if i > 0 && (len(s)-i)%3 == 0 {
			out = append(out, ',')
		}
		out = append(out, c)
	}
	if neg {
		return "-" + string(out)
	}
	return string(out)
}

// agentCmd prints what this tree does to the size of the `amenbo agent --json` entry,
// section by section, and always exits 0 — the entry is read once per AI session, so
// what lands there is paid forever, and the delta puts that to the author rather than
// answering it. head is the tree as it stands, uncommitted changes included; base is
// the merge-base with --base, so what others landed on main is not billed to this diff.
// The base is measured in a persistent rig and cached by commit SHA, because a
// throwaway worktree has no target/ and would pay a cold cargo build every run.
func agentCmd(args []string) {
	if len(args) == 0 || args[0] != "size" {
		logf("devtool: unknown agent subcommand (expected `agent size`)")
		usage()
		os.Exit(2)
	}
	fs := flag.NewFlagSet("agent size", flag.ExitOnError)
	baseRef := fs.String("base", "main", "branch to compare against (the merge-base with it is the base)")
	asJSON := fs.Bool("json", false, "machine-readable output")
	fs.Parse(args[1:])

	cwd, err := os.Getwd()
	if err != nil {
		logf("devtool: %v", err)
		os.Exit(1)
	}
	tree, err := run(cwd, "git", "rev-parse", "--show-toplevel")
	if err != nil {
		logf("devtool: %v", err)
		os.Exit(1)
	}
	root, err := gitRoot(cwd)
	if err != nil {
		logf("devtool: %v", err)
		os.Exit(1)
	}
	sha, err := run(tree, "git", "merge-base", "HEAD", *baseRef)
	if err != nil {
		logf("devtool: no merge-base with %s: %v", *baseRef, err)
		os.Exit(1)
	}

	base, err := baseMeasurement(root, sha)
	if err != nil {
		logf("devtool: measuring the base failed: %v", err)
		os.Exit(1)
	}
	logf("devtool: measuring this tree")
	head, err := measure(tree)
	if err != nil {
		logf("devtool: measuring this tree failed: %v", err)
		os.Exit(1)
	}

	if *asJSON {
		out, err := json.MarshalIndent(map[string]any{
			"base_ref": *baseRef, "base_sha": sha,
			"base": base, "head": head,
			"delta_total": head.Total - base.Total,
		}, "", "  ")
		if err != nil {
			logf("devtool: %v", err)
			os.Exit(1)
		}
		fmt.Println(string(out))
		return
	}
	fmt.Print(render(base, head, *baseRef, sha))
}
