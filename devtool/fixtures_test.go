package main

import (
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// writeTree lays out a fixture directory the way `fixtures refresh` would, so the handler under test
// answers out of the same shape the capture writes.
func writeTree(t *testing.T, files map[string]string) string {
	t.Helper()
	dir := t.TempDir()
	for name, body := range files {
		path := filepath.Join(dir, filepath.FromSlash(name))
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

// get runs one request through the handler and returns the status and body.
func get(t *testing.T, h http.Handler, path string) (int, string) {
	t.Helper()
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, path, nil))
	body, err := io.ReadAll(rec.Result().Body)
	if err != nil {
		t.Fatal(err)
	}
	return rec.Code, string(body)
}

// TestFixtureHandlerServesTheThreeFaces pins the routes to the app's own URLs, which is what makes the
// fake world a stand-in rather than a mock: the same client asks at the same paths and reads the same
// bytes.
func TestFixtureHandlerServesTheThreeFaces(t *testing.T) {
	dir := writeTree(t, map[string]string{
		"catalog.json":              `{"catalog_v":1,"plugins":[]}`,
		"update/latest.json":        `{"version":"9.9.9"}`,
		"github/repos/o__n.json":    `{"stargazers_count":512}`,
		"github/releases/o__n.json": `{"assets":[]}`,
		"github/readme/o__n.md":     "# a plugin",
	})
	h := fixtureHandler(dir, nil, time.Millisecond)

	for _, want := range []struct{ path, body string }{
		{"/catalog.json", `{"catalog_v":1,"plugins":[]}`},
		{"/update/latest.json", `{"version":"9.9.9"}`},
		{"/github/repos/o/n", `{"stargazers_count":512}`},
		{"/github/repos/o/n/releases/latest", `{"assets":[]}`},
		{"/github/repos/o/n/readme", "# a plugin"},
	} {
		code, body := get(t, h, want.path)
		if code != http.StatusOK || body != want.body {
			t.Errorf("%s = %d %q, want 200 %q", want.path, code, body, want.body)
		}
	}
}

// TestFixtureHandlerAnswers404ForWhatWasNotCaptured pins absence: a repository with no release is what
// GitHub 404s, and a fixture that is not there says the same thing, so the absent file needs no
// separate way of expressing it.
func TestFixtureHandlerAnswers404ForWhatWasNotCaptured(t *testing.T) {
	h := fixtureHandler(writeTree(t, map[string]string{"github/repos/o__n.json": "{}"}), nil, time.Millisecond)

	if code, _ := get(t, h, "/github/repos/o/n/releases/latest"); code != http.StatusNotFound {
		t.Errorf("a repository with no captured release = %d, want 404", code)
	}
}

// TestFixtureHandlerFailsOnPurpose covers the half the real API cannot be asked for. A rate limit is
// the case in point: the branch that handles it is unreachable against api.github.com, because the way
// to reach it there is to spend the quota.
func TestFixtureHandlerFailsOnPurpose(t *testing.T) {
	dir := writeTree(t, map[string]string{
		"catalog.json":           `{"catalog_v":1}`,
		"github/repos/o__n.json": `{"stargazers_count":1}`,
	})
	rules, err := parseFailures([]string{"github=429"})
	if err != nil {
		t.Fatal(err)
	}
	h := fixtureHandler(dir, rules, time.Millisecond)

	if code, _ := get(t, h, "/github/repos/o/n"); code != http.StatusTooManyRequests {
		t.Errorf("github face = %d, want 429", code)
	}
	// One face fails; the others are untouched, which is what makes "the market works but the
	// figures do not" a state you can put on screen.
	if code, _ := get(t, h, "/catalog.json"); code != http.StatusOK {
		t.Errorf("catalog face = %d, want 200 (only github was told to fail)", code)
	}
}

// TestFixtureHandlerHangsWithoutAnswering covers `timeout`, which answers nothing at all — a request
// that hangs is a different failure from one that comes back wrong, and the client's own timeout is
// what ends it.
func TestFixtureHandlerHangsWithoutAnswering(t *testing.T) {
	rules, err := parseFailures([]string{"catalog=timeout"})
	if err != nil {
		t.Fatal(err)
	}
	h := fixtureHandler(writeTree(t, map[string]string{"catalog.json": "{}"}), rules, 30*time.Millisecond)

	start := time.Now()
	code, body := get(t, h, "/catalog.json")
	if time.Since(start) < 30*time.Millisecond {
		t.Error("the request came back before the hold elapsed")
	}
	if body != "" {
		t.Errorf("a hung request wrote %q, want nothing", body)
	}
	// Nothing was written, so the recorder reports its own default rather than an answer the client
	// would ever see.
	if code != http.StatusOK {
		t.Errorf("status = %d, want the unwritten default", code)
	}
}

func TestParseFailures(t *testing.T) {
	all, err := parseFailures([]string{"all=500"})
	if err != nil {
		t.Fatal(err)
	}
	for _, f := range faces {
		if all[f].status != 500 {
			t.Errorf("all= left %s at %v", f, all[f])
		}
	}

	for _, bad := range []string{"github", "nowhere=500", "github=teapot", "github=42"} {
		if _, err := parseFailures([]string{bad}); err == nil {
			t.Errorf("--fail %q was accepted", bad)
		}
	}
}

// TestFixtureEnvNamesWhatTheAppReads pins the whole interface to the app: three names it already
// reads, pointed at the fake host.
func TestFixtureEnvNamesWhatTheAppReads(t *testing.T) {
	env := fixtureEnv("http://127.0.0.1:1234")
	want := []string{
		"AMENBO_PLUGIN_CATALOG_URL=http://127.0.0.1:1234/catalog.json",
		"AMENBO_GITHUB_API_URL=http://127.0.0.1:1234/github",
		"AMENBO_UPDATE_JSON_URL=http://127.0.0.1:1234/update/latest.json",
	}
	for i, w := range want {
		if env[i] != w {
			t.Errorf("env[%d] = %q, want %q", i, env[i], w)
		}
	}
}

// TestCatalogEntries pins that the capture follows the catalog: whatever it names — the plugins
// whose detail documents are taken, and the repositories fetched — is what is captured, so a list
// kept by hand beside it cannot go stale.
func TestCatalogEntries(t *testing.T) {
	entries := catalogEntries([]byte(`{"catalog_v":1,"plugins":[
		{"name":"a","repo":"owner/a"},
		{"name":"b"},
		{"name":"c","repo":"owner/c","unknown_field":true}]}`))
	if len(entries) != 3 {
		t.Fatalf("catalogEntries = %v, want three entries", entries)
	}
	if entries[0].Name != "a" || entries[0].Repo != "owner/a" {
		t.Errorf("entries[0] = %v, want {a owner/a}", entries[0])
	}
	if entries[1].Name != "b" || entries[1].Repo != "" {
		t.Errorf("entries[1] = %v, want {b } — an entry with no repository is still a plugin", entries[1])
	}
	if entries[2].Name != "c" || entries[2].Repo != "owner/c" {
		t.Errorf("entries[2] = %v, want {c owner/c}", entries[2])
	}

	// An envelope this build cannot read costs the capture, not the run: the catalog is the
	// producer's to grow, and a capture that refuses to run is a capture nobody takes.
	if entries := catalogEntries([]byte("not json")); entries != nil {
		t.Errorf("catalogEntries(garbage) = %v, want nil", entries)
	}
}

// TestDetailSource pins where the second document is taken from: beside the list it was named in,
// whether that list is the published one or a checkout of the catalog repository.
func TestDetailSource(t *testing.T) {
	if got := detailSource("https://example.invalid/amenbo-plugins/catalog.json", "worktree"); got !=
		"https://example.invalid/amenbo-plugins/plugins/worktree.json" {
		t.Errorf("detailSource(url) = %q", got)
	}
	if got := detailSource("/checkout/site/catalog.json", "worktree"); got !=
		"/checkout/site/plugins/worktree.json" {
		t.Errorf("detailSource(path) = %q", got)
	}
}
