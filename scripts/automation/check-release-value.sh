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
    die "スクリプトのステップとして動かしてください：AMENBO_INPUT と AMENBO_OUTPUT が設定されていません"

product="${1:-}"
[ -n "$product" ] || die "プロダクトがありません：1つ目の引数にプロダクトの値を渡してください"

task=$(jq -r --arg i "$TASK_INPUT" '.ins[$i] // empty' "$AMENBO_INPUT") || die "$AMENBO_INPUT を読めませんでした"

amenbo_ai() { amenbo "$@" --actor ai; }

if [ -z "$task" ]; then
    [ -n "${AMENBO_RUN:-}" ] || die "タスクがありません：「${TASK_INPUT}」にタスクをつなぐか、run のステップとして動かしてください"
    id=$(amenbo_ai automation run-show "$AMENBO_RUN" --json | jq -r '.tasks[-1].task_id // empty') ||
        die "run $AMENBO_RUN を読めませんでした"
    [ -n "$id" ] || die "run $AMENBO_RUN はタスクを取っていません"
    task="AMB-T-$id"
fi

shown=$(amenbo_ai task show "$task" --json) || die "$task を読めませんでした"
own_ref=$(jq -r '.ref' <<< "$shown")
value=$(jq -r '[.dimensions[] | select(.dimension == "リリース") | .value][0] // empty' <<< "$shown")
[ -n "$value" ] || die "$task に軸「リリース」の値がありません"

slug_of() {
    amenbo_ai dimension show "$1" --closed --json |
        jq -r --arg v "$2" '[.values[] | select(.name == $v) | .slug][0] // empty'
}

value_slug=$(slug_of リリース "$value") || die "軸「リリース」を読めませんでした"
[ -n "$value_slug" ] || die "「${value}」は軸「リリース」の値ではありません"
product_slug=$(slug_of プロダクト "$product") || die "軸「プロダクト」を読めませんでした"
[ -n "$product_slug" ] || die "「${product}」は軸「プロダクト」の値ではありません"

undone=$(amenbo_ai task list --filter "dim:リリース=$value_slug dim:プロダクト=$product_slug done:false" --json |
    jq -c --arg own "$own_ref" '[.tasks[] | select(.ref != $own)]') ||
    die "「${value}」の done でないタスクの一覧を読めませんでした"

if [ "$(jq -r 'length' <<< "$undone")" = 0 ]; then
    done_count=$(amenbo_ai task list --filter "dim:リリース=$value_slug dim:プロダクト=$product_slug done:true" --json |
        jq -r '.total_matched') || die "「${value}」の done のタスクの一覧を読めませんでした"
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
} > "$left" || die "$left に書き込めませんでした"

report="分類「${value}」に ${product} のタスクが $(jq -r 'length' <<< "$undone")件残っています。"$'\n'
report+=$(jq -r '.[] | "- \(.ref)　\(.title)（\(.status)）"' <<< "$undone")
jq -n --arg exit "$LEFT_EXIT" --arg out "$LEFT_OUTPUT" --arg file "$LEFT_FILE" --arg report "$report" \
    '{version: 1, exit: $exit, outs: {($out): $file}, report: $report}' > "$AMENBO_OUTPUT"
