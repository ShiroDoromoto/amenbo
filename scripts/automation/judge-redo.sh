#!/usr/bin/env bash
# judge-redo.sh — a script step: measure whether a release whose check went red was handed out, and
# say how it is to be redone. It deletes nothing.
#
# It reads `gh release view v<version> --json isPrerelease`, which fails when the tag has no release,
# and the `version` of latest.json on the latest release, fetched past any cache. The way to redo is
# then the first row that holds:
#
#   the tag has no release                       delete the tag, and tag the same version again
#   a prerelease, and latest.json serves another  delete the release with its tag, and tag the same
#                                                version again
#   anything else (promoted, or latest.json      leave the tag, and release the next patch version
#   serves it)
#
# The step names its ways out and ports in Japanese, so the names below are the ones written in the
# code:
#
#   done   hands on which of the three it is; the report names what was measured and the commands
#          that redo it. When the red check's file is given, its first lines head the report
#
# Inputs, from AMENBO_INPUT's `ins`: the version, without the leading `v` (required); the red check's
# file (optional).
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read —
# no version, a latest.json that cannot be fetched, a release that cannot be read for another reason
# than its absence — ends it by its error way out, with the reason as its report.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, which
# is where `gh` reads the repository from (AMENBO_CI_REPO overrides it, as in watch-ci.sh).
set -euo pipefail

WAY_OUTPUT="打ち直し方"
DELETE_TAG="タグを消す"
DELETE_RELEASE="Release を消す"
NEXT_PATCH="次のパッチ版"

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

version=$(input_version)
tag="v$version"
red=$(input 結果)
resolve_repo

code=0
said=$(gh release view "$tag" -R "$repo" --json isPrerelease --jq .isPrerelease 2>&1) || code=$?
if [ "$code" != 0 ]; then
    [[ "$said" == *"release not found"* ]] || die "リリース $tag を読めませんでした：$said"
    prerelease=""
else
    prerelease="$said"
fi

served=$(curl -fsSL -H 'Cache-Control: no-cache' "https://github.com/$repo/releases/latest/download/latest.json?cb=$RANDOM" |
    jq -r '.version // empty') || die "最新リリースの latest.json を取得できませんでした"
served="${served#v}"

if [ -z "$prerelease" ]; then
    way="$DELETE_TAG"
    measured="$tag のリリースはありません。latest.json のバージョンは $served です"
    commands="git push origin :refs/tags/$tag && git tag -d $tag"
elif [ "$prerelease" = true ] && [ "$served" != "$version" ]; then
    way="$DELETE_RELEASE"
    measured="$tag は prerelease です。latest.json のバージョンは $served です"
    commands="gh release delete $tag --repo $repo --cleanup-tag --yes && git tag -d $tag"
else
    way="$NEXT_PATCH"
    measured="$tag は isPrerelease=$prerelease です。latest.json のバージョンは $served です"
    commands="なし：タグは残し、次のパッチバージョンを出す"
fi

report="${measured}。打ち直し方：${way}（${commands}）"
if [ -n "$red" ] && [ -f "$red" ]; then
    report=$(printf '%s\n\n赤になった確認：\n%s' "$report" "$(head -n 20 "$red")")
fi
leave 完了 "$(jq -n --arg k "$WAY_OUTPUT" --arg w "$way" '{($k): $w}')" "$report"
