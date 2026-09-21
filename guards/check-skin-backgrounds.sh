#!/usr/bin/env bash
# check-skin-backgrounds.sh — say when a surface is painted somewhere a skin's picture cannot reach.
#
# A skin may lay a picture behind four places, each named by the colour token drawn there
# (`BACKGROUNDS` in `crates/amenbo-core/src/skin.rs`). A place is not an element: what
# the author reaches is the surface, so the picture has to go behind **every** rule that paints in
# that colour. There are over a hundred of them, and what carries it to each is the slot beside the
# colour — `background: var(--c-surface-pic) var(--c-surface)`.
#
# A rule that paints the colour and leaves the slot out draws exactly as well as one that does not.
# Nothing is undefined, nothing is invalid, no test has anything to compare against: the card simply
# keeps this build's flat colour while every other card on the screen wears the author's paper. The
# day that happens is the day a rule is added, and the person adding it has no reason to know.
#
# So the pairing is asked here: in the front end's own stylesheets, a `background` or
# `background-color` that reads one of the four colours must read that colour's slot as well. And
# each slot has to be declared empty somewhere, or the declaration reading it is dropped whole.
#
# The other half is the properties that **cannot** carry a picture at all — `box-shadow`, `border`,
# `outline` and their kind draw a colour and nothing else. A rule that paints a place with one of
# those is a place the picture stops at, and no pairing will fix it: the rule has to be written a
# different way, or the flat colour has to be the answer on purpose. Either is a judgement somebody
# made, so what is asked here is that it was made: each such rule is named in SETTLED below with the
# reason the flat colour stands. One that is not stops the build, which is the point — the person
# writing the next one has no more reason to know than the person who wrote the last.
#
# Usage: guards/check-skin-backgrounds.sh   (no args; reads app/src and core's list)
# Exit codes: 0 = every place is painted through its slot, 1 = one of them is not.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

vocabulary=crates/amenbo-core/src/skin.rs
src=app/src

for f in "$vocabulary" "$src"; do
  if [ ! -e "$f" ]; then
    echo "✗ skin backgrounds: $f is missing — did it move?" >&2
    exit 1
  fi
done

python3 - "$vocabulary" "$src" <<'PY'
import pathlib, re, sys

vocabulary, src = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])

rust = vocabulary.read_text()
at = rust.find("pub const BACKGROUNDS: &[&str] = &[")
if at < 0:
    sys.exit(f"✗ skin backgrounds: {vocabulary} no longer declares BACKGROUNDS.")
places = re.findall(r'"([a-z0-9-]+)"', rust[at:rust.index("];", at)])
if not places:
    sys.exit(f"✗ skin backgrounds: BACKGROUNDS in {vocabulary} is empty — read again.")

sheets = sorted(src.rglob("*.css"))
if not sheets:
    sys.exit(f"✗ skin backgrounds: {src} holds no stylesheet — did the front end move?")
css = {f: f.read_text() for f in sheets}
declared = {m for text in css.values() for m in re.findall(r"(--[a-z0-9-]+)\s*:", text)}

# `background` and `background-color` both paint the ground; only the first can carry a picture,
# which is what a rule caught on the second is being told.
#
# The colour **as the whole value**, and nothing else. A token inside `color-mix()` or a gradient is
# a colour something was built out of rather than the place itself, and a picture behind one of
# those would be a picture behind a patch the author never named.
PAINTS = re.compile(r"\bbackground(-color)?\s*:\s*([^;}]*)")

# The properties that draw a colour and can hold no picture. A custom property is left out on
# purpose: `--x: var(--c-surface)` can be paired with a `--x-pic` of its own, so it belongs to the
# check above rather than this one.
FLAT = re.compile(
    r"\b(box-shadow|text-shadow|outline(-color)?|border(-top|-right|-bottom|-left)?(-color)?)"
    r"\s*:\s*([^;}]*)"
)

# Every rule that paints a place with one of those, and what was decided about it. The key is the
# selector as it is written, then the property; the value is why the flat colour stands.
SETTLED = {
    (".sidebar--compact .navitem__count", "box-shadow"):
        "the ring round the folded column's count badge — a 2px box of picture would be a crop "
        "blown up to fill it, and the ring is there for the digits to stay readable",
    (".ptabs--compact .ptabs__count", "box-shadow"):
        "the workspace's side of the same ring",
}


def selector_before(text: str, at: int) -> str:
    """The selector of the rule a declaration is in — what stands between the last `}` or `*/` and
    the `{` that opens it."""
    open_at = text.rfind("{", 0, at)
    if open_at < 0:
        return ""
    head = text[:open_at]
    ends = [(head.rfind("}"), 1), (head.rfind("*/"), 2)]
    cut, width = max(ends) if max(ends)[0] >= 0 else (-1, 1)
    return " ".join(head[cut + width:].split()).strip().rstrip("{").strip()


failures = []
for f, text in css.items():
    for m in FLAT.finditer(text):
        painted = [p for p in places if f"var(--{p})" in m.group(5)]
        if not painted:
            continue
        prop = m.group(1)
        selector = selector_before(text, m.start())
        if (selector, prop) in SETTLED:
            continue
        line = text.count("\n", 0, m.start()) + 1
        failures.append(
            f"{f}:{line} paints {', '.join(painted)} with `{prop}`, which can hold no picture.\n"
            f"    Write the rule so the colour is a `background` that carries its slot, or add "
            f"`({selector!r}, {prop!r})` to SETTLED in this guard with why the flat colour stands."
        )

for place in places:
    if f"--{place}-pic" not in declared:
        failures.append(
            f"--{place}-pic is declared in no stylesheet under {src}.\n"
            f"    Every rule reading it is dropped whole, so the place is painted in the parent's "
            f"colour rather than the author's picture. Declare it `none` beside the other three."
        )
    for f, text in css.items():
        for m in PAINTS.finditer(text):
            if m.group(2).strip() != f"var(--{place})":
                continue
            line = text.count("\n", 0, m.start()) + 1
            failures.append(
                f"{f}:{line} paints {place} without the picture a skin may lay behind it.\n"
                f"    Write it `background: var(--{place}-pic) var(--{place})`, which draws the "
                f"picture over the colour and the colour alone where there is none."
            )

if failures:
    print("✗ skin backgrounds: a place is painted somewhere a skin's picture cannot reach.\n", file=sys.stderr)
    for f in failures:
        print(f"  - {f}", file=sys.stderr)
    sys.exit(1)

print(f"✓ skin backgrounds: every rule painting one of the {len(places)} places reads its slot")
PY
