package main

import (
	"strings"
	"testing"
)

// TestVerifyStartOpensTheKeychainBeforeItLaunches holds the order the whole of this turns on. The
// harness starts the app as a child of itself, so the app inherits the security session this reach
// over ssh has — and a locked login keychain there is an agent that answers `Not logged in` on an
// install that is signed in. An unlock after the launch, or in a call of its own, reaches nothing.
func TestVerifyStartOpensTheKeychainBeforeItLaunches(t *testing.T) {
	start := vmVerifyStartCommand(vmGuestHome + "/scenarios/a-road.yaml")

	unlock := strings.Index(start, "security unlock-keychain")
	launch := strings.Index(start, vmVerifyBin)
	if unlock < 0 {
		t.Fatalf("the run is started without opening the keychain:\n%s", start)
	}
	if launch < 0 || unlock > launch {
		t.Errorf("the keychain is opened after the harness is launched, which reaches nothing:\n%s", start)
	}
	if !strings.Contains(start, claudeGuestKeychain) {
		t.Errorf("the unlock does not name the guest's login keychain:\n%s", start)
	}
}

// TestVerifyStartWalksOnWhenTheKeychainWillNotOpen pins the other half. Almost every road here is
// walked with stand-ins and wants no credential at all, so a guest with no keychain to open must not
// take them down with it — the road that needs the agent fails on its own asserts instead.
func TestVerifyStartWalksOnWhenTheKeychainWillNotOpen(t *testing.T) {
	start := vmVerifyStartCommand(vmGuestHome + "/scenarios/a-road.yaml")

	unlock, _, found := strings.Cut(start, vmVerifySteps)
	if !found {
		t.Fatalf("the start command does not reach the harness at all:\n%s", start)
	}
	if !strings.Contains(unlock, "|| true") {
		t.Errorf("a keychain that will not open stops the whole run:\n%s", unlock)
	}
	if strings.Contains(unlock, "&&") {
		t.Errorf("the launch is chained onto the unlock succeeding:\n%s", unlock)
	}
}

// TestVerifyStartKeepsTheScenarioItWasGiven is the plain half: what is launched is the road the
// caller named, driving the build that was installed.
func TestVerifyStartKeepsTheScenarioItWasGiven(t *testing.T) {
	start := vmVerifyStartCommand("/tmp/a-road.yaml")
	for _, want := range []string{"/tmp/a-road.yaml", vmGuestApp, vmVerifyEvidence, vmVerifyLog} {
		if !strings.Contains(start, want) {
			t.Errorf("the start command does not carry %q:\n%s", want, start)
		}
	}
}
