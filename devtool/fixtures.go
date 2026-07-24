package main

// Fixtures: a fake outside world for GUI verification.
//
// Three things amenbo reads over the network — the plugin catalog, GitHub's API for one opened
// plugin, and the published latest.json — each already have an env var that points them somewhere
// else. What was missing is the other half: a host that answers those URLs, and a way to start the
// dev GUI pointing at it. Doing that by hand is a fake server plus three exports plus a launch, and
// it was rebuilt from scratch every time the fake world had to change.
//
// Two properties are the whole point, and they are why this is not "some JSON in a directory":
//
//   - The fixtures are COPIES, taken from the real world by `fixtures refresh`. A hand-written
//     fixture drifts from what the producer actually sends and the mismatch shows up as a green
//     check over a broken screen — an aggregation that quietly stopped copying two fields is the
//     kind of thing only a real capture catches.
//   - The fake world can FAIL ON PURPOSE. 429, 500, 404, a request that never answers: these are
//     the responses the real API will not produce on demand, and the branches that handle them are
//     exactly the ones that never get exercised against the real one.
//
// It replaces no test that talks to the real world: the fake answers what it was told to answer, so
// it can only confirm what we already believe. The `#[ignore]`d tests against the real API stay.

import (
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"syscall"
	"time"
)

// Where the real world lives — the sources `fixtures refresh` copies from. They are the same
// constants the Rust side falls back to when the env var is unset (plugin_catalog.rs,
// plugin_github.rs, update_check.rs); a copy taken from anywhere else is not a copy of production.
const (
	realCatalogURL   = "https://shirodoromoto.github.io/amenbo-plugins/catalog.json"
	realGitHubAPIURL = "https://api.github.com"
	realLatestJSON   = "https://github.com/ShiroDoromoto/amenbo/releases/latest/download/latest.json"
)

// fixturesSubdir is where the fixture tree lives, under the repo so a capture is reviewable as a diff:
//
//	devtool/fixtures/catalog.json                        the catalog envelope
//	devtool/fixtures/update/latest.json                  the update check's answer
//	devtool/fixtures/github/repos/<owner>__<name>.json     /repos/{repo}
//	devtool/fixtures/github/releases/<owner>__<name>.json  /repos/{repo}/releases/latest
//	devtool/fixtures/github/readme/<owner>__<name>.md      /repos/{repo}/readme
const fixturesSubdir = "devtool/fixtures"

// hangFor is how long a request in `timeout` mode is held before it is let go. Longer than any client timeout
// in the tree (the catalog's 10s is the longest), so what the app sees is a request that never
// answers rather than a slow one that does. A field, not a constant, so a test can prove the mode
// without waiting out a real one.
const hangFor = 30 * time.Second

// face is one face of the outside world, which is the unit a failure is injected at: one env var,
// one client, one screen that goes wrong.
type face string

const (
	faceCatalog face = "catalog"
	faceGitHub  face = "github"
	faceUpdate  face = "update"
)

var faces = []face{faceCatalog, faceGitHub, faceUpdate}

// failure is how a face is made to fail. The zero value answers normally.
type failure struct {
	// status is answered instead of the fixture.
	status int
	// hang holds the request open instead of answering it at all.
	hang bool
}

// fixturesCmd dispatches `devtool fixtures …`.
func fixturesCmd(args []string) {
	if len(args) == 0 {
		usage()
		os.Exit(2)
	}
	switch args[0] {
	case "refresh":
		fixturesRefresh(args[1:])
	case "gui":
		fixturesGUI(args[1:])
	default:
		logf("devtool: unknown command %q", "fixtures "+args[0])
		usage()
		os.Exit(2)
	}
}

// ---- refresh: take the fixtures from the real world ----

func fixturesRefresh(args []string) {
	fs := flag.NewFlagSet("fixtures refresh", flag.ExitOnError)
	catalogSrc := fs.String("catalog", realCatalogURL,
		"where to take the catalog from — a URL or a path, for a catalog repo's own generated copy")
	var repos repeated
	fs.Var(&repos, "repo", "an extra owner/name to capture, beyond the ones the catalog names (repeatable)")
	fs.Parse(args)

	dir := mustFixturesDir()
	catalog, err := readSource(*catalogSrc)
	if err != nil {
		logf("devtool: %v", err)
		os.Exit(1)
	}
	if err := writeFixture(filepath.Join(dir, "catalog.json"), catalog); err != nil {
		logf("devtool: %v", err)
		os.Exit(1)
	}
	logf("→ catalog.json (%d bytes, from %s)", len(catalog), *catalogSrc)

	latest, err := readSource(realLatestJSON)
	if err != nil {
		// Not fatal: the update banner is one of three faces, and a release that has not published
		// this asset yet is a real state of the world, not a broken capture.
		logf("! update/latest.json not captured: %v", err)
	} else if err := writeFixture(filepath.Join(dir, "update", "latest.json"), latest); err != nil {
		logf("devtool: %v", err)
		os.Exit(1)
	} else {
		logf("→ update/latest.json (%d bytes)", len(latest))
	}

	// The repositories the catalog itself names, so the capture follows the catalog rather than a
	// list kept by hand beside it.
	for _, repo := range dedupe(append(catalogRepos(catalog), repos...)) {
		refreshRepo(dir, repo)
	}
	logf("→ fixtures in %s", dir)
}

