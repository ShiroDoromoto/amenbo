#!/usr/bin/env bash
# close-release.sh — a script step: close a value of the release axis once every task under it is done.
#
# It takes no arguments. Its one input (VALUE_INPUT below) is the value, by name. When no task under it
# is left undone, it closes the value (`dimension value-close`) and leaves by DONE_EXIT; its report names
# the value it closed and the first value of the axis still open after it, or says there is none. It
# files no next value. When tasks are left undone, it closes nothing and leaves by the error way out;
# its report names each of them, by ref and title.
#
# A filter splits on whitespace before it reads a value, so it names the value by its slug, looked up by
# name on the axis.
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or
# write — no value, a name that is not a value, a command that failed — ends it with exit code 1, which
# the step leaves through its error way out as well.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, whose
# project the axis and tasks are read from.
set -euo pipefail

# The names the board writes in Japanese: the input, and the ways out: done, and the error way out,
# which output.json names as `*`.
VALUE_INPUT="分類"
DONE_EXIT="完了"
ERROR_EXIT="*"

cd "$(dirname "$0")/../.."

die() { echo "✗ $*" >&2; exit 1; }

[ -n "${AMENBO_INPUT:-}" ] && [ -n "${AMENBO_OUTPUT:-}" ] ||
    die "run this as a script step: AMENBO_INPUT and AMENBO_OUTPUT are not set"

value=$(jq -r --arg i "$VALUE_INPUT" '.ins[$i] // empty' "$AMENBO_INPUT") || die "could not read $AMENBO_INPUT"
[ -n "$value" ] || die "no value: wire the release axis's value into this step's $VALUE_INPUT"

amenbo_ai() { amenbo "$@" --actor ai; }

slug=$(amenbo_ai dimension show リリース --closed --json |
    jq -r --arg v "$value" '[.values[] | select(.name == $v) | .slug][0] // empty') ||
    die "could not read the release axis"
[ -n "$slug" ] || die "\"$value\" is not a value of the release axis"

undone=$(amenbo_ai task list --filter "dim:リリース=$slug done:false" --json) ||
    die "could not list the undone tasks of \"$value\""

if [ "$(jq -r '.tasks | length' <<< "$undone")" != 0 ]; then
    report="分類「${value}」には done でないタスクが残っているので、閉じていません。"$'\n'
    report+=$(jq -r '.tasks[] | "- \(.ref)　\(.title)"' <<< "$undone")
    jq -n --arg exit "$ERROR_EXIT" --arg report "$report" \
        '{version: 1, exit: $exit, outs: {}, report: $report}' > "$AMENBO_OUTPUT"
    exit 0
fi

amenbo_ai dimension value-close リリース "$slug" --yes >&2 || die "could not close \"$value\""

next=$(amenbo_ai dimension show リリース --json | jq -r '[.values[] | select(.closed | not) | .name][0] // empty') ||
    die "could not read the release axis"

report="分類「${value}」を閉じました。次に開いている値：${next:-なし}"
jq -n --arg exit "$DONE_EXIT" --arg report "$report" \
    '{version: 1, exit: $exit, outs: {}, report: $report}' > "$AMENBO_OUTPUT"
