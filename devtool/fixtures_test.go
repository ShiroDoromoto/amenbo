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

// TestCatalogRepos pins that the capture follows the catalog: whatever repositories it names are the
// ones fetched, so a list kept by hand beside it cannot go stale.
func TestCatalogRepos(t *testing.T) {
	repos := catalogRepos([]byte(`{"catalog_v":1,"plugins":[
		{"name":"a","repo":"owner/a"},
		{"name":"b"},
		{"name":"c","repo":"owner/c","unknown_field":true}]}`))
	if len(repos) != 2 || repos[0] != "owner/a" || repos[1] != "owner/c" {
		t.Errorf("catalogRepos = %v, want [owner/a owner/c]", repos)
	}

	// An envelope this build cannot read costs the repositories, not the run: the catalog is the
	// producer's to grow, and a capture that refuses to run is a capture nobody takes.
	if repos := catalogRepos([]byte("not json")); repos != nil {
		t.Errorf("catalogRepos(garbage) = %v, want nil", repos)
	}
}