// refreshRepo captures the three answers a plugin's detail reads for one repository. Each is
// best-effort on its own, exactly as the app treats them: a repository with no release or no README
// is a repository with no release or no README, and the absent file makes the fake say so too.
func refreshRepo(dir, repo string) {
	flat := strings.ReplaceAll(repo, "/", "__")
	for _, want := range []struct {
		url  string
		path string
	}{
		{realGitHubAPIURL + "/repos/" + repo, filepath.Join(dir, "github", "repos", flat+".json")},
		{realGitHubAPIURL + "/repos/" + repo + "/releases/latest", filepath.Join(dir, "github", "releases", flat+".json")},
		{realGitHubAPIURL + "/repos/" + repo + "/readme", filepath.Join(dir, "github", "readme", flat+".md")},
	} {
		body, err := readSource(want.url)
		if err != nil {
			logf("! %s: %v", want.url, err)
			continue
		}
		if err := writeFixture(want.path, body); err != nil {
			logf("devtool: %v", err)
			os.Exit(1)
		}
		logf("→ %s (%d bytes)", strings.TrimPrefix(want.path, dir+string(filepath.Separator)), len(body))
	}
}

// catalogRepos picks the `repo` of every entry out of a catalog envelope. It reads the JSON loosely
// on purpose: the envelope is the producer's to grow, and a capture that refuses to run because a
// field it does not use appeared would be a capture nobody takes.
func catalogRepos(catalog []byte) []string {
	var envelope struct {
		Plugins []struct {
			Repo string `json:"repo"`
		} `json:"plugins"`
	}
	if err := json.Unmarshal(catalog, &envelope); err != nil {
		logf("! could not read the catalog's entries (%v) — capturing no repository from it", err)
		return nil
	}
	out := make([]string, 0, len(envelope.Plugins))
	for _, p := range envelope.Plugins {
		if p.Repo != "" {
			out = append(out, p.Repo)
		}
	}
	return out
}

// readSource reads a URL or a local path, so a catalog can be taken from a checkout of the catalog
// repository (where its CI generated it) as readily as from the published copy.
func readSource(src string) ([]byte, error) {
	if !strings.HasPrefix(src, "http://") && !strings.HasPrefix(src, "https://") {
		return os.ReadFile(src)
	}
	req, err := http.NewRequest(http.MethodGet, src, nil)
	if err != nil {
		return nil, err
	}
	// The README endpoint answers with a JSON envelope unless the raw media type is asked for, and
	// what the app reads is the Markdown itself (plugin_github.rs).
	if strings.HasSuffix(src, "/readme") {
		req.Header.Set("Accept", "application/vnd.github.raw")
	} else {
		req.Header.Set("Accept", "application/vnd.github+json")
	}
	req.Header.Set("User-Agent", "amenbo-devtool")
	resp, err := (&http.Client{Timeout: 30 * time.Second}).Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("%s: %s", src, resp.Status)
	}
	return io.ReadAll(resp.Body)
}

func writeFixture(path string, body []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	return os.WriteFile(path, body, 0o644)
}

// ---- gui: serve the fixtures, and start the dev GUI against them ----

