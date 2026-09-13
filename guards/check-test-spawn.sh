#!/usr/bin/env bash
# check-test-spawn.sh — keep every child a test starts going through one door.
#
# A command takes its parent's environment. A test started from inside a pane of Amenbo's own
# terminal therefore hands the pane's `AMENBO_SESSION` and `AMENBO_SESSION_DIR` to whatever it runs,
# and the binary under test reads those two as proof that it is inside the window. It then answers as
# something inside one: the road that reads "half an environment is refused" is handed a whole one and
# goes red, and the statement it made up lands in the drop box of the pane the tests were typed in —
# renaming that pane after a word a test invented, and telling the window an agent has been briefed
# when none has.
#
# **Nothing else notices.** CI runs outside a pane, so every one of those is green there: the whole of
# it lands on the one person who ran the tests from the pane they were working in, on a change that
# has nothing to do with it.
#
# `amenbo_scratch::command` is the door — the throwaway environment beside the throwaway directory —
# and it drops what the parent had by prefix rather than by name, so a variable added later goes with
# the same line. A test that reaches for `Command::new` directly is a test outside that, and the fault
# it lets back in is invisible to everything that would otherwise catch it. So the door is held in the
# source, where it can be seen at all.
#
# The line is drawn at `crates/*/tests/`: that is where a test starts the binary under test, and it is
# the only place `CARGO_BIN_EXE_…` resolves at all.
#
# Usage: guards/check-test-spawn.sh   (no args)
# Exit codes: 0 = every test run goes through the door, 1 = one goes around it.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

door='amenbo_scratch::command('

# The sanity check first, the way every guard here makes one: a pattern that matches nothing would
# pass forever, and passing is exactly what this would do on the day the door was renamed.
if ! git grep -qF "$door" -- ':(glob)crates/*/tests/**'; then
    echo "✗ test spawn: nothing under crates/*/tests calls $door — either the door was renamed," >&2
    echo "  or the tests stopped starting anything. Fix the guard rather than deleting it." >&2
    exit 1
fi

strays=$(git grep -nF 'Command::new(' -- ':(glob)crates/*/tests/**' || true)
if [ -n "$strays" ]; then
    echo "✗ test spawn: a test starts a child without going through $door." >&2
    echo "$strays" >&2
    echo "  Use amenbo_scratch::command(<exe>) instead: it drops the AMENBO_* the parent had, so a" >&2
    echo "  run made from inside an Amenbo pane does not hand that pane's session to the child." >&2
    exit 1
fi

echo "✓ test spawn: every run a test makes starts from an environment the test decided"
