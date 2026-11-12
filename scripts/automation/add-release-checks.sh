#!/usr/bin/env bash
# add-release-checks.sh — a script step: write what is to be checked on the shipped binary into the
# checklist of the release task it belongs to.
#
# It takes no arguments. Its one input (CHECKS_INPUT below) is the list of checks, either a file or the
# text itself: one line per check, `AMB-T-<n>：<what to check>` (a half-width colon does as well), which
# may be led by `- ` or `- [ ] `. Lines that are not a check are passed over; a list that is not empty
# but holds no check at all is refused.
#
# For each check it reads the task it names. The release task is the one classified with that task's
# value of the release axis — or, where the task has none, the first open value of the axis — the same
# value of the product axis, and the release kind of work. It reads that task's notes and adds the line
# `- [ ] AMB-T-<n>：<what to check>` at the end of the section headed CHECKLIST_HEADING, or, where the
# notes have no such heading, adds the heading and the line at the end of the notes. Nothing already in
# the notes is taken out. Where a checklist line for the same task is already there, it adds nothing.
# Once it has written the notes back, it reads them again and sees that they are what it wrote.
#
# A check whose task has no value of the product axis, or whose release task cannot be found, is written
# nowhere: the report names it. It has one way out, done, and its report gives the lines it added, by
# release task, the ones it found already there, and the ones it found no release task for.
#
# A filter splits on whitespace before it reads a value, so it names each value by its slug, looked up by
# name on the axis.
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or
# write — no list, a list with no check, a name that is not a value, notes that did not read back as
# written, a command that failed — ends it by its error way out, with the reason as its report.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, whose
# project the axes and tasks are read from.
set -euo pipefail

# The names the board writes in Japanese: the input, the heading of the checklist, the release kind of
# work, and the way out.
CHECKS_INPUT="昇格前"
CHECKLIST_HEADING="チェックリスト"
RELEASE_KIND="リリース作業"
DONE_EXIT="完了"

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

given=$(jq -r --arg i "$CHECKS_INPUT" '.ins[$i] // empty' "$AMENBO_INPUT") || die "$AMENBO_INPUT を読めませんでした"
[ -n "$given" ] || die "一覧がありません：確かめることの一覧を、このステップの「${CHECKS_INPUT}」につないでください"
if [ -f "$given" ]; then
    list=$(cat "$given") || die "一覧 $given を読めませんでした"
else
    list="$given"
fi

