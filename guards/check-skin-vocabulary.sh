#!/usr/bin/env bash
# check-skin-vocabulary.sh — hold core's copy of the stylesheet against the stylesheet.
#
# Two copies, for two questions: which names a skin may set, and what this build's own value for
# each of them is. Both are compiled into the binary, and both are read from the same file here.
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
# The second copy is the values themselves — `skin_contrast::BASE` for the colours, `PLAIN` for the
# open names that carry none, `SIZED` for the ones a multiplier moves. The contrast check reads the
# first for every name a skin leaves alone, and the template writes all three. A value moved in the
# stylesheet and not there would be measured against a colour nobody is looking at, or written into
# a template at a number no screen is drawn at. Aliases are followed on both sides, so what is
# compared is the colour, not the spelling.
#
# Usage: guards/check-skin-vocabulary.sh   (no args; reads the three files)
# Exit codes: 0 = the copies agree, 1 = one of them has a name or a value the others do not.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

tokens=app/src/styles/tokens.css
vocabulary=crates/amenbo-core/src/skin.rs
palette=crates/amenbo-core/src/skin_contrast.rs

for f in "$tokens" "$vocabulary" "$palette"; do
  if [ ! -f "$f" ]; then
    echo "✗ skin vocabulary: $f is missing — did it move?" >&2
    exit 1
  fi
done

python3 - "$tokens" "$vocabulary" "$palette" <<'PY'
import re, sys

tokens_path, vocabulary_path, palette_path = sys.argv[1], sys.argv[2], sys.argv[3]
css = open(tokens_path).read()
rust = open(vocabulary_path).read()
palette_rust = open(palette_path).read()

# The light theme's block is where every token is declared; dark only overrides a subset of it, so
# reading `:root` alone is reading the whole vocabulary.
def rule(selector):
    at = css.index(selector)
    open_at = css.index("{", at)
    close_at = css.index("}", open_at)
    return dict(re.findall(r"(--[a-z0-9-]+)\s*:\s*([^;]+);", css[open_at + 1:close_at]))


light = rule(":root")
dark = dict(light)
dark.update(rule('[data-theme="dark"]'))
declared = {name[2:] for name in light}

ALIAS = re.compile(r"^var\((--[a-z0-9-]+)\)$")
HEX = re.compile(r"^#[0-9a-f]{6}$")


def colour(theme, name):
    """One token's value as a hex string, following `var(--other)`; None if it is not a colour."""
    value = theme[name].strip()
    alias = ALIAS.match(value)
    if alias:
        return colour(theme, alias.group(1))
    return value if HEX.match(value) else None


def names(const):
    """The string literals of one `pub const NAME: &[&str] = &[ … ];` list."""
    at = rust.find(f"pub const {const}: &[&str] = &[")
    if at < 0:
        sys.exit(f"✗ skin vocabulary: {vocabulary_path} no longer declares {const}.")
    end = rust.index("];", at)
    return {m.group(1) for m in re.finditer(r'"([a-z0-9-]+)"', rust[at:end])}


opened, closed = names("OPEN"), names("CLOSED")

# The third answer: a token nobody writes directly, moved by a family's multiplier. The names of
# the multipliers are not tokens and must not be looked for among them.
at = rust.find("pub const SCALES: &[(&str, &[&str])] = &[")
if at < 0:
    sys.exit(f"✗ skin vocabulary: {vocabulary_path} no longer declares SCALES.")
end = rust.index("\n];", at)
scale_names = {m.group(1) for m in re.finditer(r'\("([a-z0-9-]+-scale)"', rust[at:end])}
scaled = {m.group(1) for m in re.finditer(r'"([a-z0-9-]+)"', rust[at:end])} - scale_names

failures = []
for name in sorted((opened & closed) | (opened & scaled) | (closed & scaled)):
    failures.append(f"{name} is on two lists — a token is settled one way")
for name in sorted(scale_names & declared):
    failures.append(
        f"--{name} is a multiplier's name and also a token in {tokens_path}.\n"
        f"    A multiplier is what a skin writes; a token is what it moves. One name cannot be both."
    )
for name in sorted(declared - opened - closed - scaled):
    failures.append(
        f"--{name} is declared in {tokens_path} and on no list.\n"
        f"    Say how a skin reaches it: OPEN to be written directly, SCALES to be moved by a "
        f"family's multiplier, CLOSED if the value says something (which service, whose mark, a "
        f"width a person drags, a floor)."
    )
for name in sorted((opened | closed | scaled) - declared):
    failures.append(
        f"{name} is on a list in {vocabulary_path} but is not declared in {tokens_path}.\n"
        f"    It was renamed or removed; follow it, so the list does not read as coverage."
    )

