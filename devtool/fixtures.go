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
		"where to take the catalog from — a URL, a generated copy's path, or a catalog repo checkout to aggregate")
	amenboBin := fs.String("amenbo", "",
		"the amenbo build to validate manifests with when aggregating a checkout (default: this one)")
	var repos repeated
	fs.Var(&repos, "repo", "an extra owner/name to capture, beyond the ones the catalog names (repeatable)")
	fs.Parse(args)

	dir := mustFixturesDir()
	// A checkout of the catalog repository is aggregated rather than copied: while the published
	// catalog lists nothing there is no copy to take, and the manifests are the material either way.
	catalog, details, err := readCatalog(*catalogSrc, *amenboBin)
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

	// The catalog is served in two documents, so a capture of the list alone is a fake world where
	// nothing can be installed: each entry's detail is taken from beside the list it was named in, or
	// comes out of the aggregation that just built the list.
	entries := catalogEntries(catalog)
	for _, entry := range entries {
		if body, built := details[entry.Name]; built {
			writeDetail(dir, entry.Name, body, "built")
			continue
		}
		refreshDetail(dir, *catalogSrc, entry.Name)
	}

	// The repositories the catalog itself names, so the capture follows the catalog rather than a
	// list kept by hand beside it.
	repoList := make([]string, 0, len(entries))
	for _, entry := range entries {
		if entry.Repo != "" {
			repoList = append(repoList, entry.Repo)
		}
	}
	for _, repo := range dedupe(append(repoList, repos...)) {
		refreshRepo(dir, repo)
	}
	logf("→ fixtures in %s", dir)
}

// readCatalog answers with the catalog list and, when it built them, the detail documents that go with
// it. A directory is a checkout of the catalog repository and is aggregated from its manifests
// (`fixtures_catalog.go`); anything else is a URL or a generated copy, and is taken as it is.
func readCatalog(src, amenboBin string) ([]byte, map[string][]byte, error) {
	if info, err := os.Stat(src); err == nil && info.IsDir() {
		bin, err := validatorBinary(amenboBin)
		if err != nil {
			return nil, nil, err
		}
		logf("→ aggregating %s with %s", src, bin)
		return aggregateCatalog(src, bin)
	}
	catalog, err := readSource(src)
	return catalog, nil, err
}

// validatorBinary is the amenbo build that splits a manifest into the two published documents. The
// released CLI does not carry the plugin commands yet, so the default is this checkout's own build
// rather than whatever `amenbo` is on the PATH — the same principle as the dev GUI's, and the pick is
// named in the log so it is never a guess.
func validatorBinary(chosen string) (string, error) {
	if chosen != "" {
		return chosen, nil
	}
	root := mustTreeRoot()
	candidates := []string{
		filepath.Join(root, "target", "debug", "amenbo"),
		filepath.Join(root, "target", "dev", "release", "amenbo"),
		filepath.Join(root, "target", "release", "amenbo"),
	}
	if runtime.GOOS == "windows" {
		for i, c := range candidates {
			candidates[i] = c + ".exe"
		}
	}
	for _, c := range candidates {
		if _, err := os.Stat(c); err == nil {
			return c, nil
		}
	}
	return "", fmt.Errorf("no amenbo build to validate with (%s) — build one with 'cargo build -p amenbo-cli', or pass --amenbo",
		strings.Join(candidates, ", "))
}

// writeDetail puts one built detail document where a detail is fetched from — the same place a
// captured one lands, so what serves them cannot tell the two apart.
func writeDetail(dir, name string, body []byte, how string) {
	path := filepath.Join(dir, "plugins", name+".json")
	if err := writeFixture(path, body); err != nil {
		logf("devtool: %v", err)
		os.Exit(1)
	}
	logf("→ %s (%d bytes, %s)", strings.TrimPrefix(path, dir+string(filepath.Separator)), len(body), how)
}

// refreshDetail captures one plugin's detail document — what an install reads, and what a detail
// view opens. Best-effort like a repository's answers: a catalog that lists an entry whose detail is
// not published yet is a real state of the world, and the absent file makes the fake say so too.
func refreshDetail(dir, catalogSrc, name string) {
	src := detailSource(catalogSrc, name)
	body, err := readSource(src)
	if err != nil {
		logf("! %s: %v", src, err)
		return
	}
	writeDetail(dir, name, body, "captured")
}

