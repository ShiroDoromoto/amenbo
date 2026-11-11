#!/usr/bin/env bash
# run-cli-scenarios.sh — a script step: run the CLI pre-distribution scenarios against the CLI the
# downloaded mac arm64 .pkg ships.
#
# It runs `devtool vm verify cli` on `dist-macos/amenbo-darwin-arm64.pkg` in the place with `--json`,
# its JSON written to `verify-cli.json` and the rest to `verify-cli.log` there. devtool is built with
# `make devtool` when it is not on the PATH. Its exit code is the verdict. The step names its ways out
# and ports in Japanese, so the names below are the ones written in the code:
#
#   done   exit code 0. The report names how many scenarios ran and how many were skipped for their
#          driver
#   red    any other. Hands on, as a file beside output.json, the scenarios that failed and their
#          failing steps, or the end of the log when the JSON is empty
#
# Input, from AMENBO_INPUT's `ins`: the place the artifacts were downloaded to (required).
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot do — no
# place, no .pkg in it, no devtool — ends it with exit code 1, which the step leaves through its error
# way out.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, whose
# scenarios are the ones run.
set -euo pipefail

RED_FILE="CLI のシナリオを回す.md"

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

place=$(input_place)
pkg="$place/dist-macos/amenbo-darwin-arm64.pkg"
[ -f "$pkg" ] || die "$pkg がありません"
command -v devtool >/dev/null || make devtool >&2 || die "devtool が PATH に無く、make devtool も失敗しました"
command -v devtool >/dev/null || die "make devtool のあとも devtool が PATH にありません"

json="$place/verify-cli.json"
log="$place/verify-cli.log"
code=0
devtool vm verify cli "$pkg" --json > "$json" 2> "$log" || code=$?

# The roll-up verify-all prints: the last line of the output that is a JSON object with a total.
rollup=$(grep '^{' "$json" | tail -n 1 | jq -c 'select(has("total"))' 2>/dev/null) || rollup=""

if [ "$code" = 0 ]; then
    if [ -n "$rollup" ]; then
        leave 完了 '{}' "$(jq -r '"CLI のシナリオは緑です：\(.total) 件のうち \(.passed) 件が通り、ドライバの都合で \(.skipped) 件を飛ばしました"' <<< "$rollup")"
    fi
    leave 完了 '{}' "devtool vm verify cli は終了コード 0 で終わりましたが、$json に集計がありません"
fi

{
    echo "# The CLI scenarios are red"
    echo
    echo "- pkg: $pkg"
    echo "- devtool vm verify cli ended with exit code $code"
    echo "- json: $json"
    echo "- log: $log"
    echo
    if [ -n "$rollup" ]; then
        jq -r '"\(.passed) of \(.total) passed, \(.failed) failed, \(.skipped) skipped for their driver", ""' <<< "$rollup"
        jq -r '.scenarios[] | select(.passed == false) |
            "## \(.scenario // .path)",
            (if .error then "- error: \(.error)" else empty end),
            ((.steps // [])[] | select(.pass == false) | "- step \(.step) (\(.kind)): \(.note)"),
            ""' <<< "$rollup"
    else
        echo "The JSON is empty, so the end of the log is what devtool stopped on:"
        echo
        echo '```'
        tail -n 40 "$log"
        echo '```'
    fi
} > "$out_dir/$RED_FILE"
if [ -n "$rollup" ]; then
    leave_red "$RED_FILE" "$(jq -r '"CLI のシナリオが赤です：\(.total) 件のうち \(.failed) 件が失敗しました（\([.scenarios[] | select(.passed == false) | .scenario // .path] | join(", "))）"' <<< "$rollup")"
fi
leave_red "$RED_FILE" "devtool vm verify cli が、結果を出す前に終了コード $code で終わりました：$(tail -n 1 "$log")"
