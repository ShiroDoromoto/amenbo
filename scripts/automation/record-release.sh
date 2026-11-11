#!/usr/bin/env bash
# record-release.sh — a script step: write down, on the task the run is working, the version a value of
# the release axis went out as and the tasks it carried.
#
# Its arguments are the products, by their values' names, one or more. Its inputs are the release
# axis's value (VALUE_INPUT below), by name, and the version (VERSION_INPUT), without a leading `v`; the
# tag is the version with one. For each product it lists the tasks done under that value and that
# product, and adds one comment to the run's task giving the version, the tag, the product and those
# tasks, by ref and title. It has one way out, done, and its report gives the version and how many done
# tasks it wrote down for each product.
#
# The run's task is the one the optional input TASK_INPUT names, wired from the task a take_task step
# hands on. Where it is not wired, it is the task the run took last, read from the run that started
# this step (AMENBO_RUN).
#
# A filter splits on whitespace before it reads a value, so it names each value by its slug, looked up
# by name on the axis.
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or
# write — no product, no value, no version, a name that is not a value, no task to write on, a command
# that failed — ends it with exit code 1, which the step leaves through its error way out.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, whose
# project the axes and tasks are read from.
set -euo pipefail

# The names the board writes in Japanese: the inputs, and the way out.
VALUE_INPUT="分類"
VERSION_INPUT="版"
TASK_INPUT="タスク"
DONE_EXIT="完了"

cd "$(dirname "$0")/../.."

die() { echo "✗ $*" >&2; exit 1; }

[ -n "${AMENBO_INPUT:-}" ] && [ -n "${AMENBO_OUTPUT:-}" ] ||
    die "run this as a script step: AMENBO_INPUT and AMENBO_OUTPUT are not set"

[ "$#" -gt 0 ] || die "no product: pass the products' values as the arguments"
products=("$@")

input() { jq -r --arg i "$1" '.ins[$i] // empty' "$AMENBO_INPUT" || die "could not read $AMENBO_INPUT"; }
value=$(input "$VALUE_INPUT")
[ -n "$value" ] || die "no value: wire the release axis's value into this step's $VALUE_INPUT"
version=$(input "$VERSION_INPUT")
version="${version#v}"
[ -n "$version" ] || die "no version: wire the version into this step's $VERSION_INPUT"
task=$(input "$TASK_INPUT")

amenbo_ai() { amenbo "$@" --actor ai; }

if [ -z "$task" ]; then
    [ -n "${AMENBO_RUN:-}" ] || die "no task: wire one into $TASK_INPUT, or run this as a step of a run"
    id=$(amenbo_ai automation run-show "$AMENBO_RUN" --json | jq -r '.tasks[-1].task_id // empty') ||
        die "could not read run $AMENBO_RUN"
    [ -n "$id" ] || die "run $AMENBO_RUN has taken no task"
    task="AMB-T-$id"
fi

slug_of() {
    amenbo_ai dimension show "$1" --closed --json |
        jq -r --arg v "$2" '[.values[] | select(.name == $v) | .slug][0] // empty'
}

value_slug=$(slug_of リリース "$value") || die "could not read the release axis"
[ -n "$value_slug" ] || die "\"$value\" is not a value of the release axis"

counts=()
for product in "${products[@]}"; do
    product_slug=$(slug_of プロダクト "$product") || die "could not read the product axis"
    [ -n "$product_slug" ] || die "\"$product\" is not a value of the product axis"
    done_tasks=$(amenbo_ai task list --filter "dim:リリース=$value_slug dim:プロダクト=$product_slug done:true" --json) ||
        die "could not list the done tasks of \"$product\""
    count=$(jq -r '.tasks | length' <<< "$done_tasks")
    {
        echo "分類「${value}」で出した版を記録する。"
        echo "- 版：$version"
        echo "- タグ：v$version"
        echo "- プロダクト：$product"
        echo "- done のタスク：${count}件"
        echo
        jq -r '.tasks[] | "- \(.ref)　\(.title)"' <<< "$done_tasks"
    } | amenbo_ai comment add "$task" --text - >&2 || die "could not add the record of \"$product\" to $task"
    counts+=("$product ${count}件")
done

report="版 ${version}（タグ v${version}）を $task に記録しました。done のタスク：$(printf '%s、' "${counts[@]}")"
report="${report%、}"
jq -n --arg exit "$DONE_EXIT" --arg report "$report" \
    '{version: 1, exit: $exit, outs: {}, report: $report}' > "$AMENBO_OUTPUT"
