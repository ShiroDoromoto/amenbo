#!/usr/bin/env bash
# rerun-ci.sh — a script step: re-run the failed jobs of one run, and wait for it again.
#
# It runs `gh run rerun --failed`, waits until the run reads a new attempt, and waits for it with
# `watch-ci.sh run`. The step names its ways out and ports in Japanese, so the names below are the
# ones written in the code:
#
#   done            the run is green. Hands the run's id on
#   CI is red       the run is red again. Hands the run's id on; its report names the jobs that failed
#
# Input, from AMENBO_INPUT's `ins`: the run's id (required).
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or
# do — no run id, a re-run GitHub refused or never started, a watch that broke, a run that ended
# without a verdict — ends it with exit code 1, which the step leaves through its error way out.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, which
# is where `gh` reads the repository from (AMENBO_CI_REPO overrides it, as in watch-ci.sh).
set -euo pipefail

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

id=$(input run)
[[ "$id" =~ ^[0-9]+$ ]] || die "入力に run の id がありません：\"$id\""
resolve_repo

attempt() { gh run view "$id" -R "$repo" --json attempt --jq .attempt; }

before=$(attempt) || die "run を読めませんでした：$(run_url "$id")"
gh run rerun "$id" -R "$repo" --failed || die "失敗したジョブを再実行できませんでした：$(run_url "$id")"

# Until GitHub registers the re-run, the run reads its previous attempt and that attempt's verdict.
tries=0
until [ "$(attempt)" -gt "$before" ] 2>/dev/null; do
    tries=$((tries + 1))
    [ "$tries" -le 60 ] || die "10分待っても再実行が始まりませんでした：$(run_url "$id")"
    sleep 10
done

verdict=$(watch_run "$id")
outs=$(jq -n --arg r "$id" '{run: $r}')
if [ "$verdict" = green ]; then
    leave 完了 "$outs" "run $id の再実行は緑です：$(run_url "$id")"
fi
leave "CI が赤" "$outs" "run $id の再実行は赤です（$(failed_jobs "$id")）：$(run_url "$id")"
