#!/usr/bin/env bash
# push-tag.sh — a script step: tag main with a release version, push the tag, and wait for the run it
# starts.
#
# It brings the checkout's main up to origin/main (fast-forward only), tags it `v<version>` with an
# annotated tag, pushes the tag, and waits for its release-tag.yml run with `watch-ci.sh tag`. It
# pushes and waits only: deleting a tag, or choosing another version, is left to the steps after.
# The step names its ways out and ports in Japanese, so the names below are the ones written in the
# code:
#
#   done            the run is green. Hands on the run's id
#   CI is red       the run is red. Hands on the run's id; its report names the jobs that failed
#
# Input, from AMENBO_INPUT's `ins`: the version, without the leading `v` (required).
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read or do
# — no version, a checkout not on main or not even with origin/main, a tag that already exists, a watch
# that broke, a run that ended without a verdict — ends it by its error way out, with the reason as its
# report.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in. The tag
# is made there, and `gh` reads the repository from it (AMENBO_CI_REPO overrides it, as in
# watch-ci.sh).
set -euo pipefail

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

version=$(input_version)
tag="v$version"
resolve_repo

[ "$(git branch --show-current)" = main ] || die "チェックアウト $(pwd) が main にいません"
git fetch origin -q || die "origin を fetch できませんでした"
git pull -q --ff-only || die "main を origin/main へ fast-forward できませんでした"
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || die "main が origin/main と一致していません"
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null || git ls-remote --exit-code --tags origin "$tag" >/dev/null; then
    die "タグ $tag は既にあります"
fi

git tag -a "$tag" -m "amenbo $version" || die "タグ $tag を付けられませんでした"
git push -q origin "$tag" || die "$tag を push できませんでした"
echo "pushed $tag at $(git rev-parse --short HEAD)" >&2

id=$(resolve_run tag "$tag")
verdict=$(watch_run "$id")
outs=$(jq -n --arg r "$id" '{run: $r}')
if [ "$verdict" = green ]; then
    leave 完了 "$outs" "$tag の run は緑です：$(run_url "$id")"
fi
leave "CI が赤" "$outs" "$tag の run は赤です（$(failed_jobs "$id")）：$(run_url "$id")"