// detailSource is where one plugin's detail sits beside the list it was named in: the same base,
// under `plugins/`. Derived rather than configured, because the two documents are published together
// — a checkout of the catalog repository holds both, and so does the published site.
func detailSource(catalogSrc, name string) string {
	rel := "plugins/" + name + ".json"
	if strings.HasPrefix(catalogSrc, "http://") || strings.HasPrefix(catalogSrc, "https://") {
		if cut := strings.LastIndex(catalogSrc, "/"); cut >= 0 {
			return catalogSrc[:cut+1] + rel
		}
		return rel
	}
	return filepath.Join(filepath.Dir(catalogSrc), "plugins", name+".json")
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

// catalogEntry is the little of a list entry a capture needs: the name its detail document is
// fetched by, and the repository its figures are read from.
type catalogEntry struct {
	Name string `json:"name"`
	Repo string `json:"repo"`
}

// catalogEntries picks the entries out of a catalog envelope. It reads the JSON loosely on purpose:
// the envelope is the producer's to grow, and a capture that refuses to run because a field it does
// not use appeared would be a capture nobody takes.
func catalogEntries(catalog []byte) []catalogEntry {
	var envelope struct {
		Plugins []catalogEntry `json:"plugins"`
	}
	if err := json.Unmarshal(catalog, &envelope); err != nil {
		logf("! could not read the catalog's entries (%v) — capturing nothing from it", err)
		return nil
	}
	return envelope.Plugins
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
	app := fs.String("app", "", "the dev GUI to launch — its bundle or the executable inside it (default: the installed dev app)")
	noLaunch := fs.Bool("no-launch", false, "serve and print the environment, but launch nothing")
	fresh := fs.Bool("fresh", false,
		"run against a throwaway store, so every cache starts cold and the fake world is actually asked")
	var fails repeated
	fs.Var(&fails, "fail", "make a face fail: <catalog|github|update|all>=<status|timeout> (repeatable)")
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

	// The second catalog reaches the app through the store, not through an env var: a registered
	// catalog **is** a record in the store, so the fake world can only offer one by registering it in
	// the store the GUI will open.
	registered := base + "/" + registeredCatalogPath
	if cli, cliEnv, err := devCLI(*app, env); err != nil {
		logf("  ! %s is not registered (%v)", registeredCatalogName, err)
		logf("    register it by hand: amenbo plugin catalog add %s --name %q --yes", registered, registeredCatalogName)
	} else {
		registerFakeCatalog(cli, cliEnv, registered)
		defer dropFakeCatalogs(cli, cliEnv, "")
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

	// serveBytes is serve for a document the fake world holds in memory rather than on disk, which is
	// what an invented one is: there is no file for it to be missing.
	serveBytes := func(f face, body []byte, contentType string) http.HandlerFunc {
		return func(w http.ResponseWriter, r *http.Request) {
			if failed(f, w, r) {
				return
			}
			logf("  ← %s → %d bytes", r.URL.Path, len(body))
			w.Header().Set("Content-Type", contentType)
			w.Write(body)
		}
	}
	// The catalog the fake world invents rather than captures, and the key it publishes beside it
	// (`fixtures_registered.go`). It answers under the catalog face, so `--fail catalog=…` takes both
	// shelves down at once — which is what "the catalog is unreachable" means to the app.
	for path, body := range registeredCatalogDocs() {
		contentType := "application/json"
		if strings.HasSuffix(path, ".pub") {
			contentType = "text/plain; charset=utf-8"
		}
		mux.HandleFunc("GET /"+path, serveBytes(faceCatalog, body, contentType))
	}

	mux.HandleFunc("GET /catalog.json", serve(faceCatalog, filepath.Join(dir, "catalog.json"), "application/json"))
	// The second document of the catalog: what an install reads for the one plugin it is installing.
	// The wildcard is the whole file name, which is all a mux pattern may match, and `filepath.Base`
	// keeps a path that tries to climb out of the fixtures directory from naming a file above it.
	mux.HandleFunc("GET /plugins/{file}", func(w http.ResponseWriter, r *http.Request) {
		file := filepath.Base(r.PathValue("file"))
		serve(faceCatalog, filepath.Join(dir, "plugins", file), "application/json")(w, r)
	})
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
