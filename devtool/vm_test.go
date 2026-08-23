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