checks=$(jq -Rc '
    capture("^\\s*(?:[-*]\\s+(?:\\[[ xX]\\]\\s+)?)?AMB-T-(?<n>[0-9]+)\\s*[：:]\\s*(?<what>.*\\S)\\s*$")
' <<< "$list") || die "一覧を読めませんでした"

if [ -z "$checks" ]; then
    [ -z "$(tr -d '[:space:]' <<< "$list")" ] || die "一覧に「AMB-T-<n>：<確かめること>」の形の行がありません"
    jq -n --arg exit "$DONE_EXIT" '{version: 1, exit: $exit, outs: {}, report: "足す項目：なし"}' > "$AMENBO_OUTPUT"
    exit 0
fi

amenbo_ai() { amenbo "$@" --actor ai; }

slug_of() {
    amenbo_ai dimension show "$1" --closed --json |
        jq -r --arg v "$2" '[.values[] | select(.name == $v) | .slug][0] // empty'
}

kind_slug=$(slug_of 作業種別 "$RELEASE_KIND") || die "軸「作業種別」を読めませんでした"
[ -n "$kind_slug" ] || die "「${RELEASE_KIND}」は軸「作業種別」の値ではありません"
first_open=$(amenbo_ai dimension show リリース --json | jq -r '[.values[] | select(.closed | not) | .name][0] // empty') ||
    die "軸「リリース」を読めませんでした"

# Whether the notes already hold a checklist line for task n.
# shellcheck disable=SC2016 # $n is jq's, not the shell's
HAS_LINE='split("\n") | any(test("^\\s*[-*]\\s+\\[[ xX]\\]\\s+AMB-T-" + $n + "(?![0-9])"))'

# The notes with the line added at the end of the checklist section, or under a new heading at the end.
# shellcheck disable=SC2016 # $line and $heading are jq's, not the shell's
ADD_LINE='
    split("\n") as $l
    | ([$l | to_entries[] | select(.value | test("^##\\s+" + $heading + "\\s*$")) | .key][0]) as $h
    | if $h == null then
        sub("\\s+$"; "") as $body
        | if $body == "" then "## " + $heading + "\n" + $line
          else $body + "\n\n## " + $heading + "\n" + $line end
      else
        ([$l | to_entries[] | select(.key > $h and (.value | test("^#{1,2}\\s"))) | .key][0] // ($l | length)) as $end
        | ([range($h; $end) | select($l[.] | test("\\S"))] | last) as $last
        | $l[0:$last + 1] + [$line] + $l[$last + 1:] | join("\n")
      end'

added=()
already=()
missing=()
while IFS= read -r check; do
    n=$(jq -r .n <<< "$check")
    what=$(jq -r .what <<< "$check")
    task="AMB-T-$n"
    shown=$(amenbo_ai task show "$task" --json) || die "$task を読めませんでした"
    value=$(jq -r '[.dimensions[] | select(.dimension == "リリース") | .value][0] // empty' <<< "$shown")
    [ -n "$value" ] || value="$first_open"
    product=$(jq -r '[.dimensions[] | select(.dimension == "プロダクト") | .value][0] // empty' <<< "$shown")
    if [ -z "$product" ] || [ -z "$value" ]; then
        missing+=("${task}：プロダクトかリリースの値が無い（${what}）")
        continue
    fi
    value_slug=$(slug_of リリース "$value") || die "軸「リリース」を読めませんでした"
    [ -n "$value_slug" ] || die "「${value}」は軸「リリース」の値ではありません"
    product_slug=$(slug_of プロダクト "$product") || die "軸「プロダクト」を読めませんでした"
    [ -n "$product_slug" ] || die "「${product}」は軸「プロダクト」の値ではありません"

    release=$(amenbo_ai task list --filter "dim:リリース=$value_slug dim:プロダクト=$product_slug dim:作業種別=$kind_slug" --json |
        jq -r '.tasks[0].ref // empty') || die "「${value}」のリリースのタスクの一覧を読めませんでした"
    if [ -z "$release" ]; then
        missing+=("${task}：分類「${value}」・プロダクト「${product}」のリリースのタスクが無い（${what}）")
        continue
    fi

    notes=$(amenbo_ai task show "$release" --json | jq -r '.notes // ""') || die "$release を読めませんでした"
    if [ "$(jq -Rsr --arg n "$n" "$HAS_LINE" <<< "$notes")" = true ]; then
        already+=("${release}：${task}")
        continue
    fi
    line="- [ ] ${task}：${what}"
    new_notes=$(jq -Rsr --arg heading "$CHECKLIST_HEADING" --arg line "$line" "$ADD_LINE" <<< "$notes") ||
        die "$release の notes に行を足せませんでした"
    amenbo_ai task update "$release" --notes - <<< "$new_notes" >&2 || die "$release の notes を書き込めませんでした"

    read_back=$(amenbo_ai task show "$release" --json | jq -r '.notes // ""') || die "$release を読み直せませんでした"
    [ "$read_back" = "$new_notes" ] ||
        die "$release の notes を読み直すと、書いたとおりになっていません"
    added+=("${release}：${line}")
done <<< "$checks"

list_of() { if [ "$#" -eq 0 ]; then echo "なし"; else printf -- '\n- %s' "$@"; fi; }
report="足した行：$(list_of ${added[@]+"${added[@]}"})"
report+=$'\n'"すでにあった行：$(list_of ${already[@]+"${already[@]}"})"
report+=$'\n'"リリースのタスクが見つからなかった項目：$(list_of ${missing[@]+"${missing[@]}"})"
jq -n --arg exit "$DONE_EXIT" --arg report "$report" \
    '{version: 1, exit: $exit, outs: {}, report: $report}' > "$AMENBO_OUTPUT"
