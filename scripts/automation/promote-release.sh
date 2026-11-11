#!/usr/bin/env bash
# promote-release.sh — a script step: promote a release from prerelease to latest, and see that what
# the latest release serves points at it.
#
# It is run after a person has said go; asking is not this script's. It dispatches promote-manual.yml
# on main for `v<version>` and waits for that run with `watch-ci.sh dispatch`. Then it fetches
# `latest.json` and `latest-tauri.json` from the latest release, past any cache, and checks:
#
#   latest.json        `version` is the version, and each of the six keys below in its `assets` is
#                      a url that answers 200: macos-arm64-pkg, macos-x64-pkg, windows-x64-exe,
#                      windows-arm64-exe, linux-x64-appimage, linux-arm64-appimage
#   latest-tauri.json  `version` is the version, and each of the six `platforms` has a `url` that
#                      answers 200
#
# The step names its ways out in Japanese, so the name below is the one written in the code:
#
#   done   the run is green and both files hold
#
# Input, from AMENBO_INPUT's `ins`: the version, without the leading `v` (required).
#
# The way out is decided by output.json, never by this script's exit code. A red run, a file that does
# not hold, or anything it cannot read or do ends it with exit code 1, which the step leaves through
# its error way out; the last line on stderr says what. When the latest release still serves another
# version, that line also says whether `v<version>` reads as a prerelease, and which version the
# tag's own latest.json carries, to tell a promotion that did not happen from a stale cache.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, which
# is where `gh` reads the repository from (AMENBO_CI_REPO overrides it, as in watch-ci.sh).
set -euo pipefail

LATEST_KEYS=(macos-arm64-pkg macos-x64-pkg windows-x64-exe windows-arm64-exe linux-x64-appimage linux-arm64-appimage)

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

version=$(input_version)
tag="v$version"
resolve_repo

stamp=$(now_utc)
gh workflow run promote-manual.yml -R "$repo" --ref main -f "tag=$tag" || die "could not dispatch promote-manual.yml for $tag"
id=$(resolve_run dispatch promote-manual.yml main "$stamp")
verdict=$(watch_run "$id")
[ "$verdict" = green ] || die "the promotion of $tag is red ($(failed_jobs "$id")): $(run_url "$id")"

fetch() { curl -fsSL -H 'Cache-Control: no-cache' "$1?cb=$RANDOM"; }
status_of() { curl -sIL -o /dev/null -w '%{http_code}' "$1" || true; }
joined() {
    local out="$1"
    shift
    for item in "$@"; do out="$out; $item"; done
    echo "$out"
}

latest=$(fetch "https://github.com/$repo/releases/latest/download/latest.json") || die "could not fetch the latest latest.json"
tauri=$(fetch "https://github.com/$repo/releases/latest/download/latest-tauri.json") || die "could not fetch the latest latest-tauri.json"

wrong=()
for name in latest.json latest-tauri.json; do
    if [ "$name" = latest.json ]; then body="$latest"; else body="$tauri"; fi
    served=$(jq -r '.version // empty' <<< "$body") || served="(not JSON)"
    [ "${served#v}" = "$version" ] || wrong+=("$name serves version \"$served\", not $version")
done

if [ "${#wrong[@]}" != 0 ]; then
    prerelease=$(gh release view "$tag" -R "$repo" --json isPrerelease --jq .isPrerelease 2>/dev/null) || prerelease="(no release)"
    own=$(fetch "https://github.com/$repo/releases/download/$tag/latest.json" | jq -r '.version // empty') || own="(unreadable)"
    die "$(joined "${wrong[@]}"); $tag reads isPrerelease=$prerelease, and its own latest.json carries \"$own\""
fi

for key in "${LATEST_KEYS[@]}"; do
    url=$(jq -r --arg k "$key" '.assets[$k] // empty' <<< "$latest")
    if [ -z "$url" ]; then
        wrong+=("latest.json has no url for $key")
        continue
    fi
    code=$(status_of "$url")
    [ "$code" = 200 ] || wrong+=("latest.json $key answered $code: $url")
done

platforms=$(jq -r '.platforms // {} | to_entries[] | "\(.key) \(.value.url // "")"' <<< "$tauri")
count=0
while read -r platform url; do
    [ -n "$platform" ] || continue
    count=$((count + 1))
    if [ -z "$url" ]; then
        wrong+=("latest-tauri.json has no url for $platform")
        continue
    fi
    code=$(status_of "$url")
    [ "$code" = 200 ] || wrong+=("latest-tauri.json $platform answered $code: $url")
done <<< "$platforms"
[ "$count" = 6 ] || wrong+=("latest-tauri.json has $count platforms, not 6")

[ "${#wrong[@]}" = 0 ] || die "$tag is promoted ($(run_url "$id")), but: $(joined "${wrong[@]}")"

leave 完了 '{}' "promoted $tag ($(run_url "$id")); latest.json and latest-tauri.json serve $version, and their 6 + 6 urls answer 200"
