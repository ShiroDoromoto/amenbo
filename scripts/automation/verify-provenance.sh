#!/usr/bin/env bash
# verify-provenance.sh — a script step: see that the downloaded mac arm64 .pkg was built by this
# repository's release workflow, from the commit its tag points at.
#
# It runs `gh attestation verify` on `dist-macos/amenbo-darwin-arm64.pkg` in the place, writes its
# JSON to `attestation.json` there, and reads the certificate of the first result. Three things must
# hold: the source repository is this one, the source digest is the commit `v<version>` points at on
# GitHub, and the build signer ends with `_release.yml@refs/tags/v<version>`. The step names its ways
# out and ports in Japanese, so the names below are the ones written in the code:
#
#   done   all three hold
#   red    the verification failed, or one of the three does not hold. Hands on, as a file beside
#          output.json, what was read and what was wanted
#
# Inputs, from AMENBO_INPUT's `ins`: the version, without the leading `v`, and the place the artifacts
# were downloaded to (both required).
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read —
# no version, no place, no .pkg in it, a tag GitHub does not have — ends it with exit code 1, which
# the step leaves through its error way out.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, which
# is where `gh` reads the repository from (AMENBO_CI_REPO overrides it, as in watch-ci.sh).
set -euo pipefail

RED_FILE="来歴を確かめる.md"

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

version=$(input_version)
place=$(input_place)
pkg="$place/dist-macos/amenbo-darwin-arm64.pkg"
[ -f "$pkg" ] || die "no $pkg"
resolve_repo

sha=$(gh api "repos/$repo/commits/v$version" --jq .sha) || die "could not read the commit of v$version on $repo"
[ -n "$sha" ] || die "the tag v$version on $repo names no commit"

code=0
gh attestation verify "$pkg" --repo "$repo" --format json > "$place/attestation.json" 2> "$place/attestation.log" || code=$?

got_repo=""
got_sha=""
got_workflow=""
if [ "$code" = 0 ]; then
    cert='.[0].verificationResult.signature.certificate'
    got_repo=$(jq -r "$cert.sourceRepositoryURI // empty" "$place/attestation.json") || die "could not read $place/attestation.json"
    got_sha=$(jq -r "$cert.sourceRepositoryDigest // empty" "$place/attestation.json")
    got_workflow=$(jq -r "$cert.buildSignerURI // empty" "$place/attestation.json")
fi

wrong=()
[ "$code" = 0 ] || wrong+=("gh attestation verify ended with exit code $code: $(tail -n 1 "$place/attestation.log")")
[ "$code" != 0 ] || [ "$got_repo" = "https://github.com/$repo" ] ||
    wrong+=("repo is \"$got_repo\", not https://github.com/$repo")
[ "$code" != 0 ] || [ "$got_sha" = "$sha" ] || wrong+=("sha is \"$got_sha\", not $sha (v$version)")
[ "$code" != 0 ] || [[ "$got_workflow" == *"_release.yml@refs/tags/v$version" ]] ||
    wrong+=("workflow is \"$got_workflow\", which does not end with _release.yml@refs/tags/v$version")

if [ "${#wrong[@]}" = 0 ]; then
    leave 完了 '{}' "the provenance of $(basename "$pkg") holds: $repo at $sha, signed by $got_workflow"
fi

{
    echo "# The provenance of $(basename "$pkg") does not hold"
    echo
    echo "- pkg: $pkg"
    echo "- attestation: $place/attestation.json"
    echo "- log: $place/attestation.log"
    echo
    printf -- '- %s\n' "${wrong[@]}"
} > "$out_dir/$RED_FILE"
leave_red "$RED_FILE" "the provenance of $(basename "$pkg") does not hold: ${wrong[0]}"
