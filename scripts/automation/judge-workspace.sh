#!/usr/bin/env bash
# judge-workspace.sh — a script step: sort the paths a task says it touches by where each one sits in
# a repository, and leave by what they add up to.
#
# Its one argument, which may be left out, is the full path of the work folder whose repository the
# paths are judged against, for a product whose code lives in another checkout than this one. Left
# out, it is this checkout. Its one input (TASK_INPUT below) is a task_take input wired from
# take_task's output of the same name, which reaches it as the task's ref.
#
# It reads the section of the task's notes headed SECTION, and takes each `code span` there that reads
# as a path: one with a slash in it, or a bare file name that the repository holds at its top. A span
# with spaces, placeholders, wildcards or other marks of a command or an expression is passed over.
# A path is read against the repository's top; `~/` is the home folder. Each path is put in one of five
# kinds:
#
#   TRACKED   — git tracks it, or a file under it (`git ls-files`)
#   IGNORED   — inside the repository, and `git check-ignore` matches it
#   UNTRACKED — inside the repository, neither tracked nor ignored, and its parent folder is already
#               there: a new file put in that folder
#   MISSING   — inside the repository, neither tracked nor ignored, and its parent folder is not there
#               either: it may be a path in another repository rather than a new file in this one
#   OUTSIDE   — not under the repository's top
#
# The kinds are written to a Markdown table (TABLE_FILE, beside output.json, handed on as TABLE_OUTPUT),
# one row per path, with the reason a MISSING or OUTSIDE path is sent to be judged. It leaves by:
#
#   DONE_EXIT     — every path is inside the repository, and one at least is tracked or new
#   IGNORED_EXIT  — every path is inside the repository and ignored
#   JUDGE_EXIT    — no section, no path in it, or a path that is missing or outside the repository.
#                   Whether such a task commits to another repository, makes new folders in this one or
#                   commits nothing is not for a script to say.
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or
# write — no task, a work folder outside git, a command that failed — ends it by its error way out, with
# the reason as its report.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in.
set -euo pipefail

# The names the board writes in Japanese: the input, the notes' section, the output and its file, the
# kinds, and the ways out.
TASK_INPUT="タスク"
SECTION="## 触るところ"
TABLE_OUTPUT="作業場所"
TABLE_FILE="作業場所.md"
TRACKED="管理下"
IGNORED="gitignore"
UNTRACKED="追跡外"
MISSING="リポジトリに無い"
OUTSIDE="リポジトリの外"
DONE_EXIT="完了"
IGNORED_EXIT="git の外"
JUDGE_EXIT="判断が要る"

cd "$(dirname "$0")/../.."

# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

folder="${1:-$(pwd)}"
top=$(git -C "$folder" rev-parse --show-toplevel 2>/dev/null) || die "$folder は git のリポジトリの中にありません"

task=$(input "$TASK_INPUT") || die "$AMENBO_INPUT を読めませんでした"
[ -n "$task" ] || die "タスクがありません：take_task の「${TASK_INPUT}」を、このステップの「${TASK_INPUT}」につないでください"

notes=$(amenbo task show "$task" --json --actor ai | jq -r '.notes // ""') || die "$task を読めませんでした"

# The section runs from its heading to the next heading of the same or a higher level. It is read in
# bash rather than awk, whose comparison of two Japanese words is not exact on every platform.
section=""
on=0
while IFS= read -r line; do
    if [ "$on" = 0 ]; then
        [ "${line%"${line##*[![:space:]]}"}" != "$SECTION" ] || on=1
        continue
    fi
    case "$line" in "# "* | "## "*) break ;; esac
    section+="$line"$'\n'
done <<< "$notes"

# Every code span in the section, one per line, without its backticks.
# shellcheck disable=SC2016 # the backticks are Markdown, not a command
spans=$(grep -o '`[^`]*`' <<< "$section" | sed 's/^`//; s/`$//' || true)

paths=()
while IFS= read -r span; do
    [ -n "$span" ] || continue
    case "$span" in
        -* | *[[:space:]\<\>\*\?\{\}\[\]\(\)\$\|=\"\'\;\&]*) continue ;;
    esac
    span="${span%/}"
    case "$span" in
        \~/*) paths+=("$HOME/${span#\~/}") ;;
        */*) paths+=("$span") ;;
        *.*) [ -e "$top/$span" ] && paths+=("$span") ;;
    esac
