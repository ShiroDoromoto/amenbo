#!/usr/bin/env bash
# watch-pr-land.sh — a script step: wait for one pull request to land, and say which way out its
# landing is.
#
# The step before it opened the pull request with auto-merge on and hands its number on. This
# waits for it with `watch-ci.sh pr` and reads how that ended into one of the step's ways out,
# written to AMENBO_OUTPUT. The step names its ways out and ports in Japanese, so the names below
# are the ones written in the code:
#
#   done            merged. Hands on the merge commit
#   conflict        the merge state is DIRTY: the branch conflicts with main
#   red             a check failed or was cancelled. Hands on the failed log of each such check as a
#                   file beside output.json. Whether a re-run would clear it is the next step's to
#                   judge, from the log
#
# Input, from AMENBO_INPUT's `ins`: the pull request number (required).
#
# The way out is decided by output.json, never by this script's exit code. Anything else — every
# check green and the pull request still not merging (watch-ci.sh's exit code 4), a pull request
# closed without merging, a watch that broke or stalled — ends it with exit code 1, which the step
# leaves through its error way out, the reason being the last line on stderr.
#
# watch-ci.sh is called as it is: its exit codes are read by hand and by other steps. It does not
# end on DIRTY — it says `needs a hand: DIRTY` once and waits on — so its events are read as they
# come, and the watch is stopped here on that line.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in,
# which is where `gh` reads the repository from (AMENBO_CI_REPO overrides it, as in watch-ci.sh).
set -euo pipefail

cd "$(dirname "$0")/../.."

die() { echo "✗ $*" >&2; exit 1; }

[ -n "${AMENBO_INPUT:-}" ] && [ -n "${AMENBO_OUTPUT:-}" ] ||
    die "run this as a script step: AMENBO_INPUT and AMENBO_OUTPUT are not set"

pr=$(jq -r '.ins["PR"] // empty' "$AMENBO_INPUT")
[[ "$pr" =~ ^[0-9]+$ ]] || die "no pull request number in the input: \"$pr\""

if [ -n "${AMENBO_CI_REPO:-}" ]; then
    repo="$AMENBO_CI_REPO"
else
    repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner) ||
        die "no repository: set AMENBO_CI_REPO=OWNER/REPO"
fi
url="https://github.com/$repo/pull/$pr"

work=$(mktemp -d)
watch=""
trap '[ -n "$watch" ] && kill "$watch" 2>/dev/null; rm -rf "$work"' EXIT
out_dir=$(dirname "$AMENBO_OUTPUT")

# Write output.json and stop: the way out, its outputs as a JSON object, and the report.
leave() {
    jq -n --arg exit "$1" --argjson outs "$2" --arg report "$3" \
        '{version: 1, exit: $exit, outs: $outs, report: $report}' > "$AMENBO_OUTPUT"
    exit 0
}

# Its stdout is the events it saw. Each goes to stderr, which the run keeps the end of, and to a
# log read once the watch is over.
mkfifo "$work/events"
AMENBO_CI_REPO="$repo" scripts/watch-ci.sh pr "$pr" > "$work/events" &
watch=$!
dirty=""
while IFS= read -r line; do
    echo "$line" >&2
    echo "$line" >> "$work/events.log"
    [ "$line" = "needs a hand: DIRTY" ] && { dirty=yes; break; }
done < "$work/events"

if [ -n "$dirty" ]; then
    kill "$watch" 2>/dev/null || :
    wait "$watch" 2>/dev/null || :
    watch=""
    v=$(gh pr view "$pr" -R "$repo" --json state,mergeStateStatus --jq '"\(.state) \(.mergeStateStatus)"') ||
        die "could not read the merge state of $url"
    [ "$v" = "OPEN DIRTY" ] || die "watch-ci.sh said DIRTY, and the pull request reads \"$v\": $url"
    leave 衝突 '{}' "pull request $pr conflicts with main: $url"
fi

set +e
wait "$watch"
watched=$?
set -e
watch=""

case "$watched" in
    0)
        v=$(gh pr view "$pr" -R "$repo" --json state,mergeCommit) || die "could not read the landing of $url"
        [ "$(jq -r .state <<< "$v")" = MERGED ] || die "watch-ci.sh said merged, and the pull request reads $(jq -c . <<< "$v"): $url"
        oid=$(jq -r '.mergeCommit.oid // empty' <<< "$v")
        [ -n "$oid" ] || die "pull request $pr is merged, and carries no merge commit: $url"
        leave 完了 "$(jq -n --arg c "$oid" '{"コミット": $c}')" "pull request $pr landed as $oid: $url"
        ;;
    1)
        # 1 is also a watch that broke, and a pull request closed without merging. Only a check
        # read as failed or cancelled now is red.
        state=$(gh pr view "$pr" -R "$repo" --json state --jq .state) || die "could not read the state of $url"
        [ "$state" = OPEN ] || die "pull request $pr is $state without landing: $url"
        # `gh pr checks` exits non-zero when a check failed, so its output is what is read.
        checks=$(gh pr checks "$pr" -R "$repo" --json name,bucket,link 2>&1) || :
        failed=$(jq -c '[.[] | select(.bucket == "fail" or .bucket == "cancel")]' <<< "$checks" 2>/dev/null) ||
            die "could not read the checks of $url: $checks"
        [ "$(jq length <<< "$failed")" -gt 0 ] ||
            die "watch-ci.sh ended with exit code 1, and no check of $url reads failed: $(tail -n 1 "$work/events.log" 2>/dev/null)"
        while IFS=$'\t' read -r name bucket link; do
            echo "## $name ($bucket) $link"
            run=$(sed -nE 's|.*/actions/runs/([0-9]+).*|\1|p' <<< "$link")
            if [ -n "$run" ]; then
                gh run view "$run" -R "$repo" --log-failed || echo "(could not read the failed log of run $run)"
            fi
            echo
        done < <(jq -r '.[] | [.name, .bucket, .link] | @tsv' <<< "$failed") > "$out_dir/failed.log"
        leave 赤 '{"失敗ログ": "failed.log"}' "pull request $pr is red ($(jq -r 'map(.name) | join(", ")' <<< "$failed")): $url"
        ;;
    4)
        die "every check of $url is green and it is not merging: $(grep '^settled but not merging:' "$work/events.log" | tail -n 1)"
        ;;
    *) die "watch-ci.sh pr $pr ended with exit code $watched: $url" ;;
esac