# The values, for the names that carry a colour. A token whose value is a length or a font stack is
# not one the contrast check can read a number from, and is not expected in the table.
at = palette_rust.find("const BASE: &[(&str, &str, &str)] = &[")
if at < 0:
    sys.exit(f"✗ skin vocabulary: {palette_path} no longer declares BASE.")
end = palette_rust.index("];", at)
compiled = {
    m.group(1): (m.group(2), m.group(3))
    for m in re.finditer(r'\("([a-z0-9-]+)", "(#[0-9a-f]{6})", "(#[0-9a-f]{6})"\)', palette_rust[at:end])
}
in_css = {
    name[2:]: (colour(light, name), colour(dark, name))
    for name in light
    if colour(light, name) is not None
}

for name in sorted(set(in_css) - set(compiled)):
    failures.append(
        f"--{name} is a colour in {tokens_path} and is not in {palette_path}'s BASE.\n"
        f"    The contrast check reads that table for every name a skin leaves alone."
    )
for name in sorted(set(compiled) - set(in_css)):
    failures.append(f"{name} is in {palette_path}'s BASE but is no longer a colour in {tokens_path}.")
for name in sorted(set(compiled) & set(in_css)):
    if compiled[name] != in_css[name]:
        want, got = in_css[name], compiled[name]
        failures.append(
            f"{name} is {want[0]} / {want[1]} in {tokens_path} and {got[0]} / {got[1]} in "
            f"{palette_path} (light / dark)."
        )

# The open names that carry no colour, held the way the colours are: the template writes this
# build's value beside each of them, and a value moved in the stylesheet and not there would put a
# number into a template that no screen is drawn at.
at = palette_rust.find("const PLAIN: &[(&str, &str)] = &[")
if at < 0:
    sys.exit(f"\u2717 skin vocabulary: {palette_path} no longer declares PLAIN.")
end = palette_rust.index("\n];", at)
plain = {
    m.group(1): m.group(2).replace('\\"', '"').replace("\\\\", "\\")
    for m in re.finditer(r'\("([a-z0-9-]+)", "((?:[^"\\]|\\.)*)"\)', palette_rust[at:end])
}
uncoloured = opened - set(in_css)
for name in sorted(uncoloured - set(plain)):
    failures.append(
        f"--{name} is open, carries no colour, and is not in {palette_path}'s PLAIN.\n"
        f"    The template writes a value beside every name it offers."
    )
for name in sorted(set(plain) - uncoloured):
    failures.append(f"{name} is in {palette_path}'s PLAIN and is not an open non-colour token.")
for name in sorted(set(plain) & uncoloured):
    want, dark_value = light[f"--{name}"].strip(), dark[f"--{name}"].strip()
    if want != dark_value:
        failures.append(
            f"--{name} is {want} on the light side and {dark_value} on the dark one in "
            f"{tokens_path}, and PLAIN holds one value for both."
        )
    elif plain[name] != want:
        failures.append(f"{name} is {want} in {tokens_path} and {plain[name]} in PLAIN.")

# The sizes a multiplier multiplies, the way the colours are held.
at = rust.find("const SIZED: &[(&str, &str, &str)] = &[")
if at < 0:
    sys.exit(f"✗ skin vocabulary: {vocabulary_path} no longer declares SIZED.")
end = rust.index("\n];", at)
sizes = {
    m.group(1): (m.group(2), m.group(3))
    for m in re.finditer(r'\("([a-z0-9-]+)", "([^"]+)", "([^"]+)"\)', rust[at:end])
}
for name in sorted(scaled):
    want = (light[f"--{name}"].strip(), dark[f"--{name}"].strip()) if f"--{name}" in light else None
    if name not in sizes:
        failures.append(f"{name} is moved by a multiplier and is not in {vocabulary_path}'s SIZED.")
    elif want is not None and sizes[name] != want:
        failures.append(
            f"{name} is {want[0]} / {want[1]} in {tokens_path} and {sizes[name][0]} / "
            f"{sizes[name][1]} in SIZED (light / dark)."
        )
for name in sorted(set(sizes) - scaled):
    failures.append(f"{name} is in SIZED and is not moved by any multiplier.")

if failures:
    for line in failures:
        print(f"✗ skin vocabulary: {line}", file=sys.stderr)
    sys.exit(1)

print(
    f"✓ skin vocabulary: all {len(declared)} tokens are settled ({len(opened)} open, "
    f"{len(scaled)} scaled by {len(scale_names)} multipliers, {len(closed)} closed), "
    f"and {len(compiled)} colours match the stylesheet"
)
PY
