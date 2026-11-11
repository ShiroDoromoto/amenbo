#!/usr/bin/env bash
# verify-pkg-signature.sh — a script step: see that the downloaded mac .pkg files are signed and
# notarised, by reading the files. Nothing is installed.
#
# On `dist-macos/amenbo-darwin-arm64.pkg` in the place it runs three checks, each written to a log of
# its own in the place:
#
#   pkgutil --check-signature   prints `Notarization: trusted by the Apple notary service`
#   spctl -a -vvv -t install    prints `accepted` and `source=Notarized Developer ID`
#   xcrun stapler validate      prints `The validate action worked!`
#
# From `dist-macos/amenbo-darwin-amd64.pkg` it takes the CLI out with scripts/extract-shipped-cli.sh
# into `cli-amd64/amenbo` in the place, and `lipo -archs` must answer `x86_64`. The step names its
# ways out and ports in Japanese, so the names below are the ones written in the code:
#
#   done   all four hold
#   red    one does not. Hands on, as a file beside output.json, the checks that failed and their
#          logs
#
# Input, from AMENBO_INPUT's `ins`: the place the artifacts were downloaded to (required).
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read —
# no place, no .pkg in it — ends it with exit code 1, which the step leaves through its error way
# out.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in.
set -euo pipefail

RED_FILE="署名と notarize を確かめる.md"

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

place=$(input_place)
arm="$place/dist-macos/amenbo-darwin-arm64.pkg"
amd="$place/dist-macos/amenbo-darwin-amd64.pkg"
[ -f "$arm" ] || die "$arm がありません"
[ -f "$amd" ] || die "$amd がありません"

wrong=()

# Run one check into its log, and say whether every text it must print is there.
check() {
    local label="$1" log="$place/$2"
    shift 2
    local texts=()
    while [ "$1" != -- ]; do
        texts+=("$1")
        shift
    done
    shift
    "$@" > "$log" 2>&1 || true
    local text
    for text in "${texts[@]}"; do
        grep -qF -- "$text" "$log" || wrong+=("${label}：$log に \"$text\" がありません")
    done
}

check "pkgutil --check-signature" pkgutil-check-signature.log \
    'Notarization: trusted by the Apple notary service' -- pkgutil --check-signature "$arm"
check "spctl -a -vvv -t install" spctl.log \
    'accepted' 'source=Notarized Developer ID' -- spctl -a -vvv -t install "$arm"
check "xcrun stapler validate" stapler.log \
    'The validate action worked!' -- xcrun stapler validate "$arm"

mkdir -p "$place/cli-amd64"
cli="$place/cli-amd64/amenbo"
if scripts/extract-shipped-cli.sh "$amd" "$cli" > "$place/extract-amd64.log" 2>&1; then
    archs=$(lipo -archs "$cli" 2>&1) || true
    [ "$archs" = x86_64 ] || wrong+=("lipo -archs $cli が x86_64 ではなく \"$archs\" を返しました")
else
    wrong+=("$amd から CLI を取り出せませんでした：$(tail -n 1 "$place/extract-amd64.log")")
fi

if [ "${#wrong[@]}" = 0 ]; then
    leave 完了 '{}' "$(basename "$arm") は署名・notarize・staple 済みで、$(basename "$amd") の CLI は x86_64 です"
fi

{
    echo "# The mac .pkg files are not signed or built as they should be"
    echo
    echo "- place: $place"
    echo
    printf -- '- %s\n' "${wrong[@]}"
} > "$out_dir/$RED_FILE"
leave_red "$RED_FILE" "mac の .pkg の確認が ${#wrong[@]} 件失敗しました：${wrong[0]}"
