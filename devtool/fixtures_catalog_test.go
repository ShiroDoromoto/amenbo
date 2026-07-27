package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// stubValidator answers as `plugin validate --json` would for one manifest, so the aggregation can be
// walked end to end without a Rust build. Everything else in the path — the split into two documents,
// the digest, the curation list, the envelope — is the real code.
func stubValidator(t *testing.T, entry, detail string) {
	t.Helper()
	previous := validateManifest
	validateManifest = func(_, path string) (map[string]any, map[string]any, error) {
		var e, d map[string]any
		if err := decodeDocument([]byte(entry), &e); err != nil {
			t.Fatalf("stub entry: %v", err)
		}
		if err := decodeDocument([]byte(detail), &d); err != nil {
			t.Fatalf("stub detail: %v", err)
		}
		e["name"] = strings.TrimSuffix(filepath.Base(path), ".yaml")
		return e, d, nil
	}
	t.Cleanup(func() { validateManifest = previous })
}

// catalogRepo lays out a checkout the way the catalog repository is laid out: manifests under
// `plugins/`, and the curation list beside them.
func catalogRepo(t *testing.T, manifests []string, featured string) string {
	t.Helper()
	dir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(dir, "plugins"), 0o755); err != nil {
		t.Fatal(err)
	}
	for _, name := range manifests {
		if err := os.WriteFile(filepath.Join(dir, "plugins", name+".yaml"), []byte("name: "+name+"\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if featured != "" {
		if err := os.WriteFile(filepath.Join(dir, "featured.txt"), []byte(featured), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

// TestAggregateBuildsTheTwoDocumentsTheCatalogPublishes holds the aggregation to the shape a client
// reads: an envelope with a version and one entry per manifest, and a detail document per entry.
func TestAggregateBuildsTheTwoDocumentsTheCatalogPublishes(t *testing.T) {
	stubValidator(t,
		`{"author":"amenbo","official":false,"repo":"someone/thing"}`,
		`{"payload_v":1,"scope":"project","url":"https://example.invalid/a.tar.gz"}`)
	repo := catalogRepo(t, []string{"alpha", "zeta"}, "zeta\n# a comment\n\n")

	catalog, details, err := aggregateCatalog(repo, "unused")
	if err != nil {
		t.Fatal(err)
	}
	var envelope struct {
		CatalogV    int              `json:"catalog_v"`
		GeneratedAt string           `json:"generated_at"`
		Plugins     []map[string]any `json:"plugins"`
	}
	if err := json.Unmarshal(catalog, &envelope); err != nil {
		t.Fatal(err)
	}
	if envelope.CatalogV != catalogV || envelope.GeneratedAt == "" {
		t.Errorf("the envelope is not stamped: %+v", envelope)
	}
	if len(envelope.Plugins) != 2 {
		t.Fatalf("want an entry per manifest, got %d", len(envelope.Plugins))
	}
	// Alphabetical, which is what globbing the manifests in order gives — a catalog whose order moves
	// between runs is a diff nobody can read.
	if envelope.Plugins[0]["name"] != "alpha" || envelope.Plugins[1]["name"] != "zeta" {
		t.Errorf("entries are not in manifest order: %v", envelope.Plugins)
	}
	if len(details) != 2 || details["alpha"] == nil {
		t.Errorf("want a detail document per entry, got %v", details)
	}
	// The manifest's own distributable rides through: without a signing key the catalog cannot
	// re-publish it, and dropping it would be a detail that says nothing about where the bytes are.
	if !strings.Contains(string(details["alpha"]), "https://example.invalid/a.tar.gz") {
		t.Errorf("the detail lost its distributable: %s", details["alpha"])
	}
}

// TestDetailSumDigestsTheDocumentAsWritten pins the digest to the bytes the detail file actually holds.
// `detail_sum` is what a client reads off the list it already has to notice a plugin whose install
// information moved, so a digest of a second rendering names nothing.
func TestDetailSumDigestsTheDocumentAsWritten(t *testing.T) {
	stubValidator(t, `{"repo":"someone/thing"}`, `{"payload_v":1,"scope":"project"}`)
	repo := catalogRepo(t, []string{"alpha"}, "")

	catalog, details, err := aggregateCatalog(repo, "unused")
	if err != nil {
		t.Fatal(err)
	}
	var envelope struct {
		Plugins []struct {
			DetailSum string `json:"detail_sum"`
		} `json:"plugins"`
	}
	if err := json.Unmarshal(catalog, &envelope); err != nil {
		t.Fatal(err)
	}
	// Recomputed the way a client would: over the file's bytes, not over a second rendering of it.
	if got, want := envelope.Plugins[0].DetailSum, digestOf(details["alpha"]); got != want {
		t.Errorf("detail_sum %q does not name the document written (%q)", got, want)
	}
}

// digestOf is what a client computes over a detail document it fetched.
func digestOf(body []byte) string {
	sum := sha256.Sum256(body)
	return "sha256:" + hex.EncodeToString(sum[:])
}

// TestFeaturedComesFromTheCurationList holds the recommendation to the catalog's list rather than the
// manifest's claim: a plugin is recommended because the list names it, and a comment or a blank line
// names nothing.
func TestFeaturedComesFromTheCurationList(t *testing.T) {
	stubValidator(t, `{"repo":"someone/thing","featured":true}`, `{"scope":"project"}`)
	repo := catalogRepo(t, []string{"alpha", "zeta"}, "  zeta  # recommended\n\n#alpha\n")

	catalog, _, err := aggregateCatalog(repo, "unused")
	if err != nil {
		t.Fatal(err)
	}
	var envelope struct {
		Plugins []struct {
			Name     string `json:"name"`
			Featured bool   `json:"featured"`
		} `json:"plugins"`
	}
	if err := json.Unmarshal(catalog, &envelope); err != nil {
		t.Fatal(err)
	}
	// `alpha` claimed the badge in its manifest and is commented out of the list: the list wins.
	if envelope.Plugins[0].Featured {
		t.Errorf("a manifest granted itself the recommendation: %+v", envelope.Plugins[0])
	}
	if !envelope.Plugins[1].Featured {
		t.Errorf("the curated plugin is not recommended: %+v", envelope.Plugins[1])
	}
}

// TestASelfDeclaredOfficialManifestIsRefused covers the badge a submitter has most reason to claim. A
// manifest outside the amenbo team claiming it is dropped rather than published with the badge quietly
// removed — a fixture that showed it would be a lie about the market's trust picture.
func TestASelfDeclaredOfficialManifestIsRefused(t *testing.T) {
	stubValidator(t, `{"official":true,"repo":"someone/thing"}`, `{"scope":"project"}`)
	repo := catalogRepo(t, []string{"alpha"}, "")

	if _, _, err := aggregateCatalog(repo, "unused"); err == nil {
		t.Fatal("want the run to end with nothing published")
	}

	stubValidator(t, `{"official":true,"repo":"`+officialOwner+`/thing"}`, `{"scope":"project"}`)
	if _, details, err := aggregateCatalog(repo, "unused"); err != nil || details["alpha"] == nil {
		t.Errorf("the catalog's own official plugin is published: %v", err)
	}
}

// TestAManifestMustAgreeWithItsFileName covers the identity a reviewer sees in the diff, which is also
// the name a client fetches the detail by: a manifest that disagrees with its file name would publish a
// detail nobody can find.
func TestAManifestMustAgreeWithItsFileName(t *testing.T) {
	previous := validateManifest
	validateManifest = func(_, _ string) (map[string]any, map[string]any, error) {
		return map[string]any{"name": "something-else", "repo": "someone/thing"}, map[string]any{"scope": "project"}, nil
	}
	t.Cleanup(func() { validateManifest = previous })

	repo := catalogRepo(t, []string{"alpha"}, "")
	if _, _, err := aggregateCatalog(repo, "unused"); err == nil {
		t.Fatal("want a manifest whose name disagrees with its file to be dropped")
	}
}

// TestDocumentsAreWrittenTheWayTheCatalogWritesThem pins the rendering that makes a fixture a stand-in:
// sorted keys, two-space indent, a trailing newline, and no escaping of characters JSON does not require
// it for.
func TestDocumentsAreWrittenTheWayTheCatalogWritesThem(t *testing.T) {
	body, err := encodeDocument(map[string]any{"z": 1, "a": "<b>&", "m": "日本語"})
	if err != nil {
		t.Fatal(err)
	}
	want := "{\n  \"a\": \"<b>&\",\n  \"m\": \"日本語\",\n  \"z\": 1\n}\n"
	if string(body) != want {
		t.Errorf("document written as %q, want %q", body, want)
	}
}

// TestNumbersRideThroughAsTheyWereWritten guards the round trip a decode-and-republish invites: a
// version number written back as `1e+00` is a document no client can read.
func TestNumbersRideThroughAsTheyWereWritten(t *testing.T) {
	var doc map[string]any
	if err := decodeDocument([]byte(`{"payload_v":1,"big":9007199254740993}`), &doc); err != nil {
		t.Fatal(err)
	}
	body, err := encodeDocument(doc)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(body), `"payload_v": 1`) || !strings.Contains(string(body), "9007199254740993") {
		t.Errorf("numbers did not survive the round trip: %s", body)
	}
}
