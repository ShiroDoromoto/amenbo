package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestSettingValuesFillsOnlyWhatTheGateWouldRefuse pins which settings a round writes. The gate refuses to
// open while a required setting is empty, so those are filled; an optional one is left empty on purpose,
// because empty is a state the plugin is meant to run in and filling it would hide the branch that reads it.
func TestSettingValuesFillsOnlyWhatTheGateWouldRefuse(t *testing.T) {
	m := roundManifest{
		Name: "slack",
		Config: []configField{
			{Key: "webhook_url", Required: true},
			{Key: "channel"},
			{Key: "events", Required: true, Type: "multi", Default: "task.created,task.done"},
		},
	}
	got, err := settingValues(m, []string{"webhook_url=http://127.0.0.1:9/x"})
	if err != nil {
		t.Fatalf("settingValues: %v", err)
	}
	if len(got) != 2 {
		t.Fatalf("wrote %d settings, want the two required ones: %+v", len(got), got)
	}
	if got[0].key != "webhook_url" || got[0].value != "http://127.0.0.1:9/x" || got[0].filled {
		t.Errorf("what the caller named must be taken as given: %+v", got[0])
	}
	if got[1].key != "events" || got[1].value != "task.created,task.done" || !got[1].filled {
		t.Errorf("a required setting nobody named is filled from the field: %+v", got[1])
	}
}

// TestSettingValuesRefusesASettingTheManifestDoesNotDeclare — amenbo refuses one too, and refusing here
// says so before a store has been made for it.
func TestSettingValuesRefusesASettingTheManifestDoesNotDeclare(t *testing.T) {
	m := roundManifest{Name: "slack", Config: []configField{{Key: "webhook_url", Required: true}}}
	if _, err := settingValues(m, []string{"webhook_url=x", "colour=blue"}); err == nil {
		t.Fatal("a setting the manifest does not declare must be refused")
	}
	if _, err := settingValues(m, []string{"webhook_url"}); err == nil {
		t.Fatal("--set without a value must be refused")
	}
}

// TestFillerForTakesTheValueFromTheFieldItself — a field with candidates takes only its candidates, so a
// filler invented here would be refused at the door. The value has to come from the declaration.
func TestFillerForTakesTheValueFromTheFieldItself(t *testing.T) {
	for _, tc := range []struct {
		name  string
		field configField
		want  string
	}{
		{"the author's default wins", configField{Default: "task.created"}, "task.created"},
		{"every candidate, for a field that takes several", configField{
			Type:    "multi",
			Options: []configOption{{Value: "a"}, {Value: "b"}},
		}, "a,b"},
		{"the first candidate, for a field that takes one", configField{
			Options: []configOption{{Value: "a"}, {Value: "b"}},
		}, "a"},
		{"free text says where it came from", configField{Key: "webhook_url"}, "devtool-round"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if got := fillerFor(tc.field); got != tc.want {
				t.Errorf("fillerFor = %q, want %q", got, tc.want)
			}
		})
	}
}

// TestChosenEventKindsRefusesWhatItCannotFire — a misspelled kind that quietly fired nothing would read as
// "the plugin never got it", which is the one diagnosis a round exists to make trustworthy.
func TestChosenEventKindsRefusesWhatItCannotFire(t *testing.T) {
	all, err := chosenEventKinds("all")
	if err != nil || len(all) != len(roundEventKinds) {
		t.Fatalf("all = %v (%v), want every kind", all, err)
	}
	if empty, err := chosenEventKinds(""); err != nil || len(empty) != len(roundEventKinds) {
		t.Errorf("an empty spec reads as all: %v (%v)", empty, err)
	}
	some, err := chosenEventKinds("deleted, comment")
	if err != nil {
		t.Fatalf("a subset: %v", err)
	}
	if strings.Join(some, ",") != "deleted,comment" {
		t.Errorf("a subset fires in the order it was written: %v", some)
	}
	if _, err := chosenEventKinds("created,exploded"); err == nil {
		t.Error("an unknown kind must be refused")
	}
}

// TestParseRoundManifestNeedsTheNameItInstallsUnder — the name is the directory, so a manifest without one
// has nowhere to be laid down; and the JSON form is the one an install writes, so YAML is refused rather
// than read halfway.
func TestParseRoundManifestNeedsTheNameItInstallsUnder(t *testing.T) {
	m, err := parseRoundManifest([]byte(`{"name":"slack","config":[{"key":"webhook_url","required":true}]}`))
	if err != nil {
		t.Fatalf("parseRoundManifest: %v", err)
	}
	if m.Name != "slack" || len(m.Config) != 1 || !m.Config[0].Required {
		t.Errorf("read back %+v", m)
	}
	if _, err := parseRoundManifest([]byte(`{"desc":"no name"}`)); err == nil {
		t.Error("a manifest naming no plugin must be refused")
	}
	if _, err := parseRoundManifest([]byte("name: slack\n")); err == nil {
		t.Error("a YAML manifest must be refused, not half-read")
	}
}

// TestLayDownPluginWritesWhatAnInstallWouldHaveWritten pins the two file names amenbo looks for: the
// manifest under `manifest.json`, and the executable under the plugin's own name. Get either wrong and the
// plugin reads as "not installed", which looks like a store problem rather than a harness one.
func TestLayDownPluginWritesWhatAnInstallWouldHaveWritten(t *testing.T) {
	base := t.TempDir()
	manifest := filepath.Join(base, "manifest.json")
	if err := os.WriteFile(manifest, []byte(`{"name":"stand-in"}`), 0o644); err != nil {
		t.Fatal(err)
	}
	dumped := filepath.Join(base, "payload.jsonl")
	if err := layDownPlugin(base, "stand-in", manifest, "", dumped); err != nil {
		t.Fatalf("layDownPlugin: %v", err)
	}
	home := filepath.Join(base, "plugins", "stand-in")
	if _, err := os.Stat(filepath.Join(home, "manifest.json")); err != nil {
		t.Errorf("the manifest is not where amenbo reads it: %v", err)
	}
	program := filepath.Join(home, "stand-in"+exeSuffix())
	info, err := os.Stat(program)
	if err != nil {
		t.Fatalf("the executable is not under the plugin's own name: %v", err)
	}
	if info.Mode().Perm()&0o111 == 0 {
		t.Errorf("the program is not executable (%v)", info.Mode())
	}
	body, err := os.ReadFile(program)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(body), dumped) {
		t.Errorf("the stand-in records nowhere:\n%s", body)
	}
}

// TestShellQuoteSurvivesAPathWithASpace — the stand-in is a script, and an unquoted path is a script that
// works until the first machine whose temp directory has a space in it.
func TestShellQuoteSurvivesAPathWithASpace(t *testing.T) {
	if got := shellQuote("/tmp/a b/payload.jsonl"); got != "'/tmp/a b/payload.jsonl'" {
		t.Errorf("shellQuote = %s", got)
	}
	if got := shellQuote("/tmp/it's/x"); got != `'/tmp/it'\''s/x'` {
		t.Errorf("a quote inside is closed and reopened: %s", got)
	}
}