done <<< "$spans"

# Whether a path, as written, falls under the repository's top. A relative path is read from the top.
inside() {
    local abs="$1" dir
    case "$abs" in /*) ;; *) abs="$top/$abs" ;; esac
    dir=$(dirname "$abs")
    while [ ! -d "$dir" ]; do dir=$(dirname "$dir"); done
    dir=$(cd "$dir" && pwd -P)
    local real_top
    real_top=$(cd "$top" && pwd -P)
    case "$dir/" in "$real_top"/*) return 0 ;; *) return 1 ;; esac
}

# Sets kind to the path's kind, and reason to why a path sent to be judged is sent there.
kind_of() {
    local path="$1" parent abs_parent
    reason=""
    if ! inside "$path"; then
        kind="$OUTSIDE"
        reason="${top} の下に無い"
        return
    fi
    if [ -n "$(git -C "$top" ls-files -- "$path" | head -n 1)" ]; then kind="$TRACKED"; return; fi
    if git -C "$top" check-ignore -q -- "$path"; then kind="$IGNORED"; return; fi
    parent=$(dirname "$path")
    case "$parent" in /*) abs_parent="$parent" ;; *) abs_parent="$top/$parent" ;; esac
    if [ -d "$abs_parent" ]; then kind="$UNTRACKED"; return; fi
    kind="$MISSING"
    reason="ファイルも、親のフォルダ \`$parent\` もリポジトリに無い"
}

table="$out_dir/$TABLE_FILE"
tracked=0 ignored=0 untracked=0 missing=0 outside=0
{
    echo "| パス | 分け | 判断に回す理由 |"
    echo "|---|---|---|"
    for path in ${paths[@]+"${paths[@]}"}; do
        kind_of "$path"
        case "$kind" in
            "$TRACKED") tracked=$((tracked + 1)) ;;
            "$IGNORED") ignored=$((ignored + 1)) ;;
            "$UNTRACKED") untracked=$((untracked + 1)) ;;
            "$MISSING") missing=$((missing + 1)) ;;
            *) outside=$((outside + 1)) ;;
        esac
        echo "| \`$path\` | $kind | $reason |"
    done
} > "$table" || die "$table に書き込めませんでした"

total=$((tracked + ignored + untracked + missing + outside))
counts="${TRACKED} ${tracked}件・${UNTRACKED} ${untracked}件・${IGNORED} ${ignored}件・${MISSING} ${missing}件・${OUTSIDE} ${outside}件"
if [ -z "$section" ]; then
    exit_name="$JUDGE_EXIT"
    report="${task} の notes に「${SECTION#\#\# }」の節がありません"
elif [ "$total" -eq 0 ]; then
    exit_name="$JUDGE_EXIT"
    report="${task} の「${SECTION#\#\# }」にパスが書かれていません"
elif [ "$outside" -gt 0 ]; then
    exit_name="$JUDGE_EXIT"
    report="${task} の「${SECTION#\#\# }」に ${top} の外のパスがあります（${counts}）"
elif [ "$missing" -gt 0 ]; then
    exit_name="$JUDGE_EXIT"
    report="${task} の「${SECTION#\#\# }」に、親のフォルダも ${top} に無いパスがあります（${counts}）"
elif [ "$ignored" -eq "$total" ]; then
    exit_name="$IGNORED_EXIT"
    report="${task} の「${SECTION#\#\# }」のパスは、すべて ${top} の gitignore されたファイルです（${counts}）"
else
    exit_name="$DONE_EXIT"
    report="${task} の「${SECTION#\#\# }」のパスは、すべて ${top} の中にあり、git の管理に入ります（${counts}）"
fi

leave "$exit_name" "$(jq -n --arg k "$TABLE_OUTPUT" --arg v "$TABLE_FILE" '{($k): $v}')" "$report"
