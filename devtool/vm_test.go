package main

import (
	"strings"
	"testing"
)

// TestParseTartListTakesTheLocalRow holds the one distinction the whole file rests on: a local VM
// can be started, stopped and deleted, and an OCI row carrying the same name is an image on a shelf.
// Confusing them would have `vm up` try to run an image, and `vm rm` delete a pulled base.
func TestParseTartListTakesTheLocalRow(t *testing.T) {
	vms, err := parseTartList(`[
	  {"Name":"amenbo-golden","Source":"local","State":"stopped","Running":false},
	  {"Name":"amenbo-vm","Source":"local","State":"running","Running":true},
	  {"Name":"ghcr.io/cirruslabs/macos-tahoe-base:latest","Source":"OCI","State":"stopped","Running":false}
	]`)
	if err != nil {
		t.Fatal(err)
	}
	clone, ok := findVM(vms, vmCloneName)
	if !ok || !clone.Running {
		t.Errorf("findVM(%q) = %+v, %v; want the running local row", vmCloneName, clone, ok)
	}
	if _, ok := findVM(vms, vmBase); ok {
		t.Errorf("findVM(%q) found a local VM; an OCI image is not one to start or delete", vmBase)
	}
	if !hasImage(vms, vmBase) {
		t.Errorf("hasImage(%q) = false; the pulled base is present", vmBase)
	}
}

// TestFindVMAnswersNoneWhenTheCloneIsGone covers the state every reach-in command guards on: with
// no clone, an address must not be composed at all.
func TestFindVMAnswersNoneWhenTheCloneIsGone(t *testing.T) {
	vms, err := parseTartList(`[{"Name":"amenbo-golden","Source":"local","State":"stopped","Running":false}]`)
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := findVM(vms, vmCloneName); ok {
		t.Errorf("findVM(%q) = true with only the golden present", vmCloneName)
	}
}

// TestVersionDriftIgnoresThePatch pins where the line is drawn. A guest one security update behind
// is the ordinary state of an image republished weekly; warning on it every run is how the warning
// stops being read. A minor apart is the case actually measured (host 26.5.2 / guest 26.6.2), and it
// is the one worth saying.
func TestVersionDriftIgnoresThePatch(t *testing.T) {
	for _, c := range []struct {
		host, guest string
		drifted     bool
	}{
		{"26.5.2", "26.5.2", false},
		{"26.5.2", "26.5.4", false},
		{"26.5", "26.5.2", false},
		{"26.5.2", "26.6.2", true},
		{"26.5.2", "27.0", true},
	} {
		if got := versionsDrifted(c.host, c.guest); got != c.drifted {
			t.Errorf("versionsDrifted(%q, %q) = %v, want %v", c.host, c.guest, got, c.drifted)
		}
	}
}

// TestVMPushArgsTakesTheLastWordAsTheDestination reads the argument list the way cp and scp do. A
// single word is refused rather than guessed at: it names a destination with nothing to put there
// just as readily as a file with nowhere to go.
func TestVMPushArgsTakesTheLastWordAsTheDestination(t *testing.T) {
	locals, remote, err := vmPushArgs([]string{"Amenbo.app", "scenarios", "/Users/admin/"})
	if err != nil {
		t.Fatal(err)
	}
	if strings.Join(locals, " ") != "Amenbo.app scenarios" || remote != "/Users/admin/" {
		t.Errorf("vmPushArgs = %v, %q", locals, remote)
	}
	if _, _, err := vmPushArgs([]string{"/Users/admin/"}); err == nil {
		t.Error("vmPushArgs with one word returned no error; a lone path is not a send")
	}
	if _, _, err := vmPushArgs(nil); err == nil {
		t.Error("vmPushArgs with nothing returned no error")
	}
}

// TestSSHArgsNeitherChecksNorRemembersTheHostKey pins the one option set that would otherwise be
// re-derived at each call site. A clone is cut fresh from the golden and carries a new host key
// every time, so a remembered entry refuses the next clone instead of catching anything.
func TestSSHArgsNeitherChecksNorRemembersTheHostKey(t *testing.T) {
	args := strings.Join(sshArgs("192.168.64.3", "sw_vers"), " ")
	for _, want := range []string{
		"StrictHostKeyChecking=no",
		"UserKnownHostsFile=/dev/null",
		"IdentitiesOnly=yes",
		vmKeyPath(),
		vmUser + "@192.168.64.3",
	} {
		if !strings.Contains(args, want) {
			t.Errorf("sshArgs = %q, missing %q", args, want)
		}
	}
	if !strings.HasSuffix(args, "sw_vers") {
		t.Errorf("sshArgs = %q; the command has to come last, after the destination", args)
	}
}

