#!/usr/bin/env bash
# finish-creating-filed.sh — a script step: finish creating the tasks a step before it filed, and see
# that none of them is left being created.
#
# It takes no arguments. Its one input (FILED_INPUT below) is a file: the Markdown table the filing step
# writes, one row per task, which names a task in one column (REF_COLUMN) and says in another
# (FILED_COLUMN) whether this run filed it (FILED_YES). The columns are found by their headings, so
# their order does not matter. Only the rows this run filed are taken; a task it found already there
# is left as it is.
#
# It runs `task finish-creating` on each, then lists the tasks still being created (`draft:yes`) and
# looks for any of them there. When every one finished and none is left, it leaves by DONE_EXIT, and its
# report counts the tasks it finished and how many of them are ready to be worked. When one was refused,
# or one is still being created, it leaves by the error way out, and its report names each with why.
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or
# write — no input, a table with neither column, a command that failed to list — ends it with exit code
# 1, which the step leaves through its error way out as well.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, whose
# project the list is read from.
set -euo pipefail

# The names the board writes in Japanese: the input, the table's columns, the word for a task this run
# filed, and the ways out: done for every task finished, and the error way out, which output.json names
# as `*`.
FILED_INPUT="起票したタスク"
REF_COLUMN="AMB-T 番号"
FILED_COLUMN="今回起票したか"
FILED_YES="はい"
DONE_EXIT="完了"
ERROR_EXIT="*"

cd "$(dirname "$0")/../.."

die() { echo "✗ $*" >&2; exit 1; }

[ -n "${AMENBO_INPUT:-}" ] && [ -n "${AMENBO_OUTPUT:-}" ] ||
    die "run this as a script step: AMENBO_INPUT and AMENBO_OUTPUT are not set"

table=$(jq -r --arg i "$FILED_INPUT" '.ins[$i] // empty' "$AMENBO_INPUT") || die "could not read $AMENBO_INPUT"
[ -n "$table" ] || die "no table: wire the filed tasks into this step's $FILED_INPUT"
[ -r "$table" ] || die "could not read the table at $table"

# Each table row is split on its bars and its cells trimmed. The first row that carries both headings
# says which cells they are; a row whose filed cell says yes gives the task in its ref cell. It is read
# with jq rather than awk, whose comparison of two Japanese words is not exact on every platform.
refs=$(jq -Rnr --arg ref "$REF_COLUMN" --arg filed "$FILED_COLUMN" --arg yes "$FILED_YES" '
    [inputs | select(test("^\\s*\\|")) | split("|") | map(gsub("^\\s+|\\s+$"; ""))] as $rows
    | ($rows | map(index($ref) as $r | index($filed) as $f | select($r != null and $f != null) | [$r, $f])
        | first) as $cols
    | if $cols == null then error("no columns")
      else $rows[] | select(.[$cols[1]] == $yes) | .[$cols[0]] end
' "$table") || die "the table at $table has no \"$REF_COLUMN\" and \"$FILED_COLUMN\" columns"

amenbo_ai() { amenbo "$@" --actor ai; }

# A refusal comes on stderr as JSON, after any warning lines, so stderr is kept apart from the answer on
# stdout.
errors=$(mktemp)
trap 'rm -f "$errors"' EXIT

finished=0
ready=0
refused=()
targets=()
while IFS= read -r ref; do
    [ -n "$ref" ] || continue
    targets+=("$ref")
    if said=$(amenbo_ai task finish-creating "$ref" --json 2> "$errors"); then
        finished=$((finished + 1))
        [ "$(jq -r '.task.ready' <<< "$said")" != true ] || ready=$((ready + 1))
    else
        why=$(sed -n '/^{/,/^}/p' "$errors" | jq -r '.error.message // empty' 2>/dev/null) || why=""
        [ -n "$why" ] || why=$(tail -n 1 "$errors")
        refused+=("${ref}：${why}")
    fi
done <<< "$refs"

drafts=$(amenbo_ai task list --filter "draft:yes" --json | jq -r '.tasks[].ref') ||
    die "could not list the tasks being created"
left=()
for ref in ${targets[@]+"${targets[@]}"}; do
    if grep -qxF "$ref" <<< "$drafts"; then left+=("$ref"); fi
done

report="作成を終えたタスク：${finished}件（うち ready：${ready}件）"
if [ "${#refused[@]}" -eq 0 ] && [ "${#left[@]}" -eq 0 ]; then
    exit_name="$DONE_EXIT"
else
    exit_name="$ERROR_EXIT"
    if [ "${#refused[@]}" -gt 0 ]; then
        report+=$'\n\n'"作成を終えられなかったタスク："
        for one in "${refused[@]}"; do report+=$'\n'"- $one"; done
    fi
    if [ "${#left[@]}" -gt 0 ]; then
        report+=$'\n\n'"作成中に残っているタスク："
        for one in "${left[@]}"; do report+=$'\n'"- $one"; done
    fi
fi

jq -n --arg exit "$exit_name" --arg report "$report" \
    '{version: 1, exit: $exit, outs: {}, report: $report}' > "$AMENBO_OUTPUT"
