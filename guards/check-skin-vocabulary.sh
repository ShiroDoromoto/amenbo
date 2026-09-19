#!/usr/bin/env bash
# check-skin-vocabulary.sh — hold the names a skin may set against the tokens that exist.
#
# A skin names tokens. Which of them it may move is a judgement made once, per token: a colour is a
# taste, and the two that say which service a notification goes to are not. The answer lives in
# `crates/amenbo-core/src/skin.rs` as two lists, because the check that applies it is compiled into
# a binary and cannot read the stylesheet at run time.
#
# Two lists and a stylesheet drift in both directions, and neither shows. A token added to
# `tokens.css` and named in neither list is silently un-settable — a skin that names it is told the
# name is unknown, which is not what happened. A name left on a list after its token is gone is a
# permission over nothing, and reads as coverage.
#
# So the tokens are counted here: every `--x` declared in the stylesheet's `:root` must appear in
# exactly one of the two lists, and every name on a list must be a token that exists. Adding a token
# turns this red until somebody says which side it is on, which is the whole point — the choice is
# made by a person, once, rather than defaulted to by whichever list was easier to reach.
#
# Usage: guards/check-skin-vocabulary.sh   (no args; reads the two files)
# Exit codes: 0 = the three agree, 1 = one of them has a name the others do not.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

tokens=app/src/styles/tokens.css
vocabulary=crates/amenbo-core/src/skin.rs

for f in "$tokens" "$vocabulary"; do
  if [ ! -f "$f" ]; then
    echo "✗ skin vocabulary: $f is missing — did it move?" >&2
    exit 1
  fi
done

python3 - "$tokens" "$vocabulary" <<'PY'
import re, sys

tokens_path, vocabulary_path = sys.argv[1], sys.argv[2]
css = open(tokens_path).read()
rust = open(vocabulary_path).read()

# The light theme's block is where every token is declared; dark only overrides a subset of it, so
# reading `:root` alone is reading the whole vocabulary.
root_at = css.index(":root")
open_at = css.index("{", root_at)
close_at = css.index("}", open_at)
declared = {m.group(1) for m in re.finditer(r"--([a-z0-9-]+)\s*:", css[open_at + 1:close_at])}


def names(const):
    """The string literals of one `pub const NAME: &[&str] = &[ … ];` list."""
    at = rust.find(f"pub const {const}: &[&str] = &[")
    if at < 0:
        sys.exit(f"✗ skin vocabulary: {vocabulary_path} no longer declares {const}.")
    end = rust.index("];", at)
    return {m.group(1) for m in re.finditer(r'"([a-z0-9-]+)"', rust[at:end])}


opened, closed = names("OPEN"), names("CLOSED")

failures = []
for name in sorted(opened & closed):
    failures.append(f"{name} is on both OPEN and CLOSED — a token is one or the other")
for name in sorted(declared - opened - closed):
    failures.append(
        f"--{name} is declared in {tokens_path} and on neither list.\n"
        f"    Say whether a skin may move it: OPEN if it is a matter of taste, CLOSED if the value "
        f"says something (which service, whose mark, a width a person drags, a floor)."
    )
for name in sorted((opened | closed) - declared):
    failures.append(
        f"{name} is on a list in {vocabulary_path} but is not declared in {tokens_path}.\n"
        f"    It was renamed or removed; follow it, so the list does not read as coverage."
    )

if failures:
    for line in failures:
        print(f"✗ skin vocabulary: {line}", file=sys.stderr)
    sys.exit(1)

print(f"✓ skin vocabulary: all {len(declared)} tokens are settled ({len(opened)} open, {len(closed)} closed)")
PY
