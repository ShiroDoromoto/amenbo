package main

import (
	_ "embed"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"syscall"
	"time"
)

// `devtool vm …` — the throwaway macOS VM the GUI is verified in.
//
// Driving a screen means posting CGEvents to `cghidEventTap`, which takes the keyboard and the
// mouse of whatever Mac it runs on for as long as it runs. The way out is a second screen, and a
// VM of the same arch and the same OS generation is one the existing tools work inside unchanged.
// So the arrangement is: the golden image is never started, a clone is; the clone is thrown away
// when a person says so; `screen` is compiled on the host each time rather than baked in.
//
// What lives here is the operating — raise, throw away, reach into, send to — and nothing about
// what is verified: the wrapper that runs `verify-gui` inside, and the dev GUI installed inside,
// are their own commands built on these.
//
// Everything is the host's `tart` and the host's `ssh`. devtool holds no second copy of either's
// behaviour; what it holds is the names (which VM, which key, which user) and the waits, so that
// two callers cannot drift apart over them.
const (
	// vmGoldenName is the image clones are cut from, and the one thing here that is never started:
	// starting it is how a golden picks up state and stops being a known ground.
	vmGoldenName = "amenbo-golden"
	// vmCloneName is the clone raised for verification. One name, because one is what a machine
	// with this much disk holds and what a person means by "the VM".
	vmCloneName = "amenbo-vm"
	// vmBase is the third-party image the golden is made from. SIP disabled, TCC granted,
	// Gatekeeper off — none of which we set, and all of which the screen tools need.
	vmBase = "ghcr.io/cirruslabs/macos-tahoe-base:latest"
	// vmUser is the account the image ships, and the one the console session belongs to.
	vmUser = "admin"
	// vmKeyName is the key enrolled in the golden's authorized_keys, under ~/.ssh.
	vmKeyName = "amenbo-vm"
	// vmPassword is that account's password, published with the third-party image the golden is cut
	// from. It is written down because one road needs it typed rather than held: the postinstall's
	// migration off a system-wide install asks for an admin password on the guest's own screen, and
	// a dialog nobody answers is a road that stops there. What it opens is a throwaway clone on this
	// machine's private network, cut fresh from a public image and thrown away when a person says so.
	vmPassword = "admin"
	// vmScreenPath is where the compiled screen tool is put in the guest. Fixed, so a caller
	// that sent it and a caller that uses it need not agree on anything but this.
	vmScreenPath = "/Users/" + vmUser + "/screen"
	// vmGuestHome is the guest account's home, and the one place anything is put in there.
	vmGuestHome = "/Users/" + vmUser
	// vmDisplayPath is where the compiled display tool is put in the guest, on the same terms.
	vmDisplayPath = vmGuestHome + "/display"
	// vmDisplaySize is the screen verification runs on, in points, declared here rather than
	// inherited from whatever the golden happened to carry: a shot is only comparable against
	// another shot taken on the same screen, and `vm golden --refresh` would otherwise move it.
	//
	// Wide enough for the layouts that only appear on a wide window (the horizontal rail), and
	// read as points so the panel behind it is twice that in pixels — an assert that reads words
	// off a shot needs the 2x.
	vmDisplaySize = "1920x1200"
)

// vmDisplaySource is the display tool, compiled on the host and sent into the guest the way the
// screen tool is. It is carried in the binary rather than read out of the checkout because it is
// devtool's own, not one of the tools this tree ships to a screen.
//
//go:embed vmdisplay.swift
var vmDisplaySource string

