#!/usr/bin/env bash
# priority-scope.sh — a script step: lay out the tasks whose priorities are compared with each other,
# and say whether the tasks this run filed are among them.
#
# It takes no arguments. Its one input (FILED_INPUT below) is a file: the Markdown table the filing step
# writes, one row per task, which names a task in one column (REF_COLUMN) and says in another
# (FILED_COLUMN) whether this run filed it (FILED_YES). The columns are found by their headings, so their
# order does not matter.
#
# The release being worked toward is the first open value of the release axis, in the axis's order. For
# each open value of the product axis, the scope is the tasks not started (`status:todo`) that carry both
# values, in priority order. Each task's row gives its priority, how many tasks it blocks, and whether
# this run filed it; each product's line counts its tasks and its high ones, and says whether the high
# ones are more than HIGH_LIMIT percent of them. Raising or lowering a priority is left to whoever reads
# the table.
#
# The table is written beside output.json (TABLE_FILE), handed on as TABLE_OUTPUT. It leaves by:
#
#   DONE_EXIT  — one at least of the tasks this run filed is in a product's scope
#   OUT_EXIT   — none of them is, or there is no open release value, so there is nothing to compare
#
# A filter splits on whitespace before it reads a value, so a value's name cannot be written in one.
# The filter names each value by its slug instead.
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or
# write — no input, a table with neither column, a command that failed — ends it with exit code 1, which
# the step leaves through its error way out.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, whose
# project the tasks are read from.
set -euo pipefail

# The names the board writes in Japanese: the input, the table's columns, the word for a task this run
# filed, the axes, the output and its file, and the ways out.
FILED_INPUT="起票したタスク"
REF_COLUMN="AMB-T 番号"
FILED_COLUMN="今回起票したか"
FILED_YES="はい"
RELEASE_AXIS="リリース"
PRODUCT_AXIS="プロダクト"
TABLE_OUTPUT="比べる範囲"
TABLE_FILE="比べる範囲.md"
DONE_EXIT="完了"
OUT_EXIT="範囲外"
HIGH_LIMIT=20

cd "$(dirname "$0")/../.."

# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

table=$(input "$FILED_INPUT") || die "could not read $AMENBO_INPUT"
[ -n "$table" ] || die "no table: wire the filed tasks into this step's $FILED_INPUT"
[ -r "$table" ] || die "could not read the table at $table"

# Each table row is split on its bars and its cells trimmed. The first row that carries both headings
# says which cells they are; a row whose filed cell says yes gives the task in its ref cell. It is read
# with jq rather than awk, whose comparison of two Japanese words is not exact on every platform.
filed=$(jq -Rnr --arg ref "$REF_COLUMN" --arg filed "$FILED_COLUMN" --arg yes "$FILED_YES" '
    [inputs | select(test("^\\s*\\|")) | split("|") | map(gsub("^\\s+|\\s+$"; ""))] as $rows
    | ($rows | map(index($ref) as $r | index($filed) as $f | select($r != null and $f != null) | [$r, $f])
        | first) as $cols
    | if $cols == null then error("no columns")
      else $rows[] | select(.[$cols[1]] == $yes) | .[$cols[0]] end
' "$table") || die "the table at $table has no \"$REF_COLUMN\" and \"$FILED_COLUMN\" columns"

amenbo_ai() { amenbo "$@" --actor ai; }

out_file="$out_dir/$TABLE_FILE"
outs=$(jq -n --arg k "$TABLE_OUTPUT" --arg v "$TABLE_FILE" '{($k): $v}')

release=$(amenbo_ai dimension show "$RELEASE_AXIS" --json |
    jq -c '[.values[] | select(.closed | not) | {name, slug}][0] // empty') ||
    die "could not read the release axis"
if [ -z "$release" ]; then
    echo "「${RELEASE_AXIS}」に開いている値がありません。" > "$out_file" || die "could not write $out_file"
    leave "$OUT_EXIT" "$outs" "「${RELEASE_AXIS}」に開いている値が無いので、比べる範囲がありません"
fi
release_name=$(jq -r .name <<< "$release")
release_slug=$(jq -r .slug <<< "$release")

products=$(amenbo_ai dimension show "$PRODUCT_AXIS" --json |
    jq -c '.values[] | select(.closed | not) | {name, slug}') ||
    die "could not read the product axis"

in_scope=()
summary=()
{
    echo "# 「${RELEASE_AXIS}」の ${release_name} の未着手のタスク"
    while IFS= read -r product; do
        [ -n "$product" ] || continue
        name=$(jq -r .name <<< "$product")
        slug=$(jq -r .slug <<< "$product")
        listed=$(amenbo_ai task list --filter "status:todo dim:${RELEASE_AXIS}=$release_slug dim:${PRODUCT_AXIS}=$slug" \
            --sort priority --json) || die "could not list the tasks of \"$name\""
        [ "$(jq '.tasks | length' <<< "$listed")" = "$(jq .total_matched <<< "$listed")" ] ||
            die "the list of \"$name\" did not hold every task it matched"
        count=$(jq '.tasks | length' <<< "$listed")
        high=$(jq '[.tasks[] | select(.priority == "high")] | length' <<< "$listed")
        if [ "$count" -eq 0 ]; then
            share=0
        else
            share=$((high * 100 / count))
        fi
        if [ $((high * 100)) -gt $((count * HIGH_LIMIT)) ]; then over="超えている"; else over="超えていない"; fi
        summary+=("| ${name} | ${count} | ${high} | ${share}% | ${over} |")

        echo
        echo "## ${name}"
        echo
        echo "| AMB-T 番号 | タイトル | 優先度 | 依存されている数 | 今回起票したか |"
        echo "|---|---|---|---|---|"
        while IFS=$'\t' read -r ref priority title; do
            [ -n "$ref" ] || continue
            blocks=$(amenbo_ai task show "$ref" --json < /dev/null | jq '.blocks | length') || die "could not read $ref"
            if grep -qxF "$ref" <<< "$filed"; then
                mine="$FILED_YES"
                in_scope+=("$ref")
            else
                mine="いいえ"
            fi
            echo "| ${ref} | ${title//|/\\|} | ${priority} | ${blocks} | ${mine} |"
        done < <(jq -r '.tasks[] | [.ref, (.priority // "-"), (.title | gsub("[\t\n]"; " "))] | @tsv' <<< "$listed")
    done <<< "$products"

    echo
    echo "## high の割合"
    echo
    echo "| プロダクト | 件数 | high の件数 | high の割合 | ${HIGH_LIMIT}% を |"
    echo "|---|---|---|---|---|"
    for line in ${summary[@]+"${summary[@]}"}; do echo "$line"; done
} > "$out_file" || die "could not write $out_file"

filed_count=$(grep -c . <<< "$filed" || true)
if [ "${#in_scope[@]}" -eq 0 ]; then
    leave "$OUT_EXIT" "$outs" \
        "今回起票したタスク ${filed_count}件は、どれも「${RELEASE_AXIS}」の ${release_name} の未着手のタスクにありません"
fi
leave "$DONE_EXIT" "$outs" \
    "「${RELEASE_AXIS}」の ${release_name} の未着手のタスクに、今回起票したタスクが ${#in_scope[@]}件あります：${in_scope[*]}"
