#!/usr/bin/env bash
# list-gui-scenarios.sh — a script step: list what the GUI pre-distribution scenarios of a release are
# chosen from. Running them, and choosing among what is listed, is not this script's.
#
# It reads the release-tag.yml runs, and from them takes:
#
#   the previous version   the newest green run of another tag that started before the given run. Its
#                          id is what the previous version is seeded from, and its commit is where the
#                          first diff starts
#   the last check         the newest run of the same tag that started before the given run, when
#                          there is one. Its commit is where the second diff starts
#
# For each diff that has a start, up to the commit of the given run, it lists the GUI scenarios
# (`verification/scenarios/*.yaml` with `steps_gui:`, as they stand at that commit) that changed, and
# the paths under `app/` that changed. Which of those paths touch a screen is not for a script to say,
# so they are listed as they are. The list is written to a file beside output.json. The step names its
# ways out and ports in Japanese, so the names below are the ones written in the code:
#
#   done   hands on the list file, and the id of the previous version's run
#
# Inputs, from AMENBO_INPUT's `ins`: the version, without the leading `v`, and the run's id (both
# required).
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read —
# no version, no run, a run that is not of this tag, no previous version, a commit that cannot be
# fetched — ends it by its error way out, with the reason as its report.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, which
# is where the commits are read, and fetched into when it lacks them, and where `gh` reads the
# repository from (AMENBO_CI_REPO overrides it, as in watch-ci.sh). No tag or branch of the checkout
# is moved.
set -euo pipefail

LIST_OUTPUT="一覧"
LIST_FILE="GUI のシナリオ.md"
PREVIOUS_OUTPUT="前の版の run"
SCENARIOS="verification/scenarios"

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

version=$(input_version)
tag="v$version"
run=$(input run)
[[ "$run" =~ ^[0-9]+$ ]] || die "入力に run の id がありません：\"$run\""
resolve_repo

runs=$(gh run list -R "$repo" --workflow release-tag.yml --limit 200 \
    --json databaseId,headBranch,headSha,createdAt,conclusion) || die "release-tag.yml の run の一覧を読めませんでした"
this=$(jq -c --argjson id "$run" '.[] | select(.databaseId == $id)' <<< "$runs")
[ -n "$this" ] || die "run $run は release-tag.yml の直近 200 件の run にありません"
[ "$(jq -r .headBranch <<< "$this")" = "$tag" ] || die "run $run は $tag ではなく $(jq -r .headBranch <<< "$this") の run です"
head=$(jq -r .headSha <<< "$this")
started=$(jq -r .createdAt <<< "$this")

previous=$(jq -c --arg tag "$tag" --arg t "$started" '[.[] | select(.headBranch != $tag and (.headBranch | startswith("v"))
    and .conclusion == "success" and .createdAt < $t)] | max_by(.createdAt) // empty' <<< "$runs")
[ -n "$previous" ] || die "run $run より前に、別のバージョンの release-tag.yml の緑の run がありません"
previous_tag=$(jq -r .headBranch <<< "$previous")
previous_run=$(jq -r .databaseId <<< "$previous")
previous_sha=$(jq -r .headSha <<< "$previous")

last=$(jq -c --arg tag "$tag" --arg t "$started" '[.[] | select(.headBranch == $tag and .createdAt < $t)]
    | max_by(.createdAt) // empty' <<< "$runs")

have() { git cat-file -e "$1^{commit}" 2>/dev/null || git fetch -q origin "$1" || die "$1 を fetch できませんでした"; }
have "$head"
have "$previous_sha"

gui=$(git grep -l '^steps_gui:' "$head" -- "$SCENARIOS/*.yaml" | sed "s/^$head://") ||
    die "$head の $SCENARIOS に GUI のシナリオがありません"

# The changes from one commit to the commit of the run: the GUI scenarios, then the app/ paths.
section() {
    local from="$1" changed
    changed=$(git diff --name-only "$from..$head" -- "$SCENARIOS/") || die "$from..$head の差分を読めませんでした"
    echo "### GUI scenarios changed"
    echo
    comm -12 <(sort <<< "$changed") <(sort <<< "$gui") | sed 's/^/- /' | grep . || echo "(none)"
    echo
    echo "### app/ paths changed"
    echo
    git diff --name-only "$from..$head" -- app/ | sed 's/^/- /' | grep . || echo "(none)"
    echo
}

{
    echo "# GUI scenarios of $tag"
    echo
    echo "- run: $(run_url "$run") at $head"
    echo "- previous version: $previous_tag, run $(run_url "$previous_run") at $previous_sha"
    if [ -n "$last" ]; then
        echo "- last check of $tag: run $(run_url "$(jq -r .databaseId <<< "$last")") at $(jq -r .headSha <<< "$last")"
    else
        echo "- last check of $tag: none"
    fi
    echo "- GUI scenarios at $head: $(grep -c . <<< "$gui")"
    echo
    echo "## Since the previous version ($previous_tag..$tag)"
    echo
    section "$previous_sha"
    if [ -n "$last" ]; then
        last_sha=$(jq -r .headSha <<< "$last")
        have "$last_sha"
        echo "## Since the last check of $tag ($last_sha..$head)"
        echo
        section "$last_sha"
    fi
} > "$out_dir/$LIST_FILE"

outs=$(jq -n --arg lk "$LIST_OUTPUT" --arg lf "$LIST_FILE" --arg pk "$PREVIOUS_OUTPUT" --arg pr "$previous_run" \
    '{($lk): $lf, ($pk): $pr}')
if [ -n "$last" ]; then
    leave 完了 "$outs" "$tag の GUI のシナリオを、${previous_tag}（run ${previous_run}）からと、前回の確認（$(jq -r .headSha <<< "$last")）からの2つで一覧にしました"
fi
leave 完了 "$outs" "$tag の GUI のシナリオを、${previous_tag}（run ${previous_run}）から一覧にしました。$tag の確認はこれが初めてです"