// vmCmd dispatches the `vm` subcommands: the two that raise the clone and throw it away, the two
// that reach into it, and the two that report — on the golden, and on the clone.
func vmCmd(args []string) {
	if len(args) == 0 {
		usage()
		os.Exit(2)
	}
	sub := args[0]
	fail := func(err error) {
		if err != nil {
			logf("devtool: %v", err)
			os.Exit(1)
		}
	}
	noArgs := func(fs *flag.FlagSet) {
		if fs.NArg() > 0 {
			logf("devtool: vm %s takes no arguments, got: %s", sub, strings.Join(fs.Args(), " "))
			usage()
			os.Exit(2)
		}
	}

	switch sub {
	case "up":
		fs := flag.NewFlagSet("vm up", flag.ExitOnError)
		fs.Parse(args[1:])
		noArgs(fs)
		fail(vmUp())
	case "rm":
		fs := flag.NewFlagSet("vm rm", flag.ExitOnError)
		fs.Parse(args[1:])
		noArgs(fs)
		fail(vmRm())
	case "status":
		fs := flag.NewFlagSet("vm status", flag.ExitOnError)
		fs.Parse(args[1:])
		noArgs(fs)
		fail(vmStatus())
	case "exec":
		// The guest command is handed over after `--`, for the same reason `devgui cli` does it:
		// without it the first flag of theirs is read as one of ours.
		_, argv, ok := splitDoubleDash(args[1:])
		if !ok || len(argv) == 0 {
			logf("devtool: vm exec passes its command to the guest after `--`, e.g. `devtool vm exec -- sw_vers`")
			os.Exit(2)
		}
		code, err := vmExec(argv)
		fail(err)
		os.Exit(code)
	case "push":
		fs := flag.NewFlagSet("vm push", flag.ExitOnError)
		fs.Parse(args[1:])
		locals, remote, err := vmPushArgs(fs.Args())
		if err != nil {
			logf("devtool: %v", err)
			os.Exit(2)
		}
		fail(vmPush(locals, remote))
	case "screen":
		fs := flag.NewFlagSet("vm screen", flag.ExitOnError)
		fs.Parse(args[1:])
		noArgs(fs)
		fail(vmSendScreen())
	case "verify":
		vmVerifyCmd(args[1:])
	case "golden":
		fs := flag.NewFlagSet("vm golden", flag.ExitOnError)
		refresh := fs.Bool("refresh", false, "pull the base image and cut the golden from it again")
		fs.Parse(args[1:])
		noArgs(fs)
		fail(vmGolden(*refresh))
	default:
		logf("devtool: unknown vm subcommand %q", sub)
		usage()
		os.Exit(2)
	}
}

// ---------------------------------------------------------------------------
// what tart says
// ---------------------------------------------------------------------------

// tartVM is one row of `tart list --format json`. The JSON form is parsed rather than the text one
// because the columns of the text form are laid out for a reader, and `Running` is a field there
// only in JSON — `State` carries words that have grown before now.
type tartVM struct {
	Name    string `json:"Name"`
	Source  string `json:"Source"`
	State   string `json:"State"`
	Running bool   `json:"Running"`
}

// tartVMs lists what tart holds, local and OCI both.
func tartVMs() ([]tartVM, error) {
	out, err := run("", "tart", "list", "--format", "json")
	if err != nil {
		return nil, err
	}
	return parseTartList(out)
}

// parseTartList is the pure half: the rows of `tart list --format json`.
func parseTartList(out string) ([]tartVM, error) {
	var vms []tartVM
	if err := json.Unmarshal([]byte(out), &vms); err != nil {
		return nil, fmt.Errorf("tart list: %w", err)
	}
	return vms, nil
}

// findVM answers with the local VM of that name — local only, because an OCI row carries the same
// name as the image it was pulled from and is not something that can be started or deleted.
func findVM(vms []tartVM, name string) (tartVM, bool) {
	for _, vm := range vms {
		if vm.Name == name && vm.Source == "local" {
			return vm, true
		}
	}
	return tartVM{}, false
}

// hasImage reports whether the base image has been pulled. An OCI row is not startable, so this is
// a separate question from findVM's.
func hasImage(vms []tartVM, name string) bool {
	for _, vm := range vms {
		if vm.Name == name {
			return true
		}
	}
	return false
}

// requireTart refuses early, and by name, when the host has no tart. Every command here is a tart
// command underneath, so a missing one otherwise surfaces as six different "executable file not
// found" messages from six different places.
func requireTart() error {
	if runtime.GOOS != "darwin" {
		return fmt.Errorf("the verification VM is an Apple-silicon macOS guest, so it only exists on macOS")
	}
	if _, err := exec.LookPath("tart"); err != nil {
		return fmt.Errorf("tart is not on PATH — `brew install cirruslabs/cli/tart` (Fair Source 0.9: unlimited on a personal machine)")
	}
	return nil
}

