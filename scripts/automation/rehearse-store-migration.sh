#!/usr/bin/env bash
# rehearse-store-migration.sh — a script step: see that the CLI the downloaded mac arm64 .pkg ships
# can open a clone of the store on this machine and read it back.
#
# It runs scripts/open-existing-store.sh on `dist-macos/amenbo-darwin-arm64.pkg` in the place, which
# takes the CLI out of the .pkg, clones the store into a throwaway folder and runs `doctor`, `status`
# and `project list` there. The real store is never written to. Its output is written to
# `open-existing-store.log` in the place. The clone is kept until the store's format version is read
# from it with the same CLI (`version --json`), and removed after. The step names its ways out and
# ports in Japanese, so the names below are the ones written in the code:
#
#   done   the clone opened and read back; the report names its format version. When this machine
#          has no store, there is nothing to open, and the report says so
#   red    it did not. Hands on, as a file beside output.json, the end of the log
#
# Input, from AMENBO_INPUT's `ins`: the place the artifacts were downloaded to (required).
#
# The way out is decided by output.json, never by this script's exit code. Anything it cannot read —
# no place, no .pkg in it — ends it by its error way out, with the reason as its report.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in.
set -euo pipefail

RED_FILE="移行リハーサル.md"

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

place=$(input_place)
pkg="$place/dist-macos/amenbo-darwin-arm64.pkg"
[ -f "$pkg" ] || die "$pkg がありません"
log="$place/open-existing-store.log"

code=0
KEEP=1 scripts/open-existing-store.sh "$pkg" > "$log" 2>&1 || code=$?

# The folder the clone and the CLI were kept in, as open-existing-store.sh names it.
kept=$(sed -n 's/^  (KEEP=1: left \(.*\) in place)$/\1/p' "$log" | tail -n 1)
format=""
if [ -n "$kept" ] && [ -d "$kept" ]; then
    if [ "$code" = 0 ] && [ -x "$kept/amenbo" ] && [ -d "$kept/home" ]; then
        format=$(AMENBO_HOME="$kept/home" AMENBO_UPDATE_CHECK=0 "$kept/amenbo" version --json --actor human |
            jq -r '.format_version // empty') || format=""
    fi
    rm -rf "$kept"
fi

if [ "$code" = 0 ]; then
    if grep -q 'nothing to do' "$log"; then
        leave 完了 '{}' "このマシンにはストアが無いので、開くものがありませんでした：$(grep 'nothing to do' "$log")"
    fi
    [ -n "$format" ] || die "複製は開けましたが、フォーマットのバージョンを読めませんでした：$log"
    leave 完了 '{}' "$(basename "$pkg") に入っている CLI で、ストアの複製を開いて読み戻せました。ストアのフォーマットは v$format です"
fi

{
    echo "# The shipped CLI could not open the store"
    echo
    echo "- pkg: $pkg"
    echo "- open-existing-store.sh ended with exit code $code"
    echo "- log: $log"
    echo
    echo '```'
    tail -n 40 "$log"
    echo '```'
} > "$out_dir/$RED_FILE"
leave_red "$RED_FILE" "$(basename "$pkg") に入っている CLI で、ストアの複製を開けませんでした：$(grep '✗' "$log" | tail -n 1)"