func fixturesGUI(args []string) {
	fs := flag.NewFlagSet("fixtures gui", flag.ExitOnError)
	port := fs.Int("port", 0, "port to serve the fake world on (0 = pick a free one)")
	app := fs.String("app", "", "the dev GUI binary to launch (default: the installed dev app)")
	noLaunch := fs.Bool("no-launch", false, "serve and print the environment, but launch nothing")
	fresh := fs.Bool("fresh", false,
		"run against a throwaway store, so every cache starts cold and the fake world is actually asked")
	var fails repeated
	fs.Var(&fails, "fail", "make a face fail: <catalog|github|update|all>=<status|timeout> (repeatable)")
	fs.Parse(args)

	rules, err := parseFailures(fails)
	if err != nil {
		logf("devtool: %v", err)
		os.Exit(2)
	}
	dir := mustFixturesDir()
	if _, err := os.Stat(filepath.Join(dir, "catalog.json")); err != nil {
		logf("! no catalog fixture in %s — run 'devtool fixtures refresh' first", dir)
	}

	listener, err := net.Listen("tcp", fmt.Sprintf("127.0.0.1:%d", *port))
	if err != nil {
		logf("devtool: %v", err)
		os.Exit(1)
	}
	base := "http://" + listener.Addr().String()
	server := &http.Server{Handler: fixtureHandler(dir, rules, hangFor)}
	go func() {
		if err := server.Serve(listener); err != nil && !errors.Is(err, http.ErrServerClosed) {
			logf("devtool: fake world stopped: %v", err)
		}
	}()
	defer server.Close()

	env := fixtureEnv(base)
	// A catalog fetch is answered from disk for an hour, and a repository's figures for six — so against the dev store's caches the fake world is usually never asked, and a
	// failure injected into it never bites. A throwaway AMENBO_HOME is the whole user layer, caches
	// included, so every run starts cold. The cost is that the store is empty too: this is for
	// looking at the market, the detail and the update banner, not at tasks.
	if *fresh {
		home, err := os.MkdirTemp("", "amenbo-fixtures-")
		if err != nil {
			logf("devtool: %v", err)
			os.Exit(1)
		}
		defer os.RemoveAll(home)
		env = append(env, "AMENBO_HOME="+home)
	}
	for _, e := range env {
		logf("  %s", e)
	}
	for f, r := range rules {
		logf("  ! %s answers %s", f, r)
	}

	if *noLaunch {
		logf("→ fake world on %s (Ctrl-C to stop)", base)
		waitForSignal()
		return
	}

	bin := *app
	if bin == "" {
		if bin, err = devAppBinary(); err != nil {
			logf("devtool: %v", err)
			os.Exit(1)
		}
	}
	logf("→ launching %s against %s", bin, base)
	cmd := exec.Command(bin)
	cmd.Env = append(os.Environ(), env...)
	cmd.Stdout, cmd.Stderr = os.Stderr, os.Stderr // stdout stays reserved for eval-able output
	if err := cmd.Run(); err != nil {
		logf("devtool: the dev GUI exited: %v", err)
		os.Exit(1)
	}
}

// fixtureEnv is the three overrides that point amenbo at the fake world, in the form a shell would
// take them. The names are the app's own (crates/amenbo-core/src/env.rs) — nothing here is a
// development-only branch in the product.
func fixtureEnv(base string) []string {
	return []string{
		"AMENBO_PLUGIN_CATALOG_URL=" + base + "/catalog.json",
		"AMENBO_GITHUB_API_URL=" + base + "/github",
		"AMENBO_UPDATE_JSON_URL=" + base + "/update/latest.json",
	}
}

// fixtureHandler answers the three faces out of the fixture tree, or fails the way it was told to.
//
// A path with no fixture behind it is a 404, which is the truthful answer: a repository with no
// release is exactly what GitHub 404s, so the absence of a file and the absence of a release read
// the same to the app.
func fixtureHandler(dir string, rules map[face]failure, hold time.Duration) http.Handler {
	mux := http.NewServeMux()

	serve := func(f face, path, contentType string) http.HandlerFunc {
		return func(w http.ResponseWriter, r *http.Request) {
			if rule, ok := rules[f]; ok {
				logf("  ← %s → %s", r.URL.Path, rule)
				applyFailure(w, r, rule, hold)
				return
			}
			body, err := os.ReadFile(path)
			if err != nil {
				// Saying which request went unanswered is the difference between "the fixture is
				// missing" and "the app never asked" — and a cache inside its freshness window means
				// it often did not ask (--fresh is the way to make it).
				logf("  ← %s → 404 (no %s)", r.URL.Path, filepath.Base(path))
				http.Error(w, "no fixture: "+filepath.Base(path), http.StatusNotFound)
				return
			}
			logf("  ← %s → %d bytes", r.URL.Path, len(body))
			w.Header().Set("Content-Type", contentType)
			w.Write(body)
		}
	}

	mux.HandleFunc("GET /catalog.json", serve(faceCatalog, filepath.Join(dir, "catalog.json"), "application/json"))
	mux.HandleFunc("GET /update/latest.json", serve(faceUpdate, filepath.Join(dir, "update", "latest.json"), "application/json"))

	// The three GitHub reads one opened plugin makes. `{owner}/{name}` is the catalog's `repo`, and
	// it is flattened to one file name the same way the app's own cache does.
	github := func(kind, ext, contentType string) http.HandlerFunc {
		return func(w http.ResponseWriter, r *http.Request) {
			flat := r.PathValue("owner") + "__" + r.PathValue("name")
			serve(faceGitHub, filepath.Join(dir, "github", kind, flat+ext), contentType)(w, r)
		}
	}
	mux.HandleFunc("GET /github/repos/{owner}/{name}", github("repos", ".json", "application/json"))
	mux.HandleFunc("GET /github/repos/{owner}/{name}/releases/latest", github("releases", ".json", "application/json"))
	mux.HandleFunc("GET /github/repos/{owner}/{name}/readme", github("readme", ".md", "text/plain; charset=utf-8"))

	return mux
}

