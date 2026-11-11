#!/usr/bin/env bash
# check-release-value.sh — a script step: take the value of the release axis the run's task carries as
# the one to ship, and see that nothing of one product is left undone under it.
#
# Its first argument is the product's value, by name. Its one input, which may be left unwired, is the
# run's task (TASK_INPUT below), wired from the task a take_task step hands on. Where it is not wired,
# it is the task the run took last, read from the run that started this step (AMENBO_RUN).
#
# The value to ship is the run's task's value of the release axis. It lists the tasks under that value
# and the product that are not done, leaving out the run's task itself. When there are none, it leaves by
# DONE_EXIT, handing the value on by name (VALUE_OUTPUT), and its report gives the value and how many of
# the product's tasks under it are done. When some are left, it writes them — ref, title and status — as
# a Markdown table (LEFT_FILE, beside output.json), hands that on (LEFT_OUTPUT) and leaves by LEFT_EXIT.
#
# A filter splits on whitespace before it reads a value, so it names each value by its slug, looked up by
# name on the axis.
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or
# write — no product, a run's task with no value of the release axis, a name that is not a value, a
# command that failed — ends it with exit code 1, which the step leaves through its error way out.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, whose
# project the axes and tasks are read from.
set -euo pipefail

# The names the board writes in Japanese: the input, the outputs and the file handed on, and the ways out.
TASK_INPUT="タスク"
VALUE_OUTPUT="分類"
LEFT_OUTPUT="残り"
LEFT_FILE="残り.md"
DONE_EXIT="完了"
LEFT_EXIT="中身が残っている"

cd "$(dirname "$0")/../.."

die() { echo "✗ $*" >&2; exit 1; }

[ -n "${AMENBO_INPUT:-}" ] && [ -n "${AMENBO_OUTPUT:-}" ] ||
    die "run this as a script step: AMENBO_INPUT and AMENBO_OUTPUT are not set"

product="${1:-}"
[ -n "$product" ] || die "no product: pass the product's value as the first argument"

task=$(jq -r --arg i "$TASK_INPUT" '.ins[$i] // empty' "$AMENBO_INPUT") || die "could not read $AMENBO_INPUT"

amenbo_ai() { amenbo "$@" --actor ai; }

if [ -z "$task" ]; then
    [ -n "${AMENBO_RUN:-}" ] || die "no task: wire one into $TASK_INPUT, or run this as a step of a run"
    id=$(amenbo_ai automation run-show "$AMENBO_RUN" --json | jq -r '.tasks[-1].task_id // empty') ||
        die "could not read run $AMENBO_RUN"
    [ -n "$id" ] || die "run $AMENBO_RUN has taken no task"
    task="AMB-T-$id"
fi

shown=$(amenbo_ai task show "$task" --json) || die "could not read $task"
own_ref=$(jq -r '.ref' <<< "$shown")
value=$(jq -r '[.dimensions[] | select(.dimension == "リリース") | .value][0] // empty' <<< "$shown")
[ -n "$value" ] || die "$task carries no value of the release axis"

slug_of() {
    amenbo_ai dimension show "$1" --closed --json |
        jq -r --arg v "$2" '[.values[] | select(.name == $v) | .slug][0] // empty'
}

value_slug=$(slug_of リリース "$value") || die "could not read the release axis"
[ -n "$value_slug" ] || die "\"$value\" is not a value of the release axis"
product_slug=$(slug_of プロダクト "$product") || die "could not read the product axis"
[ -n "$product_slug" ] || die "\"$product\" is not a value of the product axis"

undone=$(amenbo_ai task list --filter "dim:リリース=$value_slug dim:プロダクト=$product_slug done:false" --json |
    jq -c --arg own "$own_ref" '[.tasks[] | select(.ref != $own)]') ||
    die "could not list the undone tasks of \"$value\""

if [ "$(jq -r 'length' <<< "$undone")" = 0 ]; then
    done_count=$(amenbo_ai task list --filter "dim:リリース=$value_slug dim:プロダクト=$product_slug done:true" --json |
        jq -r '.total_matched') || die "could not list the done tasks of \"$value\""
    report="出す分類：${value}（${product} の done のタスク：${done_count}件）"
    jq -n --arg exit "$DONE_EXIT" --arg out "$VALUE_OUTPUT" --arg value "$value" --arg report "$report" \
        '{version: 1, exit: $exit, outs: {($out): $value}, report: $report}' > "$AMENBO_OUTPUT"
    exit 0
fi

left="$(dirname "$AMENBO_OUTPUT")/$LEFT_FILE"
{
    echo "分類「${value}」に残っている ${product} のタスク"
    echo
    echo "| 番号 | タイトル | 状態 |"
    echo "|---|---|---|"
    jq -r '.[] | "| \(.ref) | \(.title | gsub("\\|"; "\\|")) | \(.status) |"' <<< "$undone"
} > "$left" || die "could not write $left"

report="分類「${value}」に ${product} のタスクが $(jq -r 'length' <<< "$undone")件残っています。"$'\n'
report+=$(jq -r '.[] | "- \(.ref)　\(.title)（\(.status)）"' <<< "$undone")
jq -n --arg exit "$LEFT_EXIT" --arg out "$LEFT_OUTPUT" --arg file "$LEFT_FILE" --arg report "$report" \
    '{version: 1, exit: $exit, outs: {($out): $file}, report: $report}' > "$AMENBO_OUTPUT"