// ---------------------------------------------------------------------------
// raise it, throw it away
// ---------------------------------------------------------------------------

// vmUp raises the clone and prints its IP on stdout, so the address can be handed straight to
// whatever is about to talk to it. Already running, it is used as it stands — that is the whole
// contract: a test does not raise a second VM, and it does not throw away the one a session has
// been working in.
//
// The clone is cut from the golden on the way if there is none (0.03s, no disk of its own until it
// is written to), and the wait runs to a GUI session rather than to a ping: `/dev/console` owned by
// the account is what says a screen exists to draw on, and everything here is for drawing on it.
func vmUp() error {
	ip, err := vmEnsureUp()
	if err != nil {
		return err
	}
	logf("  vm      : %s ready at %s — `devtool vm rm` throws it away", vmCloneName, ip)
	fmt.Printf("%s\n", ip)
	return nil
}

// vmEnsureUp is that same raise, answering with the address instead of printing it — what the
// commands built on top of the VM start with, none of which want an address on their stdout.
func vmEnsureUp() (string, error) {
	if err := requireTart(); err != nil {
		return "", err
	}
	vms, err := tartVMs()
	if err != nil {
		return "", err
	}
	clone, ok := findVM(vms, vmCloneName)
	switch {
	case ok && clone.Running:
		logf("  vm      : %s is already running — using it", vmCloneName)
	case ok:
		logf("  vm      : %s is there but stopped — starting it", vmCloneName)
		if err := setDisplaySize(); err != nil {
			return "", err
		}
		if err := tartRun(); err != nil {
			return "", err
		}
	default:
		if _, ok := findVM(vms, vmGoldenName); !ok {
			return "", fmt.Errorf("no golden image %q — `devtool vm golden --refresh` cuts one from %s", vmGoldenName, vmBase)
		}
		logf("  vm      : cloning %s → %s", vmGoldenName, vmCloneName)
		if _, err := run("", "tart", "clone", vmGoldenName, vmCloneName); err != nil {
			return "", err
		}
		if err := setDisplaySize(); err != nil {
			return "", err
		}
		if err := tartRun(); err != nil {
			return "", err
		}
	}

	ip, err := vmWaitReady()
	if err != nil {
		return "", err
	}
	if err := vmTakeNativeDisplay(ip); err != nil {
		return "", err
	}
	reportVersionDrift(ip)
	return ip, nil
}

// setDisplaySize gives the clone the screen verification runs on, before it is started — the size
// is part of the VM, so it cannot be changed under a running one.
//
// Points, not pixels: tart reads a bare size as points for a macOS guest, and the panel it builds is
// twice that in pixels. Asking in pixels instead builds a 1x panel, which reads badly enough under
// OCR to fail asserts that are otherwise sound.
func setDisplaySize() error {
	_, err := run("", "tart", "set", vmCloneName, "--display", vmDisplaySize)
	return err
}

// vmTakeNativeDisplay puts the guest's screen on the mode its panel is built for.
//
// It has to be asked for. A macOS guest comes up on a stretched 1024x768pt desktop no matter how
// wide the panel behind it is (measured on a clone freshly cut from the golden), which is too narrow
// for the window under verification and stretched under every shot. The mode is in the guest's own
// list the whole time.
//
// Compiled and sent every time rather than kept in the guest, on the same grounds as the screen
// tool: the golden holds no copy of a tool this tree changes, and a clone cannot answer with a
// stale one.
func vmTakeNativeDisplay(ip string) error {
	bin, cleanup, err := buildDisplayTool()
	if err != nil {
		return err
	}
	defer cleanup()
	args := append(sshOpts(), bin, vmUser+"@"+ip+":"+vmDisplayPath)
	if _, err := run("", "scp", args...); err != nil {
		return err
	}
	mode, err := sshRun(ip, vmDisplayPath, "native")
	if err != nil {
		return fmt.Errorf("putting %s on the mode its panel is built for: %w", vmCloneName, err)
	}
	logf("  display : %s", strings.TrimSpace(mode))
	return nil
}

