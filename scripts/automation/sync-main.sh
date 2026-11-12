#!/usr/bin/env bash
# sync-main.sh — a script step: bring the checkout's main up to origin/main.
#
# Script steps run the scripts in the local checkout, so a merged fix to one of them only takes effect
# once that checkout's main has been brought up to origin/main. This step does that, fast-forward only.
# The step names its ways out in Japanese, so the name below is the one written in the code:
#
#   done            main is even with origin/main. The report names the commit before and after
#
# No input, no output.
#
# The way out is decided by output.json, never by this script's exit code. A checkout not on main, a
# working tree with changes to tracked files, or a main that cannot be fast-forwarded ends it by its
# error way out, with the reason as its report. Untracked files are not counted as changes: a
# fast-forward leaves them alone, and if one would be overwritten the pull fails and the step stops.
#
# Amenbo starts a script step in no particular folder, so it moves to the checkout it sits in.
set -euo pipefail

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/automation/step-lib.sh
. scripts/automation/step-lib.sh

[ "$(git branch --show-current)" = main ] || die "チェックアウト $(pwd) が main にいません"
[ -z "$(git status --porcelain --untracked-files=no)" ] || die "チェックアウト $(pwd) の作業ツリーに変更があります"

before=$(git rev-parse --short HEAD)
git pull -q --ff-only origin main || die "main を origin/main へ fast-forward できませんでした"
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || die "main が origin/main と一致していません"
after=$(git rev-parse --short HEAD)

if [ "$before" = "$after" ]; then
    leave 完了 '{}' "main は既に origin/main に追いついていました：$after"
fi
leave 完了 '{}' "main を origin/main に追いつかせました：$before → $after"
