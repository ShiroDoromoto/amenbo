#!/usr/bin/env bash
# win-ps.sh — run a PowerShell script on the Windows machine, from here.
#
# This is the plain door: the script runs where `ssh win` lands, which is
# session 0 and elevated. That is the right place for anything that only reads or
# writes files, asks about processes, or installs a build — and the wrong place
# for anything with a window, since session 0 draws no desktop. For that, use
# win-session1.sh (a script) or win-drive.sh (a sequence of presses).
#
# Elevation has one more edge worth knowing before reaching for this door: an
# elevated process will not traverse a junction a non-administrator created, so
# tools installed under scoop's `current` junction vanish from PATH here while
# being perfectly present in session 1 — measured, not theoretical.
#
# Usage:
#   scripts/windows/win-ps.sh [script.ps1]     # or the script on stdin
#   scripts/windows/win-ps.sh - <<'PS'
#   Get-Content "$env:USERPROFILE\amenbo-drive\amenbo-session1.log"
#   PS
#
# Feed it a here-document, not `echo`: zsh's builtin echo expands backslash
# escapes, so a Windows path loses its separators (`\a` becomes a bell) before this
# script ever sees it.
set -euo pipefail

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/windows/win-lib.sh
. scripts/windows/win-lib.sh

case "${1:-}" in
    -h | --help)
        win_usage "$0"
        exit 0
        ;;
esac

win_ps "${1:--}"