// buildDisplayTool writes the embedded display tool out and compiles it on the host, answering with
// the binary and the way to take the temporary directory back.
func buildDisplayTool() (bin string, cleanup func(), err error) {
	dir, err := os.MkdirTemp("", "amenbo-display-")
	if err != nil {
		return "", func() {}, err
	}
	cleanup = func() { os.RemoveAll(dir) }
	src := filepath.Join(dir, "display.swift")
	if err := os.WriteFile(src, []byte(vmDisplaySource), 0o644); err != nil {
		cleanup()
		return "", func() {}, err
	}
	bin = filepath.Join(dir, "display")
	if _, err := run("", "swiftc", "-O", "-o", bin, src); err != nil {
		cleanup()
		return "", func() {}, fmt.Errorf("compiling the display tool: %w", err)
	}
	return bin, cleanup, nil
}

// reportDisplay says which mode the guest's screen is on, and stays quiet when it cannot ask. The
// tool is put there by `vm up`, so a clone raised any other way simply has nothing to report.
func reportDisplay(ip string) {
	mode, err := sshRun(ip, "test -x "+vmDisplayPath+" && "+vmDisplayPath)
	if err != nil {
		return
	}
	logf("  display : %s", strings.TrimSpace(mode))
}

// tartRun starts the clone without a window of its own (`--no-graphics`), which is the point of the
// whole arrangement: the guest still has a virtual display to draw on, and the host's keyboard and
// mouse are never taken. `system_profiler SPDisplaysDataType` answers empty in there and the display
// is nonetheless real — do not read that answer as "no screen".
//
// `--no-clipboard` because the guest's own clipboard is what the roads are read against. With the
// sharing on, the host's clipboard is pushed into the guest whenever it changes, so a ⌘V in there
// puts down whatever the person at the host copied last rather than what the run copied a moment
// ago — measured: a path copied in the file panel came out of the paste as another session's text,
// having read back correctly from `pbpaste` just before. Nothing in this arrangement carries
// anything in or out by clipboard, so there is nothing on the other side of the switch.
//
// `tart run` stays in the foreground for as long as the VM lives, so it is detached into its own
// process group: a Ctrl-C on devtool, or devtool simply ending, must not take the VM down with it.
// Its output goes to a log file, named here, because a VM that failed to boot says so there and
// nowhere else.
func tartRun() error {
	log := filepath.Join(os.TempDir(), "amenbo-vm-"+vmCloneName+".log")
	f, err := os.OpenFile(log, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, 0o644)
	if err != nil {
		return fmt.Errorf("open the VM log %s: %w", log, err)
	}
	defer f.Close()

	cmd := exec.Command("tart", "run", vmCloneName, "--no-graphics", "--no-clipboard")
	cmd.Stdin = nil
	cmd.Stdout, cmd.Stderr = f, f
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("tart run %s: %w", vmCloneName, err)
	}
	if err := cmd.Process.Release(); err != nil {
		return fmt.Errorf("detach tart run %s: %w", vmCloneName, err)
	}
	logf("  vm      : booting (log: %s)", log)
	return nil
}

// vmWaitReady waits for the clone to be reachable and to have a GUI session, and answers with its
// IP. Three waits rather than one, because each fails differently and a caller told only "not ready"
// cannot tell a VM that never booted from one whose screen never came up.
//
// Measured on this arrangement: the address answers at ~7s, ssh at ~10s, the console at ~11s. The
// budgets below are that with room, not a guess.
func vmWaitReady() (string, error) {
	ip, err := run("", "tart", "ip", vmCloneName, "--wait", "90")
	if err != nil {
		return "", fmt.Errorf("no address for %s after 90s: %w — the boot log is %s", vmCloneName, err,
			filepath.Join(os.TempDir(), "amenbo-vm-"+vmCloneName+".log"))
	}
	if err := waitFor(60*time.Second, func() bool {
		_, err := sshRun(ip, "true")
		return err == nil
	}); err != nil {
		return "", fmt.Errorf("%s answers at %s but ssh does not, after 60s — is %s enrolled in the golden's authorized_keys? (`devtool vm golden` reports)", vmCloneName, ip, vmKeyPath())
	}
	// The console's owner is what says a GUI session exists. Without it there is no screen to
	// draw on, and the screen tools fail in the shape that is hardest to read: exit 0, nothing
	// delivered.
	if err := waitFor(60*time.Second, func() bool {
		who, err := sshRun(ip, "stat -f %Su /dev/console")
		return err == nil && strings.TrimSpace(who) == vmUser
	}); err != nil {
		return "", fmt.Errorf("%s is up at %s but no GUI session came up after 60s (/dev/console is not %s) — a screen tool would exit 0 and deliver nothing", vmCloneName, ip, vmUser)
	}
	return ip, nil
}

