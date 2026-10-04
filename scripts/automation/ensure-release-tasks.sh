#!/usr/bin/env bash
# ensure-release-tasks.sh — a script step: see that every open value of the release axis has its
# release task for one product, and file the ones that are missing.
#
# For each open value of the release axis, it looks for a task classified with that value, the
# product, and the release kind of work. Where there is none, it files a task titled to release
# that value, for the human, classified with the value, the product, the main theme and the release
# kind of work, and finishes creating it. The axes and values are named in Japanese, so the names
# below are the ones written in the board.
#
# Its one argument is the product's value, by name. It has one way out, done, and its report names
# the tasks it filed, or says it filed none.
#
# A filter splits on whitespace before it reads a value, so a value's name cannot be written in one.
# The filter names each value by its slug instead; `task add --dim` takes the name as an argument of
# its own, where a space is no harm.
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or
# write — no product, a product that is not a value, a command that failed — ends it with exit code 1,
# which the step leaves through its error way out.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in. That is
# where `task add` reads the project and the folder of a new task from.
set -euo pipefail

cd "$(dirname "$0")/../.."

die() { echo "✗ $*" >&2; exit 1; }

[ -n "${AMENBO_INPUT:-}" ] && [ -n "${AMENBO_OUTPUT:-}" ] ||
    die "run this as a script step: AMENBO_INPUT and AMENBO_OUTPUT are not set"

product="${1:-}"
[ -n "$product" ] || die "no product: pass the product's value as the argument"

amenbo_ai() { amenbo "$@" --actor ai; }

product_slug=$(amenbo_ai dimension show プロダクト --json |
    jq -r --arg p "$product" '[.values[] | select(.name == $p) | .slug][0] // empty') ||
    die "could not read the product axis"
[ -n "$product_slug" ] || die "\"$product\" is not a value of the product axis"

releases=$(amenbo_ai dimension show リリース --json |
    jq -c '.values[] | select(.closed | not) | {name, slug}') ||
    die "could not read the release axis"

# shellcheck disable=SC2016 # the backticks are Markdown, not a command
notes=$(printf '%s\n' '「リリース（amenbo）」が取って進めるリリースのタスク。' '' '## チェックリスト' \
    '出荷バイナリでしか確かめられない項目を `- [ ] …` で書く。')

filed=()
while IFS= read -r release; do
    [ -n "$release" ] || continue
    name=$(jq -r .name <<< "$release")
    slug=$(jq -r .slug <<< "$release")
    found=$(amenbo_ai task list --filter "dim:リリース=$slug dim:プロダクト=$product_slug dim:作業種別=リリース作業" --json |
        jq -r .total_matched) || die "could not list the release tasks of \"$name\""
    [ "$found" = 0 ] || continue
    id=$(amenbo_ai task add --title "$name をリリースする" --notes - --to human \
        --dim "リリース=$name" --dim "プロダクト=$product" --dim テーマ=メイン --dim 作業種別=リリース作業 \
        --json <<< "$notes" | jq -r '.task.id // empty') || die "could not file the release task of \"$name\""
    [ -n "$id" ] || die "filing the release task of \"$name\" gave no task id"
    amenbo_ai task finish-creating "AMB-T-$id" >&2 || die "could not finish creating AMB-T-$id"
    filed+=("AMB-T-$id")
done <<< "$releases"

if [ "${#filed[@]}" -eq 0 ]; then
    report="起票したタスク：なし"
else
    report="起票したタスク：${filed[*]}"
fi
jq -n --arg report "$report" '{version: 1, exit: "完了", outs: {}, report: $report}' > "$AMENBO_OUTPUT"
