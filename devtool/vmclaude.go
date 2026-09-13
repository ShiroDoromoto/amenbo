package main

import (
	"fmt"
	"strings"
)

// The real Claude Code, put into the clone rather than into the golden.
//
// One road in the pre-distribution set opens a pane on the first agent the operator really has, and
// reads what that agent does with a message that has a picture in it
// (`verification/scenarios/send-a-message-with-a-picture-in-it.yaml`). The stand-ins a stood-up
// machine puts in front of the `PATH` do not read pictures, so the road is only worth walking
// against the product itself — and a clone cut from a bare macOS has no product on it. Measured on
// 2026-09-13 during the 24.3.0 check: the row's first entry was `shell`, the pane came up on a
// prompt with nobody behind it, and the road's own sentence came back as
// `zsh: command not found: SCENARIO`.
//
// **It is not baked into the golden**, and the reason is the credential rather than the binary. A
// golden carrying a signed-in account is an account handed to everyone who ever cuts a clone from
// it, and a token baked into an image is a token that is stale by the time the image is used. Put
// into the clone at the moment it is raised, neither happens: the clone is thrown away, and what it
// carries is what the host's keychain holds right now.
//
// **Four things have to be true before a pane opens on it**, and each was found by walking into it:
//
//  1. the binary is there — at the host's own version, so the guest is not answering for a build
//     the operator does not have;
//  2. `~/.local/bin` is on the interactive shell's `PATH`, and on the **end** of it — the installer
//     says so and does not do it, and a pane is started as a login *and* interactive shell
//     (`app/src-tauri/src/launch.rs`), so `~/.zshrc` is the file that decides whether the probe
//     finds anything. The end rather than the front, because the other roads hand the guest a
//     directory of their own in front of the `PATH` and stand programs up in it under these same
//     names: a profile that prepended would take `claude` back, and the road reading a stand-in's
//     behaviour would be reading this install instead (measured 2026-09-14). Nothing is lost by being
//     last — a clone carries no other `claude`, and the road that wants this one stands nothing up.
//     `~/.zprofile` is cleared of the same directory for the same reason: the golden carries a line
//     putting it in front, and `/etc/zprofile`'s `path_helper` has already moved what a run handed
//     over to the back of the `PATH` by the time either file is read;
//  3. onboarding is behind it and the folder is trusted — otherwise the first screen in the pane is
//     a question, not a prompt. Trust is read up the tree, so trusting `/` covers a run's
//     throwaway folder, whose path nothing here can know in advance;
//  4. the login keychain holds the credential — and holds it where the *console* session can read
//     it. A reach over ssh has a security session of its own, which is why `claude -p` answered
//     `Not logged in` there while the same command run through `launchctl asuser` answered the
//     question (measured 2026-09-13).
//
// What it costs is one turn of the operator's own account each time the road is walked, which is the
// cost that road was written knowing it would carry.

const (
	// claudeInstaller is the vendor's own install script, which is the only supported way to put
	// the native build somewhere. It takes the version as its one argument.
	claudeInstaller = "https://claude.ai/install.sh"
	// claudeService is the name the credential stands under in a macOS keychain, on the host and in
	// the guest alike.
	claudeService = "Claude Code-credentials"
	// claudeGuestBin is where the native install puts the launcher in the guest.
	claudeGuestBin = vmGuestHome + "/.local/bin/claude"
	// claudeGuestConfig is the file that says onboarding is done and which folders are trusted.
	claudeGuestConfig = vmGuestHome + "/.claude.json"
	// claudeGuestKeychain is the guest account's login keychain, named in full because a reach over
	// ssh has to unlock it before it can be written to.
	claudeGuestKeychain = vmGuestHome + "/Library/Keychains/login.keychain-db"
	// claudeGuestShellRC is the file a pane's shell reads its `PATH` out of last, which is why the
	// line that puts this install on the end of it goes here.
	claudeGuestShellRC = vmGuestHome + "/.zshrc"
	// claudeGuestProfile is the file read before it, and the golden carries a line in there putting
	// `~/.local/bin` in *front*. It is cleared rather than written to: a front is what this whole
	// arrangement exists to avoid.
	claudeGuestProfile = vmGuestHome + "/.zprofile"
)