// waitFor polls until `ok` answers true or the budget runs out. Polling rather than waiting on an
// event because none of the three things waited on above raises one.
func waitFor(budget time.Duration, ok func() bool) error {
	deadline := time.Now().Add(budget)
	for {
		if ok() {
			return nil
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("timed out after %s", budget)
		}
		time.Sleep(time.Second)
	}
}

// vmRm throws the clone away — stopped first, because deleting a running VM leaves tart holding a
// process against a disk that is gone. This is the only thing that throws one away: nothing here
// decides on its own that a session is over.
func vmRm() error {
	if err := requireTart(); err != nil {
		return err
	}
	vms, err := tartVMs()
	if err != nil {
		return err
	}
	clone, ok := findVM(vms, vmCloneName)
	if !ok {
		logf("  vm      : no %s to throw away", vmCloneName)
		return nil
	}
	if clone.Running {
		logf("  vm      : stopping %s", vmCloneName)
		if _, err := run("", "tart", "stop", vmCloneName); err != nil {
			return err
		}
	}
	if _, err := run("", "tart", "delete", vmCloneName); err != nil {
		return err
	}
	logf("✓ threw away %s — `devtool vm up` cuts a fresh one from %s", vmCloneName, vmGoldenName)
	return nil
}

// ---------------------------------------------------------------------------
// reach into it, send to it
// ---------------------------------------------------------------------------

// vmKeyPath is the private key the golden was enrolled with.
func vmKeyPath() string {
	home, err := os.UserHomeDir()
	if err != nil {
		return filepath.Join("~", ".ssh", vmKeyName)
	}
	return filepath.Join(home, ".ssh", vmKeyName)
}

// sshOpts are the options every reach into the guest carries.
//
// The host key is deliberately neither checked nor remembered: a clone is cut fresh from the golden
// and carries a new one each time, so a pinned entry would refuse the next clone rather than catch
// anything. What is being reached is a VM on this machine's own private network, raised from an
// image on this machine's own disk; there is no man in that middle.
func sshOpts() []string {
	return []string{
		"-i", vmKeyPath(),
		"-o", "IdentitiesOnly=yes",
		"-o", "StrictHostKeyChecking=no",
		"-o", "UserKnownHostsFile=/dev/null",
		"-o", "LogLevel=ERROR",
		"-o", "ConnectTimeout=5",
	}
}

// sshArgs builds an `ssh` argument list reaching the guest at ip.
func sshArgs(ip string, command ...string) []string {
	args := append(sshOpts(), vmUser+"@"+ip)
	return append(args, command...)
}

// sshRun runs one command in the guest and returns its trimmed stdout — the shape used for the
// short questions asked of it here (the console's owner, the OS version). A command a caller asked
// for goes through vmExec instead, which keeps its stdio and its exit code.
func sshRun(ip string, command ...string) (string, error) {
	return run("", "ssh", sshArgs(ip, command...)...)
}

// vmIP answers with the running clone's address, and refuses when there is none — an empty answer
// would be composed into an `admin@` that reaches nothing and fails much later.
func vmIP() (string, error) {
	if err := requireTart(); err != nil {
		return "", err
	}
	vms, err := tartVMs()
	if err != nil {
		return "", err
	}
	if clone, ok := findVM(vms, vmCloneName); !ok || !clone.Running {
		return "", fmt.Errorf("%s is not running — `devtool vm up` raises it", vmCloneName)
	}
	return run("", "tart", "ip", vmCloneName)
}

