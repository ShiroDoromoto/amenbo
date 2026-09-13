#!/usr/bin/env bash
# check-notify-wording-fresh.sh — say when the baked notification wording is behind the dictionaries
# it comes from.
#
# `crates/amenbo-core/src/notify_wording_table.rs` is written by `scripts/gen-notify-wording.mjs` out
# of `app/src/core/i18n/locales`, and it is committed rather than built on the way in: core reads
# files, not TypeScript modules, and nothing in a build should wait on node.
#
# That only holds while the committed table is what the current dictionaries would write. Nothing
# else would notice: a corrected sentence in `ja.ts` shows up on screen immediately and in a
# notification never, and both sides compile and pass their own tests while they disagree. The two
# plugins this replaced each carried a dictionary of their own, and keeping them in step by hand is
# exactly what stopped working.
#
# So the generator is run and the working tree read: a file the run moved is a file the commit should
# have carried.
#
# Skipped, quietly, where node is not installed — the same terms the other generators are on.
#
# Usage: guards/check-notify-wording-fresh.sh   (no args)
# Exit codes: 0 = the committed table is what the dictionaries generate, 1 = it is behind.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

baked=crates/amenbo-core/src/notify_wording_table.rs

# The sanity check first: a guard watching a path that no longer exists passes forever, and passing
# is exactly what it would do on the day the generator's destination moved.
if ! git ls-files --error-unmatch "$baked" > /dev/null 2>&1; then
    echo "✗ notify wording: $baked is not tracked — either the generator writes somewhere else now," >&2
    echo "  or the file was dropped. Fix the guard rather than deleting it." >&2
    exit 1
fi

if ! command -v node > /dev/null 2>&1; then
    echo "→ node not installed — the notification wording guard is skipped"
    exit 0
fi

# A tree that was already dirty here cannot be judged by this: what the run leaves would be somebody's
# own edit rather than the generator's verdict. Say so instead of blaming the change under test.
if ! git diff --quiet -- "$baked"; then
    echo "✗ notify wording: $baked was already modified before this ran, so what the generator" >&2
    echo "  writes cannot be told from what was edited by hand. Commit or discard it first." >&2
    exit 1
fi

node scripts/gen-notify-wording.mjs > /dev/null

if git diff --quiet -- "$baked"; then
    echo "✓ notify wording: $baked is what the dictionaries generate"
    exit 0
fi

echo "✗ notify wording: $baked is behind app/src/core/i18n/locales." >&2
echo >&2
git --no-pager diff -- "$baked" >&2
echo >&2
echo "  Regenerate it and commit the result with the dictionary change that moved it:" >&2
echo "    make notify-wording" >&2
echo "    git add $baked" >&2
echo >&2
echo "  It belongs in the same commit as the sentence it comes from — left out, a notification goes" >&2
echo "  on saying what the dictionary no longer says." >&2
exit 1