// applyFailure is the half of this that the real world cannot be asked for: a rate limit on demand,
// a server error on demand, a request that simply never comes back.
func applyFailure(w http.ResponseWriter, r *http.Request, rule failure, hold time.Duration) {
	if rule.hang {
		select {
		case <-time.After(hold):
		case <-r.Context().Done(): // the client gave up first, which is the point
		}
		return
	}
	http.Error(w, http.StatusText(rule.status), rule.status)
}

// parseFailures reads the `--fail <face>=<mode>` specs. `all` names every face at once, so "the
// network is down" is one flag rather than three.
func parseFailures(specs []string) (map[face]failure, error) {
	rules := map[face]failure{}
	for _, spec := range specs {
		name, mode, ok := strings.Cut(spec, "=")
		if !ok {
			return nil, fmt.Errorf("--fail wants <face>=<mode>, got %q", spec)
		}
		targets := []face{face(name)}
		if name == "all" {
			targets = faces
		} else if !validFace(face(name)) {
			return nil, fmt.Errorf("--fail: unknown face %q (catalog, github, update, all)", name)
		}
		rule, err := parseFailMode(mode)
		if err != nil {
			return nil, err
		}
		for _, t := range targets {
			rules[t] = rule
		}
	}
	return rules, nil
}

func parseFailMode(mode string) (failure, error) {
	if mode == "timeout" {
		return failure{hang: true}, nil
	}
	status, err := strconv.Atoi(mode)
	if err != nil || status < 100 || status > 599 {
		return failure{}, fmt.Errorf("--fail: unknown mode %q (an HTTP status, or timeout)", mode)
	}
	return failure{status: status}, nil
}

func validFace(f face) bool {
	for _, known := range faces {
		if f == known {
			return true
		}
	}
	return false
}

func (f failure) String() string {
	if f.hang {
		return "nothing (the request hangs)"
	}
	return strconv.Itoa(f.status)
}

// devAppBinary finds the dev GUI to launch. On macOS that is the installed bundle a click actually
// reaches, taken in the order devGUIBundleNames gives — this checkout's own instance ahead of the
// shared dev app — and elsewhere the binary the dev build leaves in the tree. `--app` overrides it,
// which is also the answer for a bundle installed somewhere else. The launch names the binary it
// picked, so which of the two it landed on is never a guess.
func devAppBinary() (string, error) {
	root := mustTreeRoot()
	built := filepath.Join(root, "app", "src-tauri", "target", "release", "amenbo-app")
	candidates := []string{built}
	switch runtime.GOOS {
	case "darwin":
		candidates = nil
		for _, name := range devGUIBundleNames(root) {
			candidates = append(candidates,
				filepath.Join(macAppsDir, name+".app", "Contents", "MacOS", "amenbo-app"),
				filepath.Join(root, "app", "src-tauri", "target", "release", "bundle", "macos",
					name+".app", "Contents", "MacOS", "amenbo-app"))
		}
	case "windows":
		candidates = []string{built + ".exe"}
	}
	for _, c := range candidates {
		if _, err := os.Stat(c); err == nil {
			return c, nil
		}
	}
	return "", fmt.Errorf("no dev GUI found (%s) — build it with '%s', or pass --app",
		strings.Join(candidates, ", "), devGUIBuildCommand(root))
}

// ---- shared ----

// mustFixturesDir is THIS checkout's fixture tree — the worktree the command runs in, not the main
// one. Fixtures are tracked files a task edits like any other, and the dev GUI a task verifies is the
// one it built here, so both belong to the checkout in hand (`task start` / `finish` anchor to the
// main root because a worktree is what they manage; this is the other case).
func mustFixturesDir() string {
	return filepath.Join(mustTreeRoot(), fixturesSubdir)
}

func mustTreeRoot() string {
	cwd, err := os.Getwd()
	if err == nil {
		var root string
		if root, err = run(cwd, "git", "rev-parse", "--show-toplevel"); err == nil {
			return root
		}
	}
	logf("devtool: %v", err)
	os.Exit(1)
	return ""
}

func waitForSignal() {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	<-ctx.Done()
}

// repeated is a flag that may be given more than once, collecting its values in order.
type repeated []string

func (r *repeated) String() string { return strings.Join(*r, ",") }

func (r *repeated) Set(v string) error {
	*r = append(*r, v)
	return nil
}

func dedupe(in []string) []string {
	seen := map[string]bool{}
	out := in[:0]
	for _, v := range in {
		if !seen[v] {
			seen[v] = true
			out = append(out, v)
		}
	}
	return out
}
