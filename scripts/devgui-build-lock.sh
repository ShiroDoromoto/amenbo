#!/usr/bin/env bash
# devgui-build-lock.sh — run one command under the build lock that belongs to a task's dev GUI.
#
# Why a lock at all:
#   `make install-gui-dev-vm AMB-T-ID=<id>` builds in that task's worktree, so two runs of the same
#   id reach for one cargo `target` directory. cargo does not refuse there — it prints "Blocking
#   waiting for file lock on build directory" and waits — so a build started a second time looks
#   alive from the outside while neither side moves.
#
# Why the id is the unit:
#   Another task is another worktree and another `target`, so those builds are not in each other's
#   way. A lock on the whole machine would line up builds that never collided.
#
# Why it does not wait:
#   Waiting is what the failure looked like. The second caller is the first one asked for twice, so
#   the useful answer is to name the run that is already going and stop.
#
# Why lockf on a file descriptor, and not shlock:
#   The builds that collided had been stopped part-way, so a lock that outlives its holder would
#   hold off every later run. The kernel drops a flock(2) when the last descriptor on it closes, so
#   a build that is killed leaves nothing behind. shlock, which macOS itself calls deprecated, reads
#   the holder from a pid written in the file and then still refuses to break a lock whose mtime it
#   finds too recent — which is exactly the case here.
#   The descriptor form of lockf locks a descriptor this script already holds, so the lock lives as
#   long as this script and whatever it started, rather than as long as lockf itself.
#
# Usage: scripts/devgui-build-lock.sh <task-id> <command> [args...]
set -euo pipefail

if [ $# -lt 2 ]; then
    echo "usage: devgui-build-lock.sh <task-id> <command> [args...]" >&2
    exit 2
fi

id=$1
shift

command -v lockf >/dev/null 2>&1 || {
    echo "✗ lockf is required (it ships with macOS, which is the only place this build has a VM to reach)" >&2
    exit 1
}

# ~/Library/Caches rather than TMPDIR: a build runs for minutes, and TMPDIR is swept — a lock file
# swept out from under a running build is a lock the next caller does not see.
lock_dir=$HOME/Library/Caches
mkdir -p "$lock_dir"
lock=$lock_dir/amenbo-devgui-$id.lock

# Appended to rather than truncated: the pid inside is what a refused caller reports, and opening
# for truncation would wipe it before we know whether the lock is even ours to take.
exec 9>>"$lock"
if ! lockf -s -t 0 9; then
    holder=$(head -n 1 "$lock" 2>/dev/null || true)
    echo "✗ a build for AMB-T-ID=$id is already running (pid ${holder:-unknown}) — this one stops here" >&2
    echo "  wait for it, or stop it and run again; builds for other task ids are not held up by this" >&2
    exit 1
fi
printf '%s\n' "$$" >"$lock"

"$@"
