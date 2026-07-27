package main

// The fake world's second catalog: one the user registered themselves — the tier anyone may publish
// into, beside the official index.
//
// Everything else the fake world answers is a COPY, taken by `fixtures refresh`. This one cannot be.
// No third-party catalog exists to capture, and what only a registered catalog puts on screen — a
// market row badged with the shelf it came from, that shelf as a choice in the provenance filter, a
// fingerprint shown before a key is pinned — has no other way to appear at all: with the official
// catalog the only one served, the conditions those screens need are never met. So this one is
// invented, and invented in code rather than as files under `devtool/fixtures/`, which keeps that
// tree what it says it is.
//
// It stops at the shelf. Nothing here is signed, so an install off this catalog fails at the
// signature — an install is exercised against the real thing, and what this is for is the seeing.

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

const (
	// registeredCatalogDir is the fake host's path prefix for this catalog. `catalog.json` sits in it
	// and `catalog-key.pub` beside it, which is where a registration goes looking for the key to pin.
	registeredCatalogDir  = "registered"
	registeredCatalogPath = registeredCatalogDir + "/catalog.json"

	// registeredCatalogName is what it is called on screen: the display name given at registration,
	// which is what a market row coming off this catalog is badged with. The closed distribution an
	// own catalog is for is what it says, so a row's badge reads as the shelf rather than as a label.
	registeredCatalogName = "In-house catalog"

	// registeredCatalogKey is the key it publishes — a throwaway devtool holds, and one that signs
	// nothing. Its only job is to be a key that parses, so the registration has a fingerprint to put
	// in front of whoever is agreeing to it.
	registeredCatalogKey = "untrusted comment: minisign public key (devtool's fake world)\n" +
		"RWSw3wZ34b1PMyHu4KajlLhV0SdlMAgQGefo4pFIxv7MgRoWSVpCVXSE\n"

	// registeredCatalogRepo is the repository the invented entries name. It is the one the GitHub
	// capture already holds, so an opened detail draws real figures instead of a 404 from a face that
	// was never asked to answer for a repository nobody captured: the subject here is the shelf, not
	// the repository.
	registeredCatalogRepo = "ShiroDoromoto/amenbo-plugin-worktree"

	// registeredCatalogStamp keeps the served bytes the same from run to run. A capture carries the
	// moment it was taken; an invented document has no such moment to carry.
	registeredCatalogStamp = "2026-01-01T00:00:00Z"
)

// registeredPlugins is what this catalog offers. Two, not one: a badge and a provenance filter are
// about telling shelves apart, and a single row of each reads as a coincidence rather than a rule.
//
// Each carries the two declarations an opened detail draws — the event it is woken for, and the one
// setting it will ask for — because the detail document is the only thing on that panel that came
// from this shelf. Everything above it (the name, the description, the badge) is the entry, which the
// list already held; the scope line under it is a phrase of the interface and reads the same whatever
// document was fetched. So with a bare document there is nothing on screen that says the panel opened
// against *this* catalog rather than against nothing at all — which is exactly the reading
// `verification/scenarios/plugin-detail.yaml` is written for. The setting's label is free text an
// author wrote, so it is a word only this catalog can put there — and it deliberately says nothing
// the description above it already says, since a reading satisfied by the row's own line would pass
// against a panel where the document never arrived.
var registeredPlugins = []struct{ name, desc, category, event, askKey, askLabel string }{
	{"standup", "Post the day's finished tasks to the team channel", "workflow", "task.done", "channel", "Channel webhook"},
	{"burndown", "Chart what is left in the current milestone", "report", "task.status_changed", "milestone", "Milestone name"},
}

// registeredCatalogDocs is every document this catalog serves, keyed by the path it answers at.
//
// The list is built after the details because an entry carries the digest of its own detail document,
// and a reader refuses a detail that does not hash to what the entry said. Computing it here is why
// these are documents rather than files: a hand-written pair drifts the first time one half is edited.
func registeredCatalogDocs() map[string][]byte {
	docs := map[string][]byte{
		registeredCatalogDir + "/catalog-key.pub": []byte(registeredCatalogKey),
	}
	entries := make([]map[string]any, 0, len(registeredPlugins))
	for _, p := range registeredPlugins {
		detail := marshal(map[string]any{
			"name":      p.name,
			"payload_v": 1,
			"scope":     "project",
			"events":    []string{p.event},
			"config": []map[string]any{
				{"key": p.askKey, "label": p.askLabel, "required": true},
			},
		})
		docs[registeredCatalogDir+"/plugins/"+p.name+".json"] = detail
		sum := sha256.Sum256(detail)
		entries = append(entries, map[string]any{
			"name":     p.name,
			"desc":     p.desc,
			"author":   "in-house",
			"repo":     registeredCatalogRepo,
			"os":       []string{"macos", "linux"},
			"category": p.category,
			// Every entry here claims the official badge, and none of them is entitled to it: the
			// badge is the official index's to grant, and this is a shelf anyone may publish into.
			// The claim is deliberate. The merge clears it on everything a registered catalog
			// serves, and a claim nobody makes is a clearing nobody can see — with `false` here,
			// the badge would read as this shelf's name whether the merge folded or did nothing at
			// all. Claimed on both, a merge that stopped folding takes the shelf's name off both
			// rows, which is a screen anyone looking can tell apart.
			"official":   true,
			"featured":   false,
			"added_at":   nil,
			"detail_sum": "sha256:" + hex.EncodeToString(sum[:]),
		})
	}
	docs[registeredCatalogPath] = marshal(map[string]any{
		"catalog_v":    1,
		"generated_at": registeredCatalogStamp,
		"plugins":      entries,
	})
	return docs
}