// claudeSeeded is what the guest was left holding, for the line that reports it.
type claudeSeeded struct {
	version string
	// installed says the binary was put there by this raise rather than already standing.
	installed bool
}

// vmSeedClaudeCode puts the host's Claude Code into the running clone and signs it in as the host
// is signed in.
//
// **Nothing here stops a raise**, on the same terms as the display and the version lines beside it.
// Almost everything built on the VM has nothing to do with agents — installing a dev GUI, walking
// the other screen roads — and a host with no `claude`, a locked keychain or a guest that could not
// reach the installer would take all of it down with it. What happens instead is that the state is
// said out loud, and the one road that needs the agent fails where it should: on its own asserts,
// with the pane sitting on a plain prompt.
func vmSeedClaudeCode(ip string) {
	version, err := hostClaudeVersion()
	if err != nil {
		logf("  claude  : none on this machine (%v) — a pane in %s will open on a plain shell", err, vmCloneName)
		return
	}
	cred, err := hostClaudeCredentials()
	if err != nil {
		logf("  claude  : %s is here but signed out (%v) — a pane in %s would open on a login prompt", version, err, vmCloneName)
		return
	}
	seeded, err := guestClaude(ip, version, cred)
	if err != nil {
		logf("  claude  : could not be put into %s (%v) — a pane in there opens on whatever is already standing", vmCloneName, err)
		return
	}
	if seeded.installed {
		logf("  claude  : %s installed in %s, signed in as this machine is", seeded.version, vmCloneName)
	} else {
		logf("  claude  : %s already in %s — the credential was written again", seeded.version, vmCloneName)
	}
}

// hostClaudeVersion answers with the version standing on the host, which is the version the guest
// is given.
//
// **The host's and not the latest.** What the road reports is that a wait measured against one
// version has stopped being enough; an operator who reads that has to be able to hold it against
// the build on their own machine, and a guest that quietly ran ahead would be reporting about
// somebody else's.
func hostClaudeVersion() (string, error) {
	out, err := run("", "claude", "--version")
	if err != nil {
		return "", err
	}
	// `2.1.270 (Claude Code)` — the number is the first word, and the rest is the product saying
	// its own name.
	version, _, _ := strings.Cut(out, " ")
	if version == "" {
		return "", fmt.Errorf("`claude --version` said %q, which carries no version", out)
	}
	return version, nil
}

// hostClaudeCredentials reads the signed-in credential out of the host's login keychain.
//
// It is never printed and never becomes an argument: it goes from here onto the stdin of the reach
// into the guest, so it stands in neither machine's process list.
func hostClaudeCredentials() (string, error) {
	out, err := run("", "security", "find-generic-password", "-s", claudeService, "-w")
	if err != nil {
		return "", fmt.Errorf("nothing under %q in this machine's login keychain", claudeService)
	}
	return out, nil
}

// claudeGuestSettings is what `~/.claude.json` is seeded with when the guest has none.
//
// It carries no account: the credential alone is what signs the guest in, and an account block
// copied across would put the operator's name, address and organisation into the guest for nothing
// (measured — a guest holding only these keys came up on the same plan as the host).
//
// `/` is trusted because trust is read up the tree and a verification run's folder is made while
// the run is going, with a name nothing here can be told in advance. What is being trusted is a
// throwaway clone's whole disk, which is a machine cut fresh from a public image minutes earlier.
//
// Updates are off so that the version the host pinned is the version the road is walked against.
const claudeGuestSettings = `{"hasCompletedOnboarding":true,"installMethod":"native","autoUpdates":false,"theme":"dark","projects":{"/":{"hasTrustDialogAccepted":true}}}`

