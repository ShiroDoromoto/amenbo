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

// runThrough executes a command in dir with this process's own stdio, adding `extraEnv` to the
// environment, and returns its exit code. Unlike run it captures nothing: what the command prints is
// what the caller sees, in the order it printed it, which is the whole point when the command is one
// the caller asked to be passed through (`task cli`). An exit code comes back as a code, not an
// error — a non-zero amenbo is an answer, and only a command that could not be run at all is a
// failure of devtool's.
func runThrough(dir string, extraEnv []string, name string, args ...string) (int, error) {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), extraEnv...)
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

// run executes a command in dir and returns its trimmed stdout. On failure the
// error carries the captured stderr so callers can surface the real cause.
func run(dir, name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
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