// vmExec runs a command in the guest with this process's own stdio and ends the way it ended. The
// exit code has to come through: a caller driving the guest from a script reads it, and a step that
// failed in there must not read as green out here.
func vmExec(argv []string) (int, error) {
	ip, err := vmIP()
	if err != nil {
		return 0, err
	}
	return runThrough("", nil, "ssh", sshArgs(ip, argv...)...)
}

// vmPushArgs splits `devtool vm push <local…> <remote>` into its two halves. The last word is the
// destination, the way `cp` and `scp` read one — with at least one source before it, since a lone
// word is a destination with nothing to put there rather than a file to send somewhere unnamed.
func vmPushArgs(args []string) (locals []string, remote string, err error) {
	if len(args) < 2 {
		return nil, "", fmt.Errorf("vm push takes one or more local paths and a remote destination, e.g. `devtool vm push Amenbo.app /Users/%s/`", vmUser)
	}
	return args[:len(args)-1], args[len(args)-1], nil
}

// vmPush sends files into the guest. Recursive, because what is sent is usually a `.app` — a
// directory to everything but the Finder.
func vmPush(locals []string, remote string) error {
	ip, err := vmIP()
	if err != nil {
		return err
	}
	for _, l := range locals {
		if _, err := os.Stat(l); err != nil {
			return fmt.Errorf("nothing to send at %s: %w", l, err)
		}
	}
	args := append(sshOpts(), "-r")
	args = append(args, locals...)
	args = append(args, vmUser+"@"+ip+":"+remote)
	if _, err := run("", "scp", args...); err != nil {
		return err
	}
	logf("✓ sent %s → %s:%s", strings.Join(locals, " "), vmCloneName, remote)
	return nil
}

// vmSendScreen compiles this checkout's screen tool on the host and puts the binary in the guest,
// printing its remote path on stdout.
//
// Compiled here and sent, rather than baked into the golden or run as a script in there: the golden
// then holds no copy of a tool this tree keeps changing (12s to build, 0.09s to send — measured),
// and the guest needs no Swift toolchain. It is also why the golden can be replaced without
// anything having to be re-baked into it.
func vmSendScreen() error {
	ip, err := vmIP()
	if err != nil {
		return err
	}
	src := screenTool()
	if _, err := os.Stat(src); err != nil {
		return fmt.Errorf("no screen tool at %s: %w", src, err)
	}
	out, err := os.MkdirTemp("", "amenbo-screen-")
	if err != nil {
		return err
	}
	defer os.RemoveAll(out)
	bin := filepath.Join(out, "screen")

	logf("  screen  : compiling %s", src)
	if _, err := run("", "swiftc", "-O", "-o", bin, src); err != nil {
		return fmt.Errorf("compiling the screen tool: %w", err)
	}
	args := append(sshOpts(), bin, vmUser+"@"+ip+":"+vmScreenPath)
	if _, err := run("", "scp", args...); err != nil {
		return err
	}
	logf("  screen  : in %s at %s", vmCloneName, vmScreenPath)
	fmt.Printf("%s\n", vmScreenPath)
	return nil
}

// ---------------------------------------------------------------------------
// the golden, and the two versions drifting apart
// ---------------------------------------------------------------------------

