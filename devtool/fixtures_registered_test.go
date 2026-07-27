package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

// TestRegisteredCatalogIsServedBesideTheCapturedOne pins the shape a registration reads: the catalog
// at a URL, and the key it publishes *beside* it. amenbo derives the key's URL from the catalog's, so
// a document moved one directory would leave a catalog that registers with nothing to pin — which
// reads on screen as "it publishes no key" rather than as a broken fixture.
func TestRegisteredCatalogIsServedBesideTheCapturedOne(t *testing.T) {
	h := fixtureHandler(writeTree(t, map[string]string{"catalog.json": `{"catalog_v":1,"plugins":[]}`}), nil, time.Millisecond)

	code, body := get(t, h, "/"+registeredCatalogPath)
	if code != http.StatusOK {
		t.Fatalf("the registered catalog = %d, want 200", code)
	}
	if body == "" {
		t.Fatal("the registered catalog served nothing")
	}
	code, key := get(t, h, "/"+registeredCatalogDir+"/catalog-key.pub")
	if code != http.StatusOK || key != registeredCatalogKey {
		t.Errorf("catalog-key.pub = %d %q, want 200 and the published key", code, key)
	}
	// The captured catalog is still the one the env var points at: this one is a second shelf, not a
	// replacement for the first.
	if code, _ := get(t, h, "/catalog.json"); code != http.StatusOK {
		t.Errorf("the captured catalog = %d, want 200", code)
	}
}

// TestRegisteredCatalogEntriesMatchTheirDetails holds the join between the two documents a catalog is
// served in: every entry names a detail document that is actually served, and carries the digest of
// exactly those bytes. A reader refuses a detail that hashes to something else, so a drift here would
// show up as a plugin that cannot be opened.
func TestRegisteredCatalogEntriesMatchTheirDetails(t *testing.T) {
	docs := registeredCatalogDocs()
	var envelope struct {
		Plugins []struct {
			Name      string `json:"name"`
			Official  bool   `json:"official"`
			DetailSum string `json:"detail_sum"`
		} `json:"plugins"`
	}
	if err := json.Unmarshal(docs[registeredCatalogPath], &envelope); err != nil {
		t.Fatal(err)
	}
	if len(envelope.Plugins) != len(registeredPlugins) {
		t.Fatalf("the catalog lists %d plugins, want %d", len(envelope.Plugins), len(registeredPlugins))
	}
	for _, entry := range envelope.Plugins {
		detail, ok := docs[registeredCatalogDir+"/plugins/"+entry.Name+".json"]
		if !ok {
			t.Errorf("%s is listed with no detail document to open", entry.Name)
			continue
		}
		sum := sha256.Sum256(detail)
		if want := "sha256:" + hex.EncodeToString(sum[:]); entry.DetailSum != want {
			t.Errorf("%s detail_sum = %q, want %q", entry.Name, entry.DetailSum, want)
		}
		// The official badge is the official index's to grant; a catalog anyone may publish claiming
		// it is exactly what the intake refuses.
		if entry.Official {
			t.Errorf("%s calls itself official, which no registered catalog's entry may", entry.Name)
		}
	}
}

// TestDevStoreDirFollowsTheBundle holds the CLI to the same store as the GUI beside it. Getting this
// wrong is silent in both directions: the catalog is registered in a store nobody has open, and the
// window comes up looking exactly as it did before.
func TestDevStoreDirFollowsTheBundle(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Skip("no home directory to resolve app-data against")
	}
	for _, c := range []struct{ gui, want string }{
		{"/Applications/amenbo (dev 2289).app/Contents/MacOS/amenbo-app-dev-2289", taskDevAppData("2289")},
		{"/Applications/amenbo (dev).app/Contents/MacOS/amenbo-app-dev", sharedDevAppData},
		// A build straight out of the tree is in no bundle, and there is one dev build to be.
		{"/repo/app/src-tauri/target/release/amenbo-app-dev", sharedDevAppData},
	} {
		got, err := devStoreDir(c.gui)
		if err != nil {
			t.Fatal(err)
		}
		if want := appDataDir(home, c.want); got != want {
			t.Errorf("devStoreDir(%q) = %q, want %q", c.gui, got, want)
		}
	}
}

// TestDevCLIKeepsAnAlreadyChosenStore covers `--fresh`, which puts the GUI on a throwaway store. The
// CLI has to follow it there: registering the catalog in the permanent store instead would leave the
// window empty and the record somewhere it was never asked for.
func TestDevCLIKeepsAnAlreadyChosenStore(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "amenbo")
	if runtime.GOOS == "windows" {
		bin += ".exe"
	}
	if err := os.WriteFile(bin, nil, 0o755); err != nil {
		t.Fatal(err)
	}
	env := []string{"AMENBO_PLUGIN_CATALOG_URL=http://127.0.0.1:1/catalog.json", "AMENBO_HOME=/somewhere/throwaway"}

	cli, cliEnv, err := devCLI(filepath.Join(dir, "amenbo-app-dev"), env)
	if err != nil {
		t.Fatal(err)
	}
	if cli != bin {
		t.Errorf("cli = %q, want the one beside the GUI (%q)", cli, bin)
	}
	var homes []string
	for _, e := range cliEnv {
		if strings.HasPrefix(e, "AMENBO_HOME=") {
			homes = append(homes, e)
		}
	}
	if len(homes) != 1 || homes[0] != "AMENBO_HOME=/somewhere/throwaway" {
		t.Errorf("AMENBO_HOME = %v, want the throwaway store and nothing beside it", homes)
	}
}

// TestRegisteredCatalogFailsWithTheCatalogFace pins the second shelf to the same face as the first.
// "The catalog is unreachable" is one condition to the app, not one per registration, and a fake world
// where only half of it goes down would put a state on screen that the real one cannot produce.
func TestRegisteredCatalogFailsWithTheCatalogFace(t *testing.T) {
	rules, err := parseFailures([]string{"catalog=500"})
	if err != nil {
		t.Fatal(err)
	}
	h := fixtureHandler(writeTree(t, map[string]string{"catalog.json": "{}"}), rules, time.Millisecond)

	if code, _ := get(t, h, "/"+registeredCatalogPath); code != http.StatusInternalServerError {
		t.Errorf("the registered catalog = %d, want 500 with the catalog face failing", code)
	}
}
