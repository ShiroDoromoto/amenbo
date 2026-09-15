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

// TestFixtureHandlerServesTheUpdateFace pins the route to the app's own URL, which is what makes the
// fake world a stand-in rather than a mock: the same client asks at the same path and reads the same
// bytes.
func TestFixtureHandlerServesTheUpdateFace(t *testing.T) {
	dir := writeTree(t, map[string]string{"update/latest.json": `{"version":"9.9.9"}`})
	h := fixtureHandler(dir, nil, time.Millisecond)

	code, body := get(t, h, "/update/latest.json")
	if code != http.StatusOK || body != `{"version":"9.9.9"}` {
		t.Errorf("/update/latest.json = %d %q, want 200 with the captured manifest", code, body)
	}
}

// TestFixtureHandlerAnswers404ForWhatWasNotCaptured pins absence: a release with no manifest is what
// the real address 404s, and a fixture that is not there says the same thing, so the absent file needs
// no separate way of expressing it.
func TestFixtureHandlerAnswers404ForWhatWasNotCaptured(t *testing.T) {
	h := fixtureHandler(writeTree(t, nil), nil, time.Millisecond)

	if code, _ := get(t, h, "/update/latest.json"); code != http.StatusNotFound {
		t.Errorf("a capture that was never taken = %d, want 404", code)
	}
}

// TestFixtureHandlerFailsOnPurpose covers the half the real address cannot be asked for. A rate limit
// is the case in point: the branch that handles it is unreachable against the real host, because the
// way to reach it there is to spend the quota.
func TestFixtureHandlerFailsOnPurpose(t *testing.T) {
	dir := writeTree(t, map[string]string{"update/latest.json": `{"version":"9.9.9"}`})
	rules, err := parseFailures([]string{"update=429"})
	if err != nil {
		t.Fatal(err)
	}
	h := fixtureHandler(dir, rules, time.Millisecond)

	if code, _ := get(t, h, "/update/latest.json"); code != http.StatusTooManyRequests {
		t.Errorf("update face = %d, want 429", code)
	}
}

// TestFixtureHandlerHangsWithoutAnswering covers `timeout`, which answers nothing at all — a request
// that hangs is a different failure from one that comes back wrong, and the client's own timeout is
// what ends it.
func TestFixtureHandlerHangsWithoutAnswering(t *testing.T) {
	rules, err := parseFailures([]string{"update=timeout"})
	if err != nil {
		t.Fatal(err)
	}
	h := fixtureHandler(writeTree(t, map[string]string{"update/latest.json": "{}"}), rules, 30*time.Millisecond)

	start := time.Now()
	code, body := get(t, h, "/update/latest.json")
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

	for _, bad := range []string{"update", "nowhere=500", "update=teapot", "update=42"} {
		if _, err := parseFailures([]string{bad}); err == nil {
			t.Errorf("--fail %q was accepted", bad)
		}
	}
}

// TestFixtureEnvNamesWhatTheAppReads pins the whole interface to the app: one name it already reads,
// pointed at the fake host.
func TestFixtureEnvNamesWhatTheAppReads(t *testing.T) {
	env := fixtureEnv("http://127.0.0.1:1234")
	want := []string{"AMENBO_UPDATE_JSON_URL=http://127.0.0.1:1234/update/latest.json"}
	for i, w := range want {
		if env[i] != w {
			t.Errorf("env[%d] = %q, want %q", i, env[i], w)
		}
	}
}

// TestInsideBundleTakesTheExecutableOutOfABundle covers what `--app` is given by hand. A bundle is
// what a person has and a directory to `exec`, and the CLI that registers the fake world's catalog
// ships inside it — so resolving one path wrong costs the launch and the catalog both.
func TestInsideBundleTakesTheExecutableOutOfABundle(t *testing.T) {
	dir := t.TempDir()
	bundle := filepath.Join(dir, "amenbo (dev 2291).app")
	macos := filepath.Join(bundle, "Contents", "MacOS")
	if err := os.MkdirAll(macos, 0o755); err != nil {
		t.Fatal(err)
	}
	gui := filepath.Join(macos, "amenbo-app-dev-2291")
	// The CLI ships in the same directory, which is the whole reason the executable is the path to
	// hand on: `devCLI` looks beside it.
	for _, f := range []string{gui, filepath.Join(macos, "amenbo")} {
		if err := os.WriteFile(f, []byte("#!/bin/sh\n"), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	if got := insideBundle(bundle); got != gui {
		t.Errorf("insideBundle(bundle) = %q, want the executable %q", got, gui)
	}

	// Everything that is not a bundle is handed back as it stands: an executable already resolved,
	// a bundle with nothing in it, an empty flag, and a name that is only a name.
	empty := filepath.Join(dir, "hollow.app")
	if err := os.MkdirAll(filepath.Join(empty, "Contents", "MacOS"), 0o755); err != nil {
		t.Fatal(err)
	}
	for _, c := range []string{gui, empty, "", filepath.Join(dir, "absent.app")} {
		if got := insideBundle(c); got != c {
			t.Errorf("insideBundle(%q) = %q, want it unchanged", c, got)
		}
	}
}
