#!/usr/bin/env bash
# ensure-release-tasks.sh — a script step: see that every open value of the release axis has its
# release task for one product, and file the ones that are missing.
#
# For each open value of the release axis that holds at least one task of the product, it looks for a
# task classified with that value, the product, and the release kind of work. A value that holds none
# of the product's tasks has nothing of the product to release, and is passed over. Where there is
# none, it files a task titled to release
# that value, for the human, classified with the value, the product, the main theme and the release
# kind of work, and finishes creating it. The axes and values are named in Japanese, so the names
# below are the ones written in the board.
#
# Its first argument is the product's value, by name. Its second, which may be left out, is the id of
# the automation role that works release tasks: each task filed is worked under it, so the release
# automation's seat takes it. Left out, the task is filed under no role. Its third, which may also be
# left out, is the full path of the folder the release task is worked in (`task add --at`), for a
# product whose code lives in another checkout than this one. Left out, the task takes this
# checkout's folder. It has one way out, done, and its report names the tasks it filed, or says it
# filed none.
#
# A filter splits on whitespace before it reads a value, so a value's name cannot be written in one.
# The filter names each value by its slug instead; `task add --dim` takes the name as an argument of
# its own, where a space is no harm.
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or
# write — no product, a product that is not a value, a command that failed — ends it by its error way
# out, with the reason as its report.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in. That is
# where `task add` reads the project and the folder of a new task from.
set -euo pipefail

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

product="${1:-}"
[ -n "$product" ] || die "プロダクトがありません：1つ目の引数にプロダクトの値を渡してください"
role="${2:-}"
role_args=()
[ -z "$role" ] || role_args=(--role "$role")
folder="${3:-}"
at_args=()
[ -z "$folder" ] || at_args=(--at "$folder")

amenbo_ai() { amenbo "$@" --actor ai; }

product_slug=$(amenbo_ai dimension show プロダクト --json |
    jq -r --arg p "$product" '[.values[] | select(.name == $p) | .slug][0] // empty') ||
    die "軸「プロダクト」を読めませんでした"
[ -n "$product_slug" ] || die "「${product}」は軸「プロダクト」の値ではありません"

releases=$(amenbo_ai dimension show リリース --json |
    jq -c '.values[] | select(.closed | not) | {name, slug}') ||
    die "軸「リリース」を読めませんでした"

# shellcheck disable=SC2016 # the backticks are Markdown, not a command
notes=$(printf '%s\n' 'リリースのオートメーションが取って進めるリリースのタスク。' '' '## チェックリスト' \
    '出荷バイナリでしか確かめられない項目を `- [ ] …` で書く。')

filed=()
while IFS= read -r release; do
    [ -n "$release" ] || continue
    name=$(jq -r .name <<< "$release")
    slug=$(jq -r .slug <<< "$release")
    found=$(amenbo_ai task list --filter "dim:リリース=$slug dim:プロダクト=$product_slug dim:作業種別=リリース作業" --json |
        jq -r .total_matched) || die "「${name}」のリリースのタスクの一覧を読めませんでした"
    [ "$found" = 0 ] || continue
    held=$(amenbo_ai task list --filter "dim:リリース=$slug dim:プロダクト=$product_slug" --json |
        jq -r .total_matched) || die "「${name}」のタスクの一覧を読めませんでした"
    [ "$held" != 0 ] || continue
    id=$(amenbo_ai task add --title "$name をリリースする" --notes - --to human ${role_args[@]+"${role_args[@]}"} ${at_args[@]+"${at_args[@]}"} \
        --dim "リリース=$name" --dim "プロダクト=$product" --dim テーマ=メイン --dim 作業種別=リリース作業 \
        --json <<< "$notes" | jq -r '.task.id // empty') || die "「${name}」のリリースのタスクを起票できませんでした"
    [ -n "$id" ] || die "「${name}」のリリースのタスクを起票しましたが、タスクの id を読めませんでした"
    amenbo_ai task finish-creating "AMB-T-$id" >&2 || die "AMB-T-$id の作成を終えられませんでした"
    filed+=("AMB-T-$id")
done <<< "$releases"

if [ "${#filed[@]}" -eq 0 ]; then
    report="起票したタスク：なし"
else
    report="起票したタスク：${filed[*]}"
fi
jq -n --arg report "$report" '{version: 1, exit: "完了", outs: {}, report: $report}' > "$AMENBO_OUTPUT"
