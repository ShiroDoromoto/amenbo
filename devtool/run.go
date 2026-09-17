package main

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"strings"
)

// logf writes diagnostics to stderr so stdout stays reserved for eval-able output.
func logf(format string, a ...any) {
	fmt.Fprintf(os.Stderr, format+"\n", a...)
}

// paneMarks is what a pane of Amenbo's talk window puts on the terminal it opens, and hands down to
// everything started in it (`crates/amenbo-core/src/session.rs`). The names are spelled here rather
// than read out of Amenbo's crates: devtool is a Go module that shares nothing with them, and a
// build that stopped answering to these names would have stopped answering to what a real pane sets.
var paneMarks = [...]string{"AMENBO_PANE", "AMENBO_PANE_RESUME", "AMENBO_SESSION", "AMENBO_SESSION_DIR"}

// environ is this process's environment with the pane marks taken off — the base every child devtool
// starts is given.
//
// The amenbo a child runs reads those marks as proof that it is inside the talk window, and answers
// as one: what it did against a throwaway store is announced to the pane somebody is working in, the
// bar under that pane counts it, and pressing the count opens the same number in the production store.
// `devgui cli` seeds a screen by repeating `task add`, so it happens on every call. Nothing devtool
// starts needs to be told which pane devtool was typed in; a child that wants one is handed it by
// name, never by inheritance.
func environ() []string {
	env := os.Environ()
	kept := env[:0]
	for _, e := range env {
		if !isPaneMark(e) {
			kept = append(kept, e)
		}
	}
	return kept
}

// isPaneMark reports whether an `os.Environ()` entry is one of the pane marks.
func isPaneMark(entry string) bool {
	name, _, found := strings.Cut(entry, "=")
	if !found {
		return false
	}
	for _, mark := range paneMarks {
		if name == mark {
			return true
		}
	}
	return false
}

// runThrough executes a command in dir with this process's own stdio, adding `extraEnv` to the
// environment, and returns its exit code. Unlike run it captures nothing: what the command prints is
// what the caller sees, in the order it printed it, which is the whole point when the command is one
// the caller asked to be passed through (`devgui cli`). An exit code comes back as a code, not an
// error — a non-zero amenbo is an answer, and only a command that could not be run at all is a
// failure of devtool's.
func runThrough(dir string, extraEnv []string, name string, args ...string) (int, error) {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	cmd.Env = append(environ(), extraEnv...)
	cmd.Stdin, cmd.Stdout, cmd.Stderr = os.Stdin, os.Stdout, os.Stderr
	if err := cmd.Run(); err != nil {
		var exit *exec.ExitError
		if errors.As(err, &exit) {
			return exit.ExitCode(), nil
		}
		return 0, fmt.Errorf("%s: %w", name, err)
	}
	return 0, nil
}

// runFed is run with `input` handed to the command on its stdin.
//
// It exists for the one thing that must not be spelled as an argument: a credential read out of a
// keychain and passed as a word would stand in this machine's process list for as long as the
// command took, and in the far machine's too where the command is an `ssh`.
func runFed(dir, input, name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	cmd.Env = environ()
	cmd.Stdin = strings.NewReader(input)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		msg := strings.TrimSpace(stderr.String())
		if msg == "" {
			msg = strings.TrimSpace(stdout.String())
		}
		return "", fmt.Errorf("%s: %v: %s", name, err, msg)
	}
	return strings.TrimSpace(stdout.String()), nil
}

// run executes a command in dir and returns its trimmed stdout. On failure the
// error carries the captured stderr so callers can surface the real cause.
func run(dir, name string, args ...string) (string, error) {
	return runEnv(dir, nil, name, args...)
}

// runEnv is run with `extraEnv` added to the environment — for a command that has to be pointed at
// something other than this process's own world, such as an amenbo told which store to open.
func runEnv(dir string, extraEnv []string, name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	cmd.Env = append(environ(), extraEnv...)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		msg := strings.TrimSpace(stderr.String())
		if msg == "" {
			msg = strings.TrimSpace(stdout.String())
		}
		return "", fmt.Errorf("%s %s: %v: %s", name, strings.Join(args, " "), err, msg)
	}
	return strings.TrimSpace(stdout.String()), nil
}
