#!/usr/bin/env bash
# split-by-dim.sh — a script step: leave by the value the task taken carries on one axis.
#
# It stands in for the built-in that split a run by an axis, which v40.0.0 took out. Until that
# built-in is back, an automation that goes one way for each kind of work places this instead, with
# one way out per value it sends somewhere, plus the way out for no value (NO_VALUE below).
#
# Its first argument is the axis, by name; the rest are the values it may leave by, by name. Its one
# input (TASK_INPUT below) is a task_take input wired from take_task's output of the same name, which
# reaches it as the task's ref. It reads that task's value on the axis and leaves by the way out of
# that name. A task with no value on the axis, or one whose value is not among the arguments — a value
# added to the axis after the automation was built — leaves by NO_VALUE, so it is never stranded.
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or
# write — no axis, no task, a command that failed — ends it with exit code 1, which the step leaves
# through its error way out.
set -euo pipefail

# The names the board writes in Japanese: the input take_task hands the task on, and the way out
# for a task with no value on the axis.
TASK_INPUT="タスク"
NO_VALUE="分類なし"

cd "$(dirname "$0")/../.."

die() { echo "✗ $*" >&2; exit 1; }

[ -n "${AMENBO_INPUT:-}" ] && [ -n "${AMENBO_OUTPUT:-}" ] ||
    die "スクリプトのステップとして動かしてください：AMENBO_INPUT と AMENBO_OUTPUT が設定されていません"

axis="${1:-}"
[ -n "$axis" ] || die "軸がありません：1つ目の引数に軸の名前を渡してください"
shift
values=("$@")

task=$(jq -r --arg i "$TASK_INPUT" '.ins[$i] // empty' "$AMENBO_INPUT") || die "$AMENBO_INPUT を読めませんでした"
[ -n "$task" ] || die "タスクがありません：take_task の「${TASK_INPUT}」を、このステップの「${TASK_INPUT}」につないでください"

value=$(amenbo task show "$task" --json --actor ai |
    jq -r --arg a "$axis" '[.dimensions[] | select(.dimension == $a) | .value][0] // empty') ||
    die "$task を読めませんでした"

exit_name="$NO_VALUE"
for one in "${values[@]}"; do
    if [ "$one" = "$value" ]; then exit_name="$value"; break; fi
done

if [ -n "$value" ]; then
    report="${task} の「${axis}」は「${value}」です"
else
    report="${task} には「${axis}」の値がありません"
fi

jq -n --arg exit "$exit_name" --arg report "$report" \
    '{version: 1, exit: $exit, outs: {}, report: $report}' > "$AMENBO_OUTPUT"