// marshal encodes one of those documents. The values are literals in this file, so a failure would be
// a typo in the source rather than anything a run could produce.
func marshal(doc map[string]any) []byte {
	body, err := json.Marshal(doc)
	if err != nil {
		panic(err)
	}
	return body
}

// registerFakeCatalog puts this catalog into the store the dev GUI is about to open, so the window
// comes up with it already registered. The port the fake world listens on is picked fresh every run,
// so the URL is never the one that was typed last time — which is exactly the typing this saves.
//
// Registering pins a signing key, which the product stops and asks about, and `--yes` is what answers
// it here: starting `fixtures gui` is the consent, and there is nobody else in the room to ask.
// Failing to register is reported and never fatal — the fake world is still up, and the URL is on
// screen to register by hand.
func registerFakeCatalog(cli string, env []string, url string) {
	dropFakeCatalogs(cli, env, url)
	out, err := amenboAt(cli, env, "plugin", "catalog", "add", url, "--name", registeredCatalogName, "--json", "--yes")
	if err != nil {
		logf("  ! could not register %s (%v)", registeredCatalogName, err)
		return
	}
	// What amenbo answers is what actually happened, and it is worth reading back: the fingerprint is
	// the one now pinned, and the count is this catalog's entries having joined the merged view. A
	// registration that reported nothing would leave "is it on screen yet" to be found out on screen.
	var added struct {
		Fingerprint string `json:"fingerprint"`
		Offered     int    `json:"offered"`
	}
	if err := json.Unmarshal([]byte(out), &added); err != nil {
		logf("  registered %s — %s", registeredCatalogName, url)
		return
	}
	logf("  registered %s (%d plugins, key %s) — %s", registeredCatalogName, added.Offered, added.Fingerprint, url)
}

// dropFakeCatalogs unregisters this catalog from the store: every registration of it except `keep`,
// and all of them when `keep` is empty.
//
// Both ends of a run need it. On the way in, a run that was killed left a registration behind whose
// port nothing answers on any more, and a browse would report it as unreachable for good. On the way
// out, the one this run made is about to become that.
func dropFakeCatalogs(cli string, env []string, keep string) {
	out, err := amenboAt(cli, env, "plugin", "catalog", "list", "--json")
	if err != nil {
		logf("  ! could not read the registered catalogs (%v)", err)
		return
	}
	var listed struct {
		Sources []struct {
			URL string `json:"url"`
		} `json:"sources"`
	}
	if err := json.Unmarshal([]byte(out), &listed); err != nil {
		logf("  ! could not read the registered catalogs (%v)", err)
		return
	}
	for _, s := range listed.Sources {
		if s.URL == keep || !strings.HasSuffix(s.URL, "/"+registeredCatalogPath) {
			continue
		}
		if _, err := amenboAt(cli, env, "plugin", "catalog", "remove", s.URL, "--yes"); err != nil {
			logf("  ! could not unregister %s (%v)", s.URL, err)
			continue
		}
		logf("  unregistered %s", s.URL)
	}
}

// amenboAt runs the dev GUI's own CLI, from outside the repository. Two details are deliberate.
//
// The directory: amenbo offers to install its lint hooks the first time it is run inside a git
// working tree, and a command nobody typed is no place for that question.
//
// The facet: it is never defaulted, and nothing here is stamped with it — for these commands it is
// the reach gate alone. What stands behind them is the person who started the fake world, the same
// person the `--yes` on the registration answers for; `ai` narrows reach to the project a folder is
// bound to, and a throwaway store has neither.
func amenboAt(cli string, env []string, args ...string) (string, error) {
	return runEnv(os.TempDir(), env, cli, append(args, "--actor", "human")...)
}

// devCLI is the amenbo CLI that reads the same store as the dev GUI, and the environment that points
// it at that store.
//
// The binary is the one shipped beside the GUI inside its bundle. Which store a build opens is baked
// into it (`AMENBO_APP_NAME`, the Makefile's GUI_DEV_ block), so the `amenbo` on PATH is the
// production one and would register the catalog where nobody is looking.
//
// The environment names that store outright, because nothing else here can. A CLI reaches a store
// through a bound folder or through `AMENBO_HOME`, and the store a dev GUI opens belongs to a bundle
// rather than to any directory — so `AMENBO_HOME` it is, the same seam `--fresh` uses. When `--fresh`
// has already set one, that one stands: it is the store the GUI will open.
func devCLI(app string, env []string) (string, []string, error) {
	gui := app
	if gui == "" {
		var err error
		if gui, err = devAppBinary(); err != nil {
			return "", nil, err
		}
	}
	cli := filepath.Join(filepath.Dir(gui), "amenbo")
	if runtime.GOOS == "windows" {
		cli += ".exe"
	}
	if _, err := os.Stat(cli); err != nil {
		return "", nil, fmt.Errorf("no amenbo CLI beside %s", gui)
	}
	for _, e := range env {
		if strings.HasPrefix(e, "AMENBO_HOME=") {
			return cli, env, nil
		}
	}
	store, err := devStoreDir(gui)
	if err != nil {
		return "", nil, err
	}
	return cli, append(append([]string{}, env...), "AMENBO_HOME="+store), nil
}

// devStoreDir is the app-data directory of the dev GUI at `gui`, read off the bundle it lives in:
// which instance a bundle is, is what its name says (`devgui.go`), and the app-data name follows from
// that. Outside a bundle there is one dev build and it is the shared one.
func devStoreDir(gui string) (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	name := sharedDevAppData
	// `<bundle>.app/Contents/MacOS/<binary>` — three levels up from the executable is the bundle.
	if id := taskIDFromBundleName(filepath.Base(filepath.Dir(filepath.Dir(filepath.Dir(gui))))); id != "" {
		name = taskDevAppData(id)
	}
	return appDataDir(home, name), nil
}
