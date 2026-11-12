#!/usr/bin/env bash
# run-full-ci.sh — a script step: run the full CI on main before a release tag is pushed, and wait
# for it.
#
# It dispatches ci-full-manual.yml on main and waits for that run with `watch-ci.sh dispatch`, then
# `watch-ci.sh run`. It takes no input. The step names its ways out in Japanese, so the name below is
# the one written in the code. It has one way out, done, taken when the run is green; its report
# carries the run's URL.
#
# The way out is decided by output.json, never by this script's exit code. A red run ends it by its error
# way out, the report naming the run and the jobs that failed. So does anything it cannot read or do — a
# dispatch that failed, a watch that broke, a run that ended without a verdict.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, which
# is where `gh` reads the repository from (AMENBO_CI_REPO overrides it, as in watch-ci.sh).
set -euo pipefail

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

resolve_repo

stamp=$(now_utc)
gh workflow run ci-full-manual.yml -R "$repo" --ref main || die "main で ci-full-manual.yml を起動できませんでした"
id=$(resolve_run dispatch ci-full-manual.yml main "$stamp")

verdict=$(watch_run "$id")
if [ "$verdict" = green ]; then
    leave 完了 '{}' "main のフル CI は緑です：$(run_url "$id")"
fi

write_red_run "$id" >&2
die "main のフル CI は赤です。run ${id}（$(failed_jobs "$id")）：$(run_url "$id")"
