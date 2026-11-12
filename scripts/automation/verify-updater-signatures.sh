#!/usr/bin/env bash
# verify-updater-signatures.sh — a script step: see that the files the app's self-update downloads
# carry a minisign signature the key built into the app accepts.
#
# The key is the updater's public key in app/src-tauri/tauri.conf.json, decoded to `updater-pub.key`
# in the place. The files, in the place, are six:
#
#   dist-macos                     amenbo-darwin-arm64-update.app.tar.gz, amenbo-darwin-amd64-update.app.tar.gz
#   dist-windows, dist-windows-arm amenbo-app-windows-*-setup.exe
#   dist-linux, dist-linux-arm     amenbo-app-linux-*.AppImage
#
# Each one's `.sig` beside it is decoded to `<file>.minisig` in the place, and `minisign -V` must print
# `Signature and comment signature verified`. For the Windows and Linux files, the release's digest of
# the name must also be the same as the digest of the name with `-update` before its extension, read
# from `release-digests.txt` in the place. The step names its ways out and ports in Japanese, so the
# names below are the ones written in the code:
#
#   done   all six hold
#   red    one does not. Hands on, as a file beside output.json, the files that failed and why
#
# Input, from AMENBO_INPUT's `ins`: the place the artifacts were downloaded to (required).
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read —
# no place, no key, no release-digests.txt — ends it by its error way out, with the reason as its report.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in, where
# tauri.conf.json is read from.
set -euo pipefail

RED_FILE="updater の署名を確かめる.md"

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

place=$(input_place)
digests="$place/release-digests.txt"
[ -f "$digests" ] || die "$digests がありません"
key="$place/updater-pub.key"
jq -r '.plugins.updater.pubkey // empty' app/src-tauri/tauri.conf.json | base64 -d > "$key" ||
    die "app/src-tauri/tauri.conf.json から updater の公開鍵を読めませんでした"
[ -s "$key" ] || die "app/src-tauri/tauri.conf.json に updater の公開鍵がありません"

targets=(
    "$place/dist-macos/amenbo-darwin-arm64-update.app.tar.gz"
    "$place/dist-macos/amenbo-darwin-amd64-update.app.tar.gz"
)
for dir in dist-windows dist-windows-arm; do
    for f in "$place/$dir"/amenbo-app-windows-*-setup.exe; do targets+=("$f"); done
done
for dir in dist-linux dist-linux-arm; do
    for f in "$place/$dir"/amenbo-app-linux-*.AppImage; do targets+=("$f"); done
done

digest_of() { awk -v n="$1" '$2 == n { print $1; exit }' "$digests"; }

ok=0
wrong=()
for file in "${targets[@]}"; do
    base=$(basename "$file")
    if [ ! -f "$file" ]; then
        wrong+=("${file#"$place"/}：ダウンロードされていません")
        continue
    fi
    if [ ! -f "$file.sig" ]; then
        wrong+=("${file#"$place"/}：隣に .sig がありません")
        continue
    fi
    sig="$place/$base.minisig"
    if ! base64 -d < "$file.sig" > "$sig"; then
        wrong+=("${file#"$place"/}：.sig が base64 ではありません")
        continue
    fi
    said=$(minisign -Vm "$file" -x "$sig" -p "$key" 2>&1) || true
    if [[ "$said" != *"Signature and comment signature verified"* ]]; then
        wrong+=("${file#"$place"/}：minisign の出力は \"$(tail -n 1 <<< "$said")\" でした")
        continue
    fi
    case "$base" in
        *.exe | *.AppImage)
            update="${base%.*}-update.${base##*.}"
            fresh=$(digest_of "$base")
            again=$(digest_of "$update")
            if [ -z "$fresh" ] || [ "$fresh" != "$again" ]; then
                wrong+=("${file#"$place"/}：リリースの $base のダイジェスト（\"$fresh\"）が $update のもの（\"$again\"）と違います")
                continue
            fi
            ;;
    esac
    ok=$((ok + 1))
done

if [ "${#wrong[@]}" = 0 ] && [ "$ok" = 6 ]; then
    leave 完了 '{}' "6 個のファイルの updater の署名を、アプリに入っている鍵で確かめました"
fi
[ "${#wrong[@]}" != 0 ] || wrong+=("見つかったファイルが 6 個ではなく $ok 個です")

{
    echo "# The updater signatures do not all verify"
    echo
    echo "- place: $place"
    echo "- key: $key"
    echo "- $ok of 6 verified"
    echo
    printf -- '- %s\n' "${wrong[@]}"
} > "$out_dir/$RED_FILE"
leave_red "$RED_FILE" "updater のファイル 6 個のうち、署名を確かめられたのは $ok 個です：${wrong[0]}"
