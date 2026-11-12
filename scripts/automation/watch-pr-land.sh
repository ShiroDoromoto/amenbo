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
#   red             a check failed or was cancelled. Hands on, for each such check, its name, its run
#                   id and URL, and its failed log, as a file beside output.json. Whether the red is
#                   the change's own, main's or a flaky run is the next step's to judge, from the log
#
# Input, from AMENBO_INPUT's `ins`: the pull request number (required).
#
# The way out is decided by output.json, never by this script's exit code. Anything else — every check
# green and the pull request still not merging (watch-ci.sh's exit code 4), a pull request closed without
# merging, a watch that broke or stalled — ends it by its error way out, with the reason as its report.
#
# watch-ci.sh is called as it is: its exit codes are read by hand and by other steps. On DIRTY read
# twice in a row it says `needs a hand: DIRTY` and ends with exit code 6. Its events are read as
# they come, and the watch is stopped here on that line; exit code 6 is read as the same conflict,
# should the watch end before the line is read.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in,
# which is where `gh` reads the repository from (AMENBO_CI_REPO overrides it, as in watch-ci.sh).
set -euo pipefail

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

pr=$(jq -r '.ins["PR"] // empty' "$AMENBO_INPUT")
[[ "$pr" =~ ^[0-9]+$ ]] || die "入力にプルリクエストの番号がありません：\"$pr\""

resolve_repo
url="https://github.com/$repo/pull/$pr"

work=$(mktemp -d)
watch=""
trap '[ -n "$watch" ] && kill "$watch" 2>/dev/null; rm -rf "$work"' EXIT

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

# Leave through the conflict way out, once the pull request itself reads as conflicting.
leave_conflict() {
    local v
    v=$(gh pr view "$pr" -R "$repo" --json state,mergeStateStatus --jq '"\(.state) \(.mergeStateStatus)"') ||
        die "マージの状態を読めませんでした：$url"
    [ "$v" = "OPEN DIRTY" ] || die "watch-ci.sh は DIRTY と言いましたが、プルリクエストは \"$v\" です：$url"
    leave 衝突 '{}' "プルリクエスト $pr が main と衝突しています：$url"
}

if [ -n "$dirty" ]; then
    kill "$watch" 2>/dev/null || :
    wait "$watch" 2>/dev/null || :
    watch=""
    leave_conflict
fi

set +e
wait "$watch"
watched=$?
set -e
watch=""

case "$watched" in
    0)
        v=$(gh pr view "$pr" -R "$repo" --json state,mergeCommit) || die "マージされたかどうかを読めませんでした：$url"
        [ "$(jq -r .state <<< "$v")" = MERGED ] || die "watch-ci.sh はマージ済みと言いましたが、プルリクエストは $(jq -c . <<< "$v") です：$url"
        oid=$(jq -r '.mergeCommit.oid // empty' <<< "$v")
        [ -n "$oid" ] || die "プルリクエスト $pr はマージ済みですが、マージコミットがありません：$url"
        leave 完了 "$(jq -n --arg c "$oid" '{"コミット": $c}')" "プルリクエスト $pr が $oid としてマージされました：$url"
        ;;
    1)
        # 1 is also a watch that broke, and a pull request closed without merging. Only a check
        # read as failed or cancelled now is red.
        state=$(gh pr view "$pr" -R "$repo" --json state --jq .state) || die "プルリクエストの状態を読めませんでした：$url"
        [ "$state" = OPEN ] || die "プルリクエスト $pr はマージされないまま $state になっています：$url"
        # `gh pr checks` exits non-zero when a check failed, so its output is what is read.
        checks=$(gh pr checks "$pr" -R "$repo" --json name,bucket,link 2>&1) || :
        failed=$(jq -c '[.[] | select(.bucket == "fail" or .bucket == "cancel")]' <<< "$checks" 2>/dev/null) ||
            die "チェックを読めませんでした：${url}：$checks"
        [ "$(jq length <<< "$failed")" -gt 0 ] ||
            die "watch-ci.sh は終了コード 1 で終わりましたが、失敗したチェックがありません（${url}）：$(tail -n 1 "$work/events.log" 2>/dev/null)"
        while IFS=$'\t' read -r name bucket link; do
            echo "## $name ($bucket) $link"
            run=$(sed -nE 's|.*/actions/runs/([0-9]+).*|\1|p' <<< "$link")
            if [ -n "$run" ]; then
                echo "run: $run"
                gh run view "$run" -R "$repo" --log-failed || echo "(could not read the failed log of run $run)"
            fi
            echo
        done < <(jq -r '.[] | [.name, .bucket, .link] | @tsv' <<< "$failed") > "$out_dir/failed.log"
        leave 赤 '{"指摘": "failed.log"}' "プルリクエスト $pr のチェックが赤です（$(jq -r 'map(.name) | join(", ")' <<< "$failed")）：$url"
        ;;
    4)
        die "チェックはすべて緑ですが、マージされません（${url}）：$(grep '^settled but not merging:' "$work/events.log" | tail -n 1)"
        ;;
    6) leave_conflict ;;
    *) die "watch-ci.sh pr $pr が終了コード $watched で終わりました：$url" ;;
esac