// guestClaude is the one reach into the guest that does all four halves, and answers with what is
// standing there afterwards.
//
// Two of them are conditional and two are not. The binary is installed when the version differs, so
// a raise onto an already seeded clone is one round trip that downloads nothing. The `PATH` line is
// taken out and written again rather than left where it is: a clone raised before the line moved to
// the end of the `PATH` is carrying the old one, and a raise is the only thing that would ever
// correct it.
//
// **`~/.claude.json` is written every time, and written over whatever is there.** Only a file
// already standing could be merged into, and the install puts one there itself — a first raise that
// wrote the settings only when the file was absent left the guest asking its theme question on the
// pane's first screen, because the installer had made the file a second earlier (measured
// 2026-09-14). What is lost by overwriting is a throwaway guest's own accumulation; what is kept is
// the three readings this file exists to state.
//
// The credential is written every time too, because it is the half that goes stale.
//
// **Writing it is a delete and an add, never an update.** `add-generic-password -U -A` over an item
// that is already there fails with `SecKeychainItemSetAccess: User interaction is not allowed`:
// widening the access list of a standing item is a thing the keychain asks a person about, and
// there is nobody at that screen. Removing the item first leaves nothing to ask about (measured
// 2026-09-14 — the second raise onto a seeded clone).
func guestClaude(ip, version, cred string) (claudeSeeded, error) {
	out, err := runFed("", cred+"\n", "ssh", sshArgs(ip, guestClaudeScript(version))...)
	if err != nil {
		return claudeSeeded{}, err
	}
	return readGuestClaude(out, version)
}

// guestClaudeScript writes the script the guest runs. The credential arrives on stdin rather than
// in these words; the last line is what readGuestClaude reads.
func guestClaudeScript(version string) string {
	return fmt.Sprintf(`set -e
IFS= read -r cred
standing=$(%[1]s --version 2>/dev/null | cut -d' ' -f1)
if [ "$standing" != %[2]q ]; then
  curl -fsSL %[3]s | bash -s %[2]s >/dev/null 2>&1
  echo installed
else
  echo standing
fi
sed -i '' -e '/\.local\/bin/d' %[4]s %[11]s 2>/dev/null || true
printf '%%s\n' 'export PATH="$PATH:$HOME/.local/bin"' >> %[4]s
printf '%%s\n' %[6]q > %[5]s
security unlock-keychain -p %[7]q %[8]s
security delete-generic-password -a %[9]q -s %[10]q >/dev/null 2>&1 || true
security add-generic-password -A -a %[9]q -s %[10]q -w "$cred"
%[1]s --version | cut -d' ' -f1
`,
		claudeGuestBin, version, claudeInstaller, claudeGuestShellRC,
		claudeGuestConfig, claudeGuestSettings,
		vmPassword, claudeGuestKeychain, vmUser, claudeService, claudeGuestProfile)
}

// readGuestClaude takes the script's two markers off its output: whether this raise installed
// anything, and what version answers in there now.
//
// **The version is read back rather than assumed.** An install that went through and left an older
// build standing reads exactly like one that worked, and the road walked afterwards would be
// reporting about a version nobody chose.
func readGuestClaude(out, want string) (claudeSeeded, error) {
	lines := strings.Fields(out)
	if len(lines) < 2 {
		return claudeSeeded{}, fmt.Errorf("the guest answered %q, which says neither what it did nor what version it has", out)
	}
	seeded := claudeSeeded{version: lines[len(lines)-1], installed: lines[0] == "installed"}
	if seeded.version != want {
		return claudeSeeded{}, fmt.Errorf("this machine has %s and the guest answers %s — the install did not take", want, seeded.version)
	}
	return seeded, nil
}

// guestClaudeVersion is the reading `vm status` reports: what the clone has, or nothing.
func guestClaudeVersion(ip string) string {
	out, err := sshRun(ip, claudeGuestBin+" --version 2>/dev/null | cut -d' ' -f1")
	if err != nil {
		return ""
	}
	return strings.TrimSpace(out)
}
