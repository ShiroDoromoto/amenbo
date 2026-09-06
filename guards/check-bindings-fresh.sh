#!/usr/bin/env bash
# check-bindings-fresh.sh — say when the generated TypeScript bindings are behind the Rust they
# come from.
#
# `app/src/bindings/bindings.ts` is written by ts-rs while the GUI crate's tests run, and it is
# committed rather than built on the way in: the front end typechecks against it without a Rust
# toolchain, and a reader opens it to see what a command answers with. That only holds while the
# committed file is what the current Rust would write.
#
# Nothing was watching that. `check-ts-derive.sh` keeps every `#[derive(TS)]` inside the GUI crate,
# which is what lets a change be judged by the layer it touched — but it says nothing about whether
# the file those derives write is up to date. A change to a DTO that nobody regenerated for is green
# everywhere: the crate compiles, its tests pass, the front end typechecks against the stale text,
# and the two only part company for whoever runs the tests next. What they get is a modified file in
# a working tree they did not touch it in, on somebody else's change, and the safe move there is to
# throw it away — which leaves it for the person after them.
#
# So this is run **straight after the tests that write it**, and reads the working tree: a file the
# run moved is a file the commit should have carried.
#
# Usage: guards/check-bindings-fresh.sh   (no args; run after `cargo test` on the GUI crate)
# Exit codes: 0 = the committed bindings are what this tree generates, 1 = they are behind.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

bindings=app/src/bindings/bindings.ts

# The sanity check first, for the reason `check-ts-derive.sh` makes one: a guard watching a path
# that no longer exists passes forever, and passing is exactly what it would do on the day the
# generator's destination moved.
if ! git ls-files --error-unmatch "$bindings" >/dev/null 2>&1; then
    echo "✗ bindings: $bindings is not tracked — either the generator writes somewhere else now," >&2
    echo "  or the file was dropped. Fix the guard rather than deleting it." >&2
    exit 1
fi

if git diff --quiet -- "$bindings"; then
    echo "✓ bindings: $bindings is what this tree's Rust generates"
    exit 0
fi

echo "✗ bindings: $bindings moved when the tests ran, so what is committed is behind the Rust." >&2
echo >&2
git --no-pager diff -- "$bindings" >&2
echo >&2
echo "  Regenerate it and commit the result with the change that moved it:" >&2
echo "    make gate-app-rust        # or: cargo test --manifest-path app/src-tauri/Cargo.toml" >&2
echo "    git add $bindings" >&2
echo >&2
echo "  It belongs in the same commit as the Rust it comes from — left out, it lands in the working" >&2
echo "  tree of whoever runs the tests next, on a change of theirs that has nothing to do with it." >&2
exit 1
