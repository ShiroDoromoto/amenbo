#!/usr/bin/env bash
# win-clean.sh — take the working dir back off the Windows machine.
#
# The other scripts here leave three kinds of thing on the far side: the .ps1 files
# they push, the plan and log of the last run, and its screenshots. None of it is
# installed and none of it is needed between runs, so this removes the lot — along
# with any scheduled task an interrupted run did not get to unregister.
#
# Usage: scripts/windows/win-clean.sh
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

win_clean_remote
