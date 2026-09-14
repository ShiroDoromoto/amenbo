#!/usr/bin/env bash
# check-worker-baked-fresh.sh — say when the baked copy of the Viewer's Worker is behind the source
# it comes from.
#
# `crates/amenbo-core/src/viewer/worker.js` and the migrations beside it are written by
# `make -C worker baked` out of `worker/src` and `worker/migrations`, and they are committed rather
# than built on the way in: core embeds them with `include_str!`, so every `cargo build` would
# otherwise have to produce them first — including the ones that run in a container over a read-only
# tree, and the ones on a machine with no network.
#
# That only holds while the committed copy is what the current source would write. What watches it
# otherwise is the Worker's own `BUILD` number, and that is a number somebody remembers to raise: an
# edit to `src/index.ts` that leaves it alone is green everywhere — the TypeScript compiles, the
# Worker's tests pass against the source, core compiles against the older copy, and what a user's
# account gets is the Worker from a commit nobody meant to deploy.
#
# So the bake is run and the working tree read: a file the run moved is a file the commit should have
# carried. Untracked counts too — a new migration is a file the bake writes and nothing else would
# notice was left out.
#
# Skipped, quietly, where node is not installed — the same terms the other generators are on.
#
# Usage: guards/check-worker-baked-fresh.sh   (no args)
# Exit codes: 0 = the committed copy is what the source bakes, 1 = it is behind.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

script=crates/amenbo-core/src/viewer/worker.js
migrations=crates/amenbo-core/src/viewer/migrations

# The sanity check first: a guard watching a path that no longer exists passes forever, and passing
# is exactly what it would do on the day the bake's destination moved.
if ! git ls-files --error-unmatch "$script" > /dev/null 2>&1; then
    echo "✗ worker baked: $script is not tracked — either the bake writes somewhere else now," >&2
    echo "  or the file was dropped. Fix the guard rather than deleting it." >&2
    exit 1
fi
if [ -z "$(git ls-files -- "$migrations")" ]; then
    echo "✗ worker baked: $migrations holds no tracked file — either the bake writes somewhere" >&2
    echo "  else now, or they were dropped. Fix the guard rather than deleting it." >&2
    exit 1
fi

if ! command -v node > /dev/null 2>&1; then
    echo "→ node not installed — the baked Worker guard is skipped"
    exit 0
fi

# What the bake wrote, tracked or not: `git diff` alone cannot see a migration that was baked and
# never added, which is the one a database would silently never be given.
moved() {
    git status --porcelain --untracked-files=all -- "$script" "$migrations"
}

# A tree that was already dirty here cannot be judged by this: what the run leaves would be somebody's
# own edit rather than the bake's verdict. Say so instead of blaming the change under test.
if [ -n "$(moved)" ]; then
    echo "✗ worker baked: the baked copy was already modified before this ran, so what the bake" >&2
    echo "  writes cannot be told from what was edited by hand. Commit or discard it first." >&2
    exit 1
fi

make -C worker baked > /dev/null

if [ -z "$(moved)" ]; then
    echo "✓ worker baked: $script and $migrations are what worker/ bakes"
    exit 0
fi

echo "✗ worker baked: the copy core embeds is behind worker/src and worker/migrations." >&2
echo >&2
moved >&2
git --no-pager diff -- "$script" "$migrations" >&2
echo >&2
echo "  Re-bake it and commit the result with the change that moved it:" >&2
echo "    make worker-baked" >&2
echo "    git add $script $migrations" >&2
echo >&2
echo "  It belongs in the same commit as the TypeScript it comes from — left out, what amenbo" >&2
echo "  deploys into somebody's own account is the Worker from an older commit." >&2
exit 1
