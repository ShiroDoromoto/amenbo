package main

import (
	"encoding/json"
	"strings"
	"testing"
)

// TestGuestClaudeScriptKeepsTheCredentialOffItsOwnLines holds the reason the credential travels on
// stdin at all. Written into the script it would be an argument of `ssh` on this machine and a word
// of a shell in the guest, and both are read by anything that can list processes.
func TestGuestClaudeScriptKeepsTheCredentialOffItsOwnLines(t *testing.T) {
	script := guestClaudeScript("2.1.270")
	if !strings.Contains(script, "IFS= read -r cred") {
		t.Errorf("the script does not read the credential off its stdin:\n%s", script)
	}
	if !strings.Contains(script, `-w "$cred"`) {
		t.Errorf("the credential reaches the keychain as something other than the line that was read:\n%s", script)
	}
}

// TestGuestClaudeScriptAsksForTheHostsVersion pins the version onto both halves that can move: the
// question asked of the guest, and the install run when the answer differs. A script that installed
// `latest` would walk the road against a build the operator does not have.
func TestGuestClaudeScriptAsksForTheHostsVersion(t *testing.T) {
	script := guestClaudeScript("2.1.270")
	if !strings.Contains(script, `!= "2.1.270"`) {
		t.Errorf("the guest is not held against the host's version:\n%s", script)
	}
	if !strings.Contains(script, "bash -s 2.1.270") {
		t.Errorf("the install does not name the version:\n%s", script)
	}
}

// TestGuestClaudeScriptPutsThePathLineAtTheEndAndOnlyOnce covers two things one line has to hold.
// It is taken out before it is written, so a clone raised ten times carries one of it and a clone
// raised before the line moved is corrected. And `~/.local/bin` goes on the end of the `PATH`: the
// screen roads hand the guest a directory of their own in front of it and stand programs up in
// there under these same names, so a profile that prepended would take `claude` back.
func TestGuestClaudeScriptPutsThePathLineAtTheEndAndOnlyOnce(t *testing.T) {
	script := guestClaudeScript("2.1.270")
	if !strings.Contains(script, "sed -i '' -e '/\\.local\\/bin/d' "+claudeGuestShellRC) {
		t.Errorf("the PATH line is written beside whatever is already there:\n%s", script)
	}
	if !strings.Contains(script, `'export PATH="$PATH:$HOME/.local/bin"'`) {
		t.Errorf("the guest's own agent goes in front of a run's own directory:\n%s", script)
	}
}

// TestGuestClaudeScriptWritesTheSettingsOverWhateverIsThere is the bug this shape was written
// against. The install leaves a `~/.claude.json` of its own, so a write that stood back for a file
// already there wrote nothing at all — and the pane's first screen was the theme question.
func TestGuestClaudeScriptWritesTheSettingsOverWhateverIsThere(t *testing.T) {
	script := guestClaudeScript("2.1.270")
	if strings.Contains(script, "[ -f "+claudeGuestConfig+" ]") {
		t.Errorf("the settings stand back for the file the installer just wrote:\n%s", script)
	}
	if !strings.Contains(script, "> "+claudeGuestConfig) {
		t.Errorf("the settings are not written at all:\n%s", script)
	}
}

// TestClaudeGuestSettingsSayTheThreeThings holds what was measured rather than what reads well.
// Onboarding unfinished and the folder untrusted both put a question on the pane's first screen
// instead of a prompt, and an update taken in the guest moves the version the road is about.
func TestClaudeGuestSettingsSayTheThreeThings(t *testing.T) {
	var settings struct {
		HasCompletedOnboarding bool `json:"hasCompletedOnboarding"`
		AutoUpdates            bool `json:"autoUpdates"`
		Projects               map[string]struct {
			HasTrustDialogAccepted bool `json:"hasTrustDialogAccepted"`
		} `json:"projects"`
	}
	if err := json.Unmarshal([]byte(claudeGuestSettings), &settings); err != nil {
		t.Fatalf("the seeded settings are not JSON: %v", err)
	}
	if !settings.HasCompletedOnboarding {
		t.Error("onboarding is not marked done — the pane's first screen would be the theme question")
	}
	if settings.AutoUpdates {
		t.Error("updates are on — the guest would walk the road against a build the host does not have")
	}
	if !settings.Projects["/"].HasTrustDialogAccepted {
		t.Error("`/` is not trusted — trust is read up the tree, and a run's folder has a name nothing here can know")
	}
	if strings.Contains(claudeGuestSettings, "oauthAccount") {
		t.Error("the settings carry an account block; the credential alone signs the guest in")
	}
}

// TestReadGuestClaudeTellsTheTwoRaisesApart covers the line `vm up` prints: an install that
// happened, and a clone that already had it.
func TestReadGuestClaudeTellsTheTwoRaisesApart(t *testing.T) {
	for _, c := range []struct {
		out       string
		installed bool
	}{
		{"installed\n2.1.270", true},
		{"standing\n2.1.270", false},
	} {
		seeded, err := readGuestClaude(c.out, "2.1.270")
		if err != nil {
			t.Fatalf("readGuestClaude(%q): %v", c.out, err)
		}
		if seeded.installed != c.installed || seeded.version != "2.1.270" {
			t.Errorf("readGuestClaude(%q) = %+v; want installed=%v at 2.1.270", c.out, seeded, c.installed)
		}
	}
}

// TestReadGuestClaudeRefusesAVersionThatDidNotTake is the failure that otherwise reads as success.
// An install can go through and leave an older build standing, and a road walked against it would
// report about a version nobody chose.
func TestReadGuestClaudeRefusesAVersionThatDidNotTake(t *testing.T) {
	if _, err := readGuestClaude("installed\n2.1.260", "2.1.270"); err == nil {
		t.Error("readGuestClaude accepted a guest one version behind the host")
	}
	if _, err := readGuestClaude("standing", "2.1.270"); err == nil {
		t.Error("readGuestClaude accepted an answer with no version in it")
	}
}
