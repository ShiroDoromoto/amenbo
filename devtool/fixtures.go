package main

// Fixtures: a fake outside world for GUI verification.
//
// What amenbo reads over the network — the published latest.json — already has an env var that
// points it somewhere else. What was missing is the other half: a host that answers that URL, and a
// way to start the dev GUI pointing at it. Doing that by hand is a fake server plus an export plus a
// launch, and it was rebuilt from scratch every time the fake world had to change.
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

// realLatestJSON is where the real world lives — the source `fixtures refresh` copies from. It is the
// same constant the Rust side falls back to when the env var is unset (update_check.rs); a copy taken
// from anywhere else is not a copy of production.
const realLatestJSON = "https://github.com/ShiroDoromoto/amenbo/releases/latest/download/latest.json"

// fixturesSubdir is where the fixture tree lives, under the repo so a capture is reviewable as a diff:
//
//	devtool/fixtures/update/latest.json  the update check's answer
const fixturesSubdir = "devtool/fixtures"

// hangFor is how long a request in `timeout` mode is held before it is let go. Longer than any client
// timeout in the tree, so what the app sees is a request that never answers rather than a slow one
// that does. A field, not a constant, so a test can prove the mode without waiting out a real one.
const hangFor = 30 * time.Second

// face is one face of the outside world, which is the unit a failure is injected at: one env var,
// one client, one screen that goes wrong.
type face string

const faceUpdate face = "update"

var faces = []face{faceUpdate}

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
	fs.Parse(args)

	dir := mustFixturesDir()
	latest, err := readSource(realLatestJSON)
	if err != nil {
		// Not fatal: a release that has not published this asset yet is a real state of the world,
		// not a broken capture.
		logf("! update/latest.json not captured: %v", err)
	} else if err := writeFixture(filepath.Join(dir, "update", "latest.json"), latest); err != nil {
		logf("devtool: %v", err)
		os.Exit(1)
	} else {
		logf("→ update/latest.json (%d bytes)", len(latest))
	}
	logf("→ fixtures in %s", dir)
}

