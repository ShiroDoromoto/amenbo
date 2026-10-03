#!/usr/bin/env bash
# watch-ci-run.sh — a script step: wait for one check-branch-manual run, and say which way out its
# verdict is.
#
# The step before it started the run and hands its id on. This waits for it with `watch-ci.sh run`
# and reads the verdict into one of the step's ways out, written to AMENBO_OUTPUT. The step names
# its ways out and ports in Japanese, so the names below are the ones written in the code:
#
#   done            green, and the step before did not ask for the screen to be checked
#   screen check    green, and the step before said the change moves what the screen does
#   red             red, or green with a perf budget WARN in its log. Hands on the failed log (the
#                   WARN lines, for a budget) as a file beside output.json
#   wrong filter    red only because the test filter matched nothing (nextest's
#                   `error: no tests to run`). Hands on the reason. Nothing in the code is wrong,
#                   so it goes back to the step that chose the tests rather than to the fix
#
# Inputs, from AMENBO_INPUT's `ins`: the run id (required), and whether the screen needs checking
# (optional).
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read
# — no run id, a watch that broke, a run that ended without a verdict — ends it with exit code 1,
# which the step leaves through its error way out, instead of passing for a red.
#
# watch-ci.sh is called as it is: its exit codes are read by hand and by other steps, so the
# reading of a red log is done here, not there.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in,
# which is where `gh` reads the repository from (AMENBO_CI_REPO overrides it, as in watch-ci.sh).
set -euo pipefail

cd "$(dirname "$0")/../.."

die() { echo "✗ $*" >&2; exit 1; }

[ -n "${AMENBO_INPUT:-}" ] && [ -n "${AMENBO_OUTPUT:-}" ] ||
    die "run this as a script step: AMENBO_INPUT and AMENBO_OUTPUT are not set"

run_id=$(jq -r '.ins["GitHub Actions の run ID"] // empty' "$AMENBO_INPUT")
[[ "$run_id" =~ ^[0-9]+$ ]] || die "no run id in the input: \"$run_id\""
screen=$(jq -r '.ins["画面の確認が要る"] // empty' "$AMENBO_INPUT")

if [ -n "${AMENBO_CI_REPO:-}" ]; then
    repo="$AMENBO_CI_REPO"
else
    repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner) ||
        die "no repository: set AMENBO_CI_REPO=OWNER/REPO"
fi
url="https://github.com/$repo/actions/runs/$run_id"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
out_dir=$(dirname "$AMENBO_OUTPUT")

# Write output.json and stop: the way out, its outputs as a JSON object, and the report.
leave() {
    jq -n --arg exit "$1" --argjson outs "$2" --arg report "$3" \
        '{version: 1, exit: $exit, outs: $outs, report: $report}' > "$AMENBO_OUTPUT"
    exit 0
}

# Hand the failure on as a file, and leave through the red way out.
red() {
    cp "$1" "$out_dir/failed.log"
    leave 赤 '{"テストの失敗": "failed.log"}' "$2"
}

# Its stdout is the events it saw; they go to stderr, which the run keeps the end of.
set +e
AMENBO_CI_REPO="$repo" scripts/watch-ci.sh run "$run_id" >&2
watched=$?
set -e

case "$watched" in
    0)
        gh run view "$run_id" -R "$repo" --log > "$work/run.log" || die "could not read the log of $url"
        if grep -F "perf budget exceeded" "$work/run.log" > "$work/perf.log"; then
            red "$work/perf.log" "run $run_id is green, but its log carries a perf budget WARN: $url"
        fi
        if [ "$screen" = 要る ]; then
            leave 画面の確認が要る '{}' "run $run_id is green, and the change moves what the screen does: $url"
        fi
        leave 完了 '{}' "run $run_id is green: $url"
        ;;
    1)
        # 1 is also a watch that broke. Only a run that finished and did not succeed is red.
        verdict=$(gh run view "$run_id" -R "$repo" --json status,conclusion --jq '"\(.status) \(.conclusion)"') ||
            die "could not read the verdict of $url"
        case "$verdict" in
            "completed success" | "completed null") die "watch-ci.sh said red, and the run reads \"$verdict\": $url" ;;
            completed\ *) ;;
            *) die "the watch ended before the run did (\"$verdict\"): $url" ;;
        esac
        gh run view "$run_id" -R "$repo" --log-failed > "$work/failed.log" ||
            die "could not read the failed log of $url"
        if grep -F "error: no tests to run" "$work/failed.log" > "$work/none.log"; then
            reason="The test filter matched no tests, so the run failed without running any: $url
$(cat "$work/none.log")"
            leave 指定の誤り "$(jq -n --arg r "$reason" '{"理由": $r}')" "run $run_id ran no tests: $url"
        fi
        red "$work/failed.log" "run $run_id is red: $url"
        ;;
    *) die "watch-ci.sh run $run_id ended with exit code $watched: $url" ;;
esac
