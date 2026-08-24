#!/usr/bin/env bash
# win-session1.sh — run a PowerShell script on the Windows machine's interactive
# desktop, from here.
#
# The door for anything with a window. `ssh win` lands in session 0, elevated,
# where no desktop is drawn; this pushes the script over and has session1.ps1 hand
# it to the Task Scheduler, which starts it in the logged-on user's own session,
# not elevated. What the script wrote comes back on stdout.
#
# The script runs on the far machine, so every path inside it is a Windows path.
# AMENBO_WIN_DIR (default <%USERPROFILE%>\amenbo-drive) is where it is put, and
# where anything it writes beside itself can be picked up from.
#
# Usage:
#   scripts/windows/win-session1.sh <script.ps1> [timeout-seconds]
#   scripts/windows/win-session1.sh - [timeout-seconds]   # script on stdin
set -euo pipefail

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/windows/win-lib.sh
. scripts/windows/win-lib.sh

case "${1:-}" in
    "" | -h | --help)
        win_usage "$0"
        [ -n "${1:-}" ] || exit 2
        exit 0
        ;;
esac

script="$1"
timeout="${2:-120}"

# A script on stdin still has to reach the far machine as a file, since the task
# scheduler runs a path, not a stream.
# The name has to end in .ps1 — PowerShell refuses to run a script whose extension
# it does not know, and BSD mktemp ignores a template's suffix, so the name is built
# inside a temporary directory instead of out of a template.
if [ "$script" = "-" ]; then
    tmp_dir=$(mktemp -d -t amenbo-win-session1)
    # shellcheck disable=SC2064  # $tmp_dir is fixed now; expanding it later would be the bug
    trap "rm -rf '$tmp_dir'" EXIT
    script="$tmp_dir/session1-script.ps1"
    cat > "$script"
fi

[ -f "$script" ] || { echo "✗ no such script: $script" >&2; exit 1; }

win_session1 "$script" "$timeout"
