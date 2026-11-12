#!/usr/bin/env bash
# watch-main-ci.sh — a script step: wait for the CI run a merge into main started, and say which way
# out its verdict is.
#
# The step before it landed the pull request and hands on its merge commit. This waits for that
# commit's push run with `watch-ci.sh main`. When GitHub started none before the deadline
# (watch-ci.sh's exit code 3), it dispatches ci-change-manual.yml on main against the commit's first
# parent, and waits for that run instead. The step names its ways out and ports in Japanese, so the
# names below are the ones written in the code:
#
#   done            green. Hands the merge commit on as it came
#   main is red     red. Hands on, as a file beside output.json, the run's id and URL, the jobs that
#                   failed, the merge commit and the main commit before it (what a revert needs), and
#                   the failed log. Whether the red is the merge's own, another merge's or a flaky
#                   run is the next step's to judge
#
# Input, from AMENBO_INPUT's `ins`: the merge commit (required).
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or do
# — no commit, a watch that broke, a run that ended without a verdict — ends it by its error way out,
# with the reason as its report.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, which
# is where `gh` reads the repository from (AMENBO_CI_REPO overrides it, as in watch-ci.sh).
set -euo pipefail

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

commit=$(input コミット)
[[ "$commit" =~ ^[0-9a-f]{7,40}$ ]] || die "入力にコミットがありません：\"$commit\""
resolve_repo

parent=$(gh api "repos/$repo/commits/$commit" --jq '.parents[0].sha') ||
    die "$repo のコミット $commit を読めませんでした"
[ -n "$parent" ] || die "$repo のコミット $commit に親がありません"

code=0
id=$(resolve_run main "$commit") || code=$?
case "$code" in
    0) ;;
    3)
        echo "no push run started for $commit; dispatching ci-change-manual.yml against $parent" >&2
        stamp=$(now_utc)
        gh workflow run ci-change-manual.yml -R "$repo" --ref main -f "base=$parent" ||
            die "main で ci-change-manual.yml を起動できませんでした"
        id=$(resolve_run dispatch ci-change-manual.yml main "$stamp")
        ;;
    *) exit "$code" ;;
esac

verdict=$(watch_run "$id")
if [ "$verdict" = green ]; then
    leave 完了 "$(jq -n --arg c "$commit" '{"コミット": $c}')" "main は $commit で緑です：$(run_url "$id")"
fi

{
    echo "merge commit: $commit"
    echo "main before it: $parent"
    write_red_run "$id"
} > "$out_dir/red.log"
leave "main が赤" '{"指摘": "red.log"}' "main は $commit で赤です。run ${id}（$(failed_jobs "$id")）：$(run_url "$id")"