// TestScreenLandsWhereEveryCallerLooks holds the guest path literally: what sends the tool and what
// runs it are different commands, in different sessions, and this string is all they share.
func TestScreenLandsWhereEveryCallerLooks(t *testing.T) {
	if got, want := vmScreenPath, "/Users/admin/screen"; got != want {
		t.Errorf("vmScreenPath = %q, want %q", got, want)
	}
}

// TestGuestPathsAreOneAgreement pins the paths a run's four commands share. `run`, `step`, `log` and
// `pull` are separate invocations with nothing between them but the guest's own disk, so a path that
// drifted on one side would have a command reading a file another one never wrote.
func TestGuestPathsAreOneAgreement(t *testing.T) {
	for _, c := range []struct{ got, want string }{
		{vmVerifyBin, "/Users/admin/verify-gui"},
		{vmVerifySteps, "/Users/admin/verify-gui-steps.txt"},
		{vmVerifyLog, "/Users/admin/verify-gui.log"},
		{vmVerifyEvidence, "/Users/admin/verify-gui-evidence"},
		{vmScreenSource, "/Users/admin/screen.swift"},
		{vmGuestApp, "/Users/admin/Applications/Amenbo.app"},
	} {
		if c.got != c.want {
			t.Errorf("guest path = %q, want %q", c.got, c.want)
		}
	}
}

// TestForeignArchIsRefusedByName covers the failure that would otherwise surface several steps from
// its cause: a build for the other architecture installs cleanly and then will not start.
func TestForeignArchIsRefusedByName(t *testing.T) {
	if err := refuseForeignArch("/x/amenbo-darwin-amd64.pkg", "arm64"); err == nil {
		t.Error("an amd64 build was accepted for an arm64 guest")
	}
	if err := refuseForeignArch("/x/amenbo-darwin-arm64.pkg", "arm64"); err != nil {
		t.Errorf("the matching build was refused: %v", err)
	}
	if err := refuseForeignArch("/x/amenbo-darwin-arm64.pkg", "x86_64"); err == nil {
		t.Error("an arm64 build was accepted for an x86_64 guest")
	}
	// A name that says nothing about architecture is nobody's to refuse: a path a caller chose is
	// still the build they meant, and the installer is the one that knows.
	if err := refuseForeignArch("/x/amenbo.pkg", "arm64"); err != nil {
		t.Errorf("a name carrying no architecture was refused: %v", err)
	}
}

// TestPkgArchNamesTheDistributable holds the two words a mac distributable is published under
// against the two `uname -m` answers, since the download picks a file by that name.
func TestPkgArchNamesTheDistributable(t *testing.T) {
	if got := pkgArch("arm64"); got != "arm64" {
		t.Errorf("pkgArch(arm64) = %q", got)
	}
	if got := pkgArch("x86_64"); got != "amd64" {
		t.Errorf("pkgArch(x86_64) = %q", got)
	}
}

// TestOriginRepoReadsBothURLForms covers the two shapes git writes, since the repository a build is
// downloaded from is read off the remote rather than written down.
func TestOriginRepoReadsBothURLForms(t *testing.T) {
	dir := t.TempDir()
	for _, c := range []struct{ url, want string }{
		{"git@github.com:owner/name.git", "owner/name"},
		{"https://github.com/owner/name.git", "owner/name"},
		{"https://github.com/owner/name", "owner/name"},
		{"ssh://git@github.com/owner/name.git", "owner/name"},
	} {
		if _, err := run(dir, "git", "init", "-q"); err != nil {
			t.Skipf("no git here: %v", err)
		}
		if _, err := run(dir, "git", "remote", "remove", "origin"); err != nil {
			_ = err // there is none on the first pass, which is not a failure
		}
		if _, err := run(dir, "git", "remote", "add", "origin", c.url); err != nil {
			t.Fatal(err)
		}
		got, err := originRepo(dir)
		if err != nil || got != c.want {
			t.Errorf("originRepo(%q) = %q, %v; want %q", c.url, got, err, c.want)
		}
	}
	if _, err := run(dir, "git", "remote", "set-url", "origin", "https://gitlab.com/owner/name.git"); err != nil {
		t.Fatal(err)
	}
	if _, err := originRepo(dir); err == nil {
		t.Error("a non-GitHub remote was accepted; downloading from the wrong repository is worse than refusing")
	}
}
