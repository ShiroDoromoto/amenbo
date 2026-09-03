#!/usr/bin/env bash
# check-token-contrast.sh — hold the colour tokens to the contrast they were chosen for.
#
# Every `--c-*` value is set so that both themes clear WCAG AA: 4.5:1 for anything read as text,
# 3.0:1 for the outline of a control. Those numbers were worked out by hand, once, and nothing has
# held them since. A theme swaps a dozen values at a time, and a colour nudged for one screen lands
# on every ground the token is used over — so the value that breaks AA is rarely the one being
# looked at while it is edited.
#
# Nothing goes red when that happens. The screen still renders, the tests still pass, and the loss
# is that some readers can no longer read it — which no build can notice on its own. So it is asked
# here instead: read `app/src/styles/tokens.css`, compute every pairing a token is used in, and fail
# on the first one that falls under its floor.
#
# Three pairings, because those are the three a token is read in:
#   - a reading ink over each of the four grounds  (4.5:1)
#   - a fill and the ink that fill carries          (4.5:1)
#   - a control's outline over each ground          (3.0:1)
#
# `--c-text-faint` is deliberately absent from the inks: it is not reading ink — it draws separators
# and the pale side of an icon, neither of which is read.
#
# Usage: guards/check-token-contrast.sh   (no args; reads the token file)
# Exit codes: 0 = every pairing clears its floor, 1 = one fell under it, or a token it names is gone.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

tokens=app/src/styles/tokens.css

if [ ! -f "$tokens" ]; then
  echo "✗ token contrast: $tokens is missing — did the tokens move?" >&2
  exit 1
fi

python3 - "$tokens" <<'PY'
import re, sys

path = sys.argv[1]
src = open(path).read()

# The two themes, as the file writes them: light is the `:root` default and dark overrides a subset
# of it, so dark is read as light with those overrides laid on top.
LIGHT_SELECTOR = ":root"
DARK_SELECTOR = '[data-theme="dark"]'

# What is read as text. Kept as a list rather than derived from the file: a token's floor follows
# from the job it does, and the file does not say which job that is. A name added here that the file
# no longer holds fails below rather than being skipped.
INKS = [
    "--c-text", "--c-text-muted", "--c-accent", "--c-accent-text",
    "--c-done", "--c-human", "--c-ai", "--c-stop", "--c-heed",
]
# The grounds an ink can land on — the three steps and the hover laid over them.
GROUNDS = ["--c-bg", "--c-surface", "--c-sunken", "--c-hover"]
# A fill and the ink it carries; the pair is read together and travels together.
FILLS = [("--c-accent", "--c-on-accent"), ("--c-heed", "--c-on-heed"), ("--c-stop", "--c-on-stop")]
# The outline of a control, which has to be found before the control can be used.
OUTLINE = "--c-edge"

TEXT_FLOOR = 4.5
OUTLINE_FLOOR = 3.0


def declarations(selector):
    """The `--x: value;` pairs of one rule block."""
    at = src.find(selector)
    if at < 0:
        print(f"✗ token contrast: {path} has no `{selector}` block — did the themes move?",
              file=sys.stderr)
        sys.exit(1)
    open_at = src.index("{", at)
    close_at = src.index("}", open_at)
    return dict(re.findall(r"(--[a-z0-9-]+)\s*:\s*([^;]+);", src[open_at + 1:close_at]))


light = declarations(LIGHT_SELECTOR)
dark = dict(light)
dark.update(declarations(DARK_SELECTOR))

ALIAS = re.compile(r"^var\((--[a-z0-9-]+)\)$")
HEX = re.compile(r"^#[0-9a-fA-F]{6}$")


def colour(theme, name, seen=None):
    """One token's value as a hex string, following `var(--other)` to what it stands for."""
    seen = seen or []
    if name in seen:
        sys.exit(f"✗ token contrast: {' -> '.join(seen + [name])} is a loop of aliases.")
    if name not in theme:
        sys.exit(f"✗ token contrast: {name} is not in {path}.\n"
                 f"    This guard names it, so a rename here is a pairing nobody checks any more. "
                 f"Follow the rename, or take the token out of the guard if its job is gone.")
    value = theme[name].strip()
    alias = ALIAS.match(value)
    if alias:
        return colour(theme, alias.group(1), seen + [name])
    if not HEX.match(value):
        sys.exit(f"✗ token contrast: {name} is `{value}`, which this guard cannot read as a colour.\n"
                 f"    It reads a 6-digit hex, or a `var(--other)` standing for one.")
    return value


def luminance(hex_colour):
    """Relative luminance, as WCAG 2.x defines it."""
    channels = [int(hex_colour[i:i + 2], 16) / 255 for i in (1, 3, 5)]
    linear = [c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4 for c in channels]
    return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]


def contrast(a, b):
    la, lb = luminance(a), luminance(b)
    return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)


failures = []
checked = 0

for theme_name, theme in (("light", light), ("dark", dark)):
    for ink in INKS:
        for ground in GROUNDS:
            ratio = contrast(colour(theme, ink), colour(theme, ground))
            checked += 1
            if ratio < TEXT_FLOOR:
                failures.append(f"{theme_name}: {ink} on {ground} is {ratio:.2f}, under {TEXT_FLOOR}")
    for fill, on_fill in FILLS:
        ratio = contrast(colour(theme, fill), colour(theme, on_fill))
        checked += 1
        if ratio < TEXT_FLOOR:
            failures.append(f"{theme_name}: {on_fill} on {fill} is {ratio:.2f}, under {TEXT_FLOOR}")
    for ground in GROUNDS:
        ratio = contrast(colour(theme, OUTLINE), colour(theme, ground))
        checked += 1
        if ratio < OUTLINE_FLOOR:
            failures.append(f"{theme_name}: {OUTLINE} on {ground} is {ratio:.2f}, under {OUTLINE_FLOOR}")

if failures:
    for line in failures:
        print(f"✗ token contrast: {line}", file=sys.stderr)
    print(f"    Move the value in {path} until it clears the floor, rather than lowering the floor "
          f"— AA is what the palette was built to.", file=sys.stderr)
    sys.exit(1)

print(f"✓ token contrast: all {checked} pairings clear AA in both themes")
PY
