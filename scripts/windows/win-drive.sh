#!/usr/bin/env bash
# win-drive.sh — press the Windows desktop from here, and bring the screenshots back.
#
# Takes one plan (a JSON array of actions — see drive.ps1 for the ops), runs it in
# the interactive session on the Windows machine, prints the transcript, and copies
# every PNG the run left behind into a local directory.
#
# ONE SEQUENCE IS ONE PLAN. Between two runs the foreground changes, and what was on
# screen does not survive it: a webview's context menu closes on `blur`, and a dialog
# with no owner window vanishes silently. Open the menu, press the item, and shoot
# the result in a single plan — the reason the plan is a file and not a command.
#
# Coordinates are desktop coordinates, the same ones a `shot` is taken in, so the
# way to write a plan is to shoot first, read the picture, and aim at what is in it.
#
# Usage:
#   scripts/windows/win-drive.sh <plan.json> [out-dir] [timeout-seconds]
#
# Environment (see win-lib.sh): AMENBO_WIN_HOST, AMENBO_WIN_DIR.
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

plan="$1"
out_dir="${2:-dist/win-drive}"
timeout="${3:-120}"

[ -f "$plan" ] || { echo "✗ no such plan: $plan" >&2; exit 1; }
mkdir -p "$out_dir"

dir=$(win_dir)
plan_name=$(basename "$plan")

# Clear the shots from the last run first, so what comes back is this run's and not
# a stale picture that would read as a passing verdict.
printf '%s\n' \
    "New-Item -ItemType Directory -Force -Path '$dir' | Out-Null" \
    "Remove-Item '$dir\\*.png' -Force -ErrorAction SilentlyContinue" | win_ps - >/dev/null

win_push scripts/windows/drive.ps1 "$plan"

# The plan is carried out by a one-line script, because the scheduler runs a path
# rather than a command, and session1.ps1 hands it exactly one.
# The name has to end in .ps1 (PowerShell refuses an unknown extension), and BSD
# mktemp ignores a template's suffix — hence a temporary directory, not a template.
runner_dir=$(mktemp -d -t amenbo-win-drive)
# shellcheck disable=SC2064  # $runner_dir is fixed now; expanding it later would be the bug
trap "rm -rf '$runner_dir'" EXIT
runner="$runner_dir/drive-run.ps1"
printf '%s\n' "& '$dir\\drive.ps1' -Plan '$dir\\$plan_name' -OutDir '$dir'" > "$runner"

win_session1 "$runner" "$timeout"

# Fetched by name rather than by a remote glob: the Windows ssh server's shell does
# not expand one, so a `*.png` would arrive as a literal filename and match nothing.
shots=$(printf '%s' \
    "Get-ChildItem '$dir\\*.png' -ErrorAction SilentlyContinue | ForEach-Object { \$_.Name }" |
    win_ps - | tr -d '\r')

[ -n "$shots" ] || { echo "→ no screenshots in $dir"; exit 0; }

while IFS= read -r shot; do
    [ -n "$shot" ] || continue
    scp -q -o LogLevel=ERROR "$(win_host):$(win_dir_scp)/$shot" "$out_dir/$shot"
    echo "→ $out_dir/$shot"
    # The desktop is wider than anything that reads the picture afterwards wants to
    # take in one piece, so a scaled copy is left beside the full-size one.
    if command -v sips > /dev/null 2>&1; then
        sips -Z 1600 "$out_dir/$shot" --out "$out_dir/${shot%.png}-small.png" > /dev/null
    fi
done <<< "$shots"
