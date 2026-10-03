package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestAgentsAsideMovesEveryAgentOutOfTheWay holds the three the v32.0.0 run moved by hand. One left
// out is a `can-start` premise that stops on that name.
func TestAgentsAsideMovesEveryAgentOutOfTheWay(t *testing.T) {
	aside := vmAgentsAsideCommand(vmGuestAgents)
	for _, a := range []string{"/Users/admin/bin/claude", "/Users/admin/.local/bin/claude", "/opt/homebrew/bin/codex"} {
		if !strings.Contains(aside, "mv "+shq(a)+" "+shq(a+vmAgentAside)) {
			t.Errorf("%s is not moved aside:\n%s", a, aside)
		}
	}
}

// TestAgentsBackIsTheMirror pins the put-back: every aside name goes back to where it came from, so a
// screen road that reads the real `claude` finds it after a CLI run.
func TestAgentsBackIsTheMirror(t *testing.T) {
	back := vmAgentsBackCommand(vmGuestAgents)
	for _, a := range vmGuestAgents {
		if !strings.Contains(back, "mv "+shq(a+vmAgentAside)+" "+shq(a)) {
			t.Errorf("%s is not put back:\n%s", a, back)
		}
	}
}

// TestRoadMovesCodexAsideAndLeavesClaude pins what a screen road has out of the way: the golden's
// `codex`, which stops a `can-start` premise, and not the seeded `claude`, which the road that reads
// a picture asks.
func TestRoadMovesCodexAsideAndLeavesClaude(t *testing.T) {
	aside := vmAgentsAsideCommand(vmRoadAgents)
	if !strings.Contains(aside, "mv "+shq("/opt/homebrew/bin/codex")+" "+shq("/opt/homebrew/bin/codex"+vmAgentAside)) {
		t.Errorf("codex is not moved aside for a road:\n%s", aside)
	}
	if strings.Contains(aside, claudeGuestBin) {
		t.Errorf("a road moves the seeded claude aside:\n%s", aside)
	}
	back := vmAgentsBackCommand(vmRoadAgents)
	if !strings.Contains(back, "mv "+shq("/opt/homebrew/bin/codex"+vmAgentAside)+" "+shq("/opt/homebrew/bin/codex")) {
		t.Errorf("codex is not put back after a road:\n%s", back)
	}
}

// TestAgentsMoveLeavesWhatIsAlreadyThere is the half that makes both safe to repeat. A run cut off
// before the put-back leaves the aside name standing, and moving again over it would throw the real
// program away; a program put back by hand in the meantime is not overwritten on the way back.
func TestAgentsMoveLeavesWhatIsAlreadyThere(t *testing.T) {
	for _, cmd := range []string{vmAgentsAsideCommand([]string{"/x/claude"}), vmAgentsBackCommand([]string{"/x/claude"})} {
		if !strings.Contains(cmd, "[ ! -e ") || !strings.Contains(cmd, "[ ! -L ") {
			t.Errorf("the move does not look at where it is going first:\n%s", cmd)
		}
	}
}

// TestVerifyAllRunsTheShippedCLIFromHome pins the line the guest runs: from the home directory (so
// no scenario named reads the whole `scenarios/`), driving the shipped CLI, and with `--json` only
// when asked.
func TestVerifyAllRunsTheShippedCLIFromHome(t *testing.T) {
	all := vmVerifyAllCommand(nil, false)
	if !strings.HasPrefix(all, "cd "+shq(vmGuestHome)+" && ") {
		t.Errorf("verify-all is not run from the home directory:\n%s", all)
	}
	if !strings.Contains(all, "--bin "+shq(vmVerifyShippedCLI)) {
		t.Errorf("verify-all is not pointed at the shipped CLI:\n%s", all)
	}
	if strings.Contains(all, "--json") {
		t.Errorf("--json is passed without being asked for:\n%s", all)
	}
	one := vmVerifyAllCommand([]string{vmGuestHome + "/scenarios/a-road.yaml"}, true)
	if !strings.Contains(one, shq(vmGuestHome+"/scenarios/a-road.yaml")) || !strings.HasSuffix(one, "--json") {
		t.Errorf("the named scenario or --json is lost:\n%s", one)
	}
}

// TestVerifyCLIArgsTellsTheBuildFromTheScenarios keeps the positional scenarios verify-all narrows
// by, beside the one .pkg every `vm verify` command takes.
func TestVerifyCLIArgsTellsTheBuildFromTheScenarios(t *testing.T) {
	pkg, scenarios, err := vmVerifyCLIArgs([]string{"a-road", "dist/amenbo-darwin-arm64.pkg", "b-road.yaml"})
	if err != nil {
		t.Fatal(err)
	}
	if pkg != "dist/amenbo-darwin-arm64.pkg" {
		t.Errorf("pkg = %q", pkg)
	}
	if strings.Join(scenarios, " ") != "a-road b-road.yaml" {
		t.Errorf("scenarios = %q", scenarios)
	}
	if _, _, err := vmVerifyCLIArgs([]string{"a.pkg", "b.pkg"}); err == nil {
		t.Error("two builds are taken without a word")
	}
}

// TestVerifyCLIScenariosNamesTheGuestCopy turns a path in the tree or a bare id into the copy sent to
// the guest, and refuses one that is not there before anything is sent.
func TestVerifyCLIScenariosNamesTheGuestCopy(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "a-road.yaml"), []byte("id: a-road\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	got, err := vmVerifyCLIScenarios(dir, []string{"verification/scenarios/a-road.yaml", "a-road"})
	if err != nil {
		t.Fatal(err)
	}
	want := vmGuestHome + "/scenarios/a-road.yaml"
	if len(got) != 2 || got[0] != want || got[1] != want {
		t.Errorf("got %q, want %q twice", got, want)
	}
	if _, err := vmVerifyCLIScenarios(dir, []string{"no-such-road"}); err == nil {
		t.Error("a scenario that is not there is taken")
	}
}

// TestClearRemovesWhatAnEarlierSendLeft pins what goes before a send: the scenarios and fixtures
// folders in the guest, so a scenario this checkout no longer has is not walked.
func TestClearRemovesWhatAnEarlierSendLeft(t *testing.T) {
	cmd := vmVerifyClearCommand()
	want := "rm -rf " + shq(vmGuestHome+"/scenarios") + " " + shq(vmVerifyFixtures)
	if cmd != want {
		t.Errorf("clear command:\n got %s\nwant %s", cmd, want)
	}
}

// TestFixturesLinkPointsTheCompiledPathAtTheSentOnes pins where the fixtures are found: the harness
// reads them from the path it was compiled at, so that path in the guest leads to what was sent.
func TestFixturesLinkPointsTheCompiledPathAtTheSentOnes(t *testing.T) {
	cmd := vmFixturesLinkCommand("/Users/alice/amenbo/verification/fixtures")
	if !strings.Contains(cmd, "mkdir -p '/Users/alice/amenbo/verification'") {
		t.Errorf("the compiled path's directory is not made:\n%s", cmd)
	}
	if !strings.Contains(cmd, "ln -sfn "+shq(vmVerifyFixtures)+" '/Users/alice/amenbo/verification/fixtures'") {
		t.Errorf("the compiled path does not lead to the fixtures sent:\n%s", cmd)
	}
}