// vmGolden reports on the golden, and with `refresh` takes the base image again and cuts the golden
// from it anew.
//
// Enrolling the key is left to a person, and named rather than done: it takes the image's password,
// which is a credential to type, not one to hold here. A golden with no key reachable is the one
// state that stops everything downstream, so it is reported every time rather than found out later
// through a 60-second ssh wait.
func vmGolden(refresh bool) error {
	if err := requireTart(); err != nil {
		return err
	}
	if refresh {
		// The clone is checked before the pull, not after: the pull is the expensive half
		// (tens of GB on a base that moved), and refusing afterwards would have spent it
		// for nothing.
		vms, err := tartVMs()
		if err != nil {
			return err
		}
		if clone, ok := findVM(vms, vmCloneName); ok && clone.Running {
			return fmt.Errorf("%s is running off the old golden — `devtool vm rm` first, so the next clone is cut from the new one", vmCloneName)
		}
		logf("  golden  : pulling %s", vmBase)
		if _, err := run("", "tart", "pull", vmBase); err != nil {
			return err
		}
		if _, ok := findVM(vms, vmGoldenName); ok {
			logf("  golden  : replacing %s", vmGoldenName)
			if _, err := run("", "tart", "delete", vmGoldenName); err != nil {
				return err
			}
		}
		if _, err := run("", "tart", "clone", vmBase, vmGoldenName); err != nil {
			return err
		}
		logf("✓ %s cut from %s", vmGoldenName, vmBase)
		logf("  golden  : it carries no key yet. Start it (`tart run %s --no-graphics`), then", vmGoldenName)
		logf("            `ssh-copy-id -i %s.pub %s@$(tart ip %s)` (the image's password), then stop it.", vmKeyPath(), vmUser, vmGoldenName)
		logf("            The golden is never started for verification — only to be prepared.")
		return nil
	}

	vms, err := tartVMs()
	if err != nil {
		return err
	}
	logf("  base    : %s %s", vmBase, present(hasImage(vms, vmBase)))
	golden, ok := findVM(vms, vmGoldenName)
	logf("  golden  : %s %s", vmGoldenName, present(ok))
	if ok && golden.Running {
		logf("            it is RUNNING — the golden is meant to stay stopped, or it stops being a known ground")
	}
	if _, err := os.Stat(vmKeyPath()); err != nil {
		logf("            no key at %s — the golden cannot be reached without one", vmKeyPath())
	}
	if !ok {
		logf("  `devtool vm golden --refresh` takes the base image and cuts the golden from it")
	}
	return nil
}

func present(ok bool) string {
	if ok {
		return "— present"
	}
	return "— NOT there"
}

// vmStatus reports what is up: the golden, the clone, and where host and guest stand against each
// other.
func vmStatus() error {
	if err := vmGolden(false); err != nil {
		return err
	}
	vms, err := tartVMs()
	if err != nil {
		return err
	}
	clone, ok := findVM(vms, vmCloneName)
	if !ok {
		logf("  clone   : no %s — `devtool vm up` cuts and raises one", vmCloneName)
		return nil
	}
	if !clone.Running {
		logf("  clone   : %s is there, stopped — `devtool vm up` starts it", vmCloneName)
		return nil
	}
	ip, err := run("", "tart", "ip", vmCloneName)
	if err != nil {
		logf("  clone   : %s is running, with no address yet", vmCloneName)
		return nil
	}
	logf("  clone   : %s running at %s", vmCloneName, ip)
	reportDisplay(ip)
	reportVersionDrift(ip)
	return nil
}

// reportVersionDrift says whether host and guest have drifted apart, and never stops anything. What
// the guest is for is standing in for this machine, and a guest several releases away stops standing
// in for it; what it costs to be wrong about that, though, is a rebuilt golden, so it is not a thing
// to refuse to run over.
//
// Being unable to ask is not a drift. A guest that will not answer has already failed louder
// somewhere else, and a second complaint here would only muddy it.
func reportVersionDrift(ip string) {
	guest, err := sshRun(ip, "sw_vers -productVersion")
	if err != nil {
		return
	}
	host, err := run("", "sw_vers", "-productVersion")
	if err != nil {
		return
	}
	guest, host = strings.TrimSpace(guest), strings.TrimSpace(host)
	if !versionsDrifted(host, guest) {
		logf("  macOS   : host %s / guest %s", host, guest)
		return
	}
	logf("  macOS   : host %s / guest %s — DRIFTED. What is verified in there is no longer this machine;", host, guest)
	logf("            `devtool vm golden --refresh` takes a newer base. Nothing is stopped over it.")
}

// versionsDrifted compares two `sw_vers -productVersion` strings on major and minor. The patch is
// left out on purpose: a guest one security update behind is the ordinary state of an image that is
// republished weekly, and reporting it every single time is how a warning stops being read.
func versionsDrifted(host, guest string) bool {
	return majorMinor(host) != majorMinor(guest)
}

// majorMinor keeps the first two components of a version, so `26.5.2` and `26.5` compare equal.
func majorMinor(v string) string {
	parts := strings.Split(strings.TrimSpace(v), ".")
	if len(parts) > 2 {
		parts = parts[:2]
	}
	return strings.Join(parts, ".")
}
