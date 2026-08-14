#!/usr/bin/env bash
# check-ts-derive.sh — keep the generator of the TypeScript bindings inside one crate.
#
# `app/src/bindings/bindings.ts` is written by ts-rs at test time, from the types that carry
# `#[derive(TS)]`. Today every one of them sits in the GUI crate (`app/src-tauri`), and that is what
# lets a change be judged by the layer it touched: a change confined to `crates/` cannot move the
# bindings, so the front end does not have to be rebuilt and typechecked to know it still matches.
#
# The moment a derive grows in `crates/`, that reasoning is false — the generated file moves on a
# change nothing on the GUI side is watching, and the mismatch surfaces later as typecheck errors
# that name a file nobody edited. Nothing else notices: the derive compiles, the crate's own tests
# pass, and the layer judgment keeps quietly excusing the side that broke.
#
# So the invariant is held where it can be seen at all — in the source: the ts-rs dependency is
# declared by one manifest, and the derive appears under one directory.
#
# Usage: guards/check-ts-derive.sh   (no args)
# Exit codes: 0 = the generator is confined to the GUI crate, 1 = it has spread.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

home=app/src-tauri

[ -d "$home" ] || { echo "✗ ts derive: $home is missing — did the GUI crate move?" >&2; exit 1; }

# The derive as it is written in this tree: a single-line attribute listing TS among the traits.
# The neighbouring characters are spelled out because git grep has no word boundary — without them
# a trait merely starting with those two letters would answer for one that does not exist.
derive='#\[derive\([^]]*[ ,(]TS[,)]'

# The sanity check first: a pattern that matches nothing would pass the scans below forever. If the
# home crate has stopped matching, the shapes this guard reads by hand have changed.
if ! git grep -qE "$derive" -- "$home" || ! git grep -qE '^[[:space:]]*ts-rs[[:space:]]*=' -- "$home/Cargo.toml"; then
    echo "✗ ts derive: nothing in $home matches what this guard looks for — either the generator" >&2
    echo "  left the crate entirely, or the shape it is written in changed. Fix the guard rather" >&2
    echo "  than deleting it." >&2
    exit 1
fi

strays=$(git grep -nE '^[[:space:]]*ts-rs[[:space:]]*=' -- '*Cargo.toml' ":(exclude)$home/Cargo.toml" || true)
if [ -n "$strays" ]; then
    echo "✗ ts derive: a manifest outside $home depends on ts-rs." >&2
    echo "$strays" >&2
    echo "  Declare the type in the GUI crate's own DTO module instead — a generator in another crate" >&2
    echo "  moves bindings.ts on changes the GUI's own gates never see." >&2
    exit 1
fi

strays=$(git grep -nE "$derive|ts_rs" -- '*.rs' ":(exclude)$home/**" || true)
if [ -n "$strays" ]; then
    echo "✗ ts derive: a Rust file outside $home reaches for ts-rs." >&2
    echo "$strays" >&2
    echo "  Keep the derive in the GUI crate: bindings.ts is generated from there, and a change" >&2
    echo "  confined to crates/ is judged not to touch it." >&2
    exit 1
fi

echo "✓ ts derive: the TypeScript bindings are generated from $home alone"