func readSource(src string) ([]byte, error) {
	if !strings.HasPrefix(src, "http://") && !strings.HasPrefix(src, "https://") {
		return os.ReadFile(src)
	}
	req, err := http.NewRequest(http.MethodGet, src, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Accept", "application/json")
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
	app := fs.String("app", "", "the dev GUI to launch — its bundle or the executable inside it (default: the installed dev app)")
	noLaunch := fs.Bool("no-launch", false, "serve and print the environment, but launch nothing")
	fresh := fs.Bool("fresh", false,
		"run against a throwaway store, so every cache starts cold and the fake world is actually asked")
	var fails repeated
	fs.Var(&fails, "fail", "make a face fail: <update|all>=<status|timeout> (repeatable)")
	fs.Parse(args)
	// Once, here: everything downstream reads this as the executable — the launch, the CLI beside it,
	// and the app-data the bundle three levels up names.
	*app = insideBundle(*app)

	rules, err := parseFailures(fails)
	if err != nil {
		logf("devtool: %v", err)
		os.Exit(2)
	}
	dir := mustFixturesDir()
	if _, err := os.Stat(filepath.Join(dir, "update", "latest.json")); err != nil {
		logf("! no update fixture in %s — run 'devtool fixtures refresh' first", dir)
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
	// The update check is answered from disk for a while, so against the dev store's caches the fake
	// world is often never asked, and a failure injected into it never bites. A throwaway AMENBO_HOME
	// is the whole user layer, caches included, so every run starts cold. The cost is that the store
	// is empty too: this is for looking at the update banner, not at tasks.
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
	cmd.Env = append(environ(), env...)
	cmd.Stdout, cmd.Stderr = os.Stderr, os.Stderr // stdout stays reserved for eval-able output
	if err := cmd.Run(); err != nil {
		logf("devtool: the dev GUI exited: %v", err)
		os.Exit(1)
	}
}

// fixtureEnv is the override that points amenbo at the fake world, in the form a shell would take
// it. The name is the app's own (crates/amenbo-core/src/env.rs) — nothing here is a
// development-only branch in the product.
func fixtureEnv(base string) []string {
	return []string{"AMENBO_UPDATE_JSON_URL=" + base + "/update/latest.json"}
}

// fixtureHandler answers the update face out of the fixture tree, or fails the way it was told to.
//
// A path with no fixture behind it is a 404, which is the truthful answer: a release that has
// published no manifest is exactly what the real address 404s, so the absence of a file and the
// absence of a release read the same to the app.
func fixtureHandler(dir string, rules map[face]failure, hold time.Duration) http.Handler {
	mux := http.NewServeMux()

	// failed answers a face the way it was told to fail, and reports whether it did — the half both
	// kinds of document share, a captured one and an invented one alike.
	failed := func(f face, w http.ResponseWriter, r *http.Request) bool {
		rule, ok := rules[f]
		if !ok {
			return false
		}
		logf("  ← %s → %s", r.URL.Path, rule)
		applyFailure(w, r, rule, hold)
		return true
	}

	serve := func(f face, path, contentType string) http.HandlerFunc {
		return func(w http.ResponseWriter, r *http.Request) {
			if failed(f, w, r) {
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

	mux.HandleFunc("GET /update/latest.json", serve(faceUpdate, filepath.Join(dir, "update", "latest.json"), "application/json"))

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
			return nil, fmt.Errorf("--fail: unknown face %q (update, all)", name)
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
	built := filepath.Join(root, "app", "src-tauri", "target", "release", devGUIBinaryGlob)
	candidates := []string{built}
	switch runtime.GOOS {
	case "darwin":
		candidates = nil
		for _, name := range devGUIBundleNames(root) {
			candidates = append(candidates,
				filepath.Join(macAppsDir, name+".app", "Contents", "MacOS", devGUIBinaryGlob),
				filepath.Join(root, "app", "src-tauri", "target", "release", "bundle", "macos",
					name+".app", "Contents", "MacOS", devGUIBinaryGlob))
		}
	case "windows":
		candidates = []string{built + ".exe"}
	}
	for _, c := range candidates {
		// A pattern, because a dev bundle names its executable after the instance it is
		// (`amenbo-app-dev`, `amenbo-app-dev-<id>`) so that a click can be aimed at one app. The CLI
		// that ships beside it is plain `amenbo`, so nothing else in there answers to `amenbo-app*`.
		found, err := filepath.Glob(c)
		if err != nil || len(found) == 0 {
			continue
		}
		return found[0], nil
	}
	return "", fmt.Errorf("no dev GUI found (%s) — build it with '%s', or pass --app",
		strings.Join(candidates, ", "), devGUIBuildCommand(root))
}

// insideBundle turns a `--app` that names a macOS bundle into the executable inside it. A bundle is
// what a person has: it is what sits in `/Applications`, what a click reaches, and what the build
// prints when it lands. It is also a directory, so passing one reaches `exec` as a permission error
// and nothing about the answer says which of the two paths was wanted.
//
// The other half is quieter and worse. The CLI that registers the fake world's own catalog is looked
// for beside the executable — inside the bundle, where it ships — so a bundle path looks beside
// `/Applications`, finds nothing, and the run carries on with the catalog unregistered: a market
// screen missing the very rows it was opened for, and nothing on it saying so.
//
// Anything that is not a bundle is handed back untouched: a path into `target/release` is already the
// executable, and one that names nothing at all keeps its own error rather than this function's.
func insideBundle(app string) string {
	if !strings.HasSuffix(app, ".app") {
		return app
	}
	if info, err := os.Stat(app); err != nil || !info.IsDir() {
		return app
	}
	found, err := filepath.Glob(filepath.Join(app, "Contents", "MacOS", devGUIBinaryGlob))
	if err != nil || len(found) == 0 {
		return app
	}
	return found[0]
}

// ---- shared ----

// mustFixturesDir is THIS checkout's fixture tree — the worktree the command runs in, not the main
// one. Fixtures are tracked files a task edits like any other, and the dev GUI a task verifies is the
// one it built here, so both belong to the checkout in hand (the devgui commands anchor to the main
// root because a per-machine instance is what they manage; this is the other case).
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
