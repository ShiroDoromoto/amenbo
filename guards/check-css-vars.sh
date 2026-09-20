#!/usr/bin/env bash
# check-css-vars.sh — say when a `var(--name)` names a custom property nothing declares.
#
# A `var()` on a name nobody declares does not fail. The declaration it sits in is dropped as
# invalid at compute time and the element inherits the parent's value instead, so the screen still
# draws, every test still passes, and what is left is a size or a colour a little off from the one
# that was written, for as long as nobody looks. `--c-muted` was that for seven rules and was found
# by hand; `--fs-display-lg` and `--fs-sm` were two more. This is the check that would have caught
# all three the day they landed.
#
# What counts as declared:
#   - a `--name:` declaration in any of `app/src`'s own stylesheets (the tokens, and a block's own
#     locals), and
#   - a name the front end sets at run time — `style.setProperty("--name", …)`, or a `"--name":`
#     key in an inline style object. Those never appear in a stylesheet and are declared all the
#     same.
#
# A fallback does not excuse a name. `var(--x, 8px)` on an `--x` nothing ever sets is not a bug the
# reader sees, but it is a `var()` that can only ever be its fallback, and the day somebody means
# to set it they will be setting a name the stylesheet does not read.
#
# A name built at run time (`var(--c-code-${kind})`) cannot be resolved here, so its prefix is asked
# for instead: at least one declared name has to start with it. That catches the rename that leaves
# the builder pointing at nothing, which is the failure worth catching — a single kind that has no
# token is the `KIND_NAMES` list's own business.
#
# Usage: guards/check-css-vars.sh   (no args; reads app/src)
# Exit codes: 0 = every name a `var()` reads is declared somewhere, 1 = one is not.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

src=app/src

if [ ! -d "$src" ]; then
  echo "✗ css vars: $src is missing — did the front end move?" >&2
  exit 1
fi

python3 - "$src" <<'PY'
import pathlib
import re
import sys

src = pathlib.Path(sys.argv[1])

STYLESHEETS = sorted(src.rglob("*.css"))
SCRIPTS = sorted(list(src.rglob("*.ts")) + list(src.rglob("*.tsx")))

if not STYLESHEETS:
    sys.exit(f"✗ css vars: no stylesheet under {src} — did they move?")

NAME = r"--[A-Za-z0-9_-]+"


def readable(path):
    """The file with its block comments blanked out, line numbers intact.

    A name written in prose is not a use and not a declaration — and a guard that reads one
    reports a line the browser never looks at.
    """
    text = path.read_text()
    return re.sub(r"/\*.*?\*/", lambda m: re.sub(r"[^\n]", " ", m.group(0)), text, flags=re.S)


# Declared in a stylesheet.
declared = set()
for sheet in STYLESHEETS:
    declared |= set(re.findall(rf"({NAME})\s*:", readable(sheet)))

# Declared at run time: `setProperty("--name", …)`, and the `"--name":` key of an inline style.
runtime = set()
for script in SCRIPTS:
    body = readable(script)
    runtime |= set(re.findall(rf"setProperty\(\s*[\"'`]({NAME})", body))
    runtime |= set(re.findall(rf"[\"']({NAME})[\"']\s*:", body))

known = declared | runtime

# A `var()` whose name is built from a value: everything up to the interpolation is the prefix.
BUILT = re.compile(r"var\(\s*(--[A-Za-z0-9_-]*?)\$\{")
USED = re.compile(rf"var\(\s*({NAME})\s*[,)]")

undeclared = []
unprefixed = []
read = 0

for path in STYLESHEETS + SCRIPTS:
    for line_no, line in enumerate(readable(path).splitlines(), 1):
        for match in USED.finditer(line):
            read += 1
            if match.group(1) not in known:
                undeclared.append((path, line_no, match.group(1)))
        for match in BUILT.finditer(line):
            read += 1
            prefix = match.group(1)
            if not any(name.startswith(prefix) for name in known):
                unprefixed.append((path, line_no, prefix))

if undeclared or unprefixed:
    for path, line_no, name in undeclared:
        print(f"✗ css vars: {path}:{line_no} reads {name}, which nothing declares", file=sys.stderr)
    for path, line_no, prefix in unprefixed:
        print(f"✗ css vars: {path}:{line_no} builds a name from {prefix}…, and no declared name "
              f"starts with it", file=sys.stderr)
    print("    Declare it, or point the rule at the token that carries the value now. The "
          "declaration it sits in is being dropped, and the element is wearing its parent's value.",
          file=sys.stderr)
    sys.exit(1)

print(f"✓ css vars: all {read} names a var() reads are declared "
      f"({len(declared)} in the stylesheets, {len(runtime)} set at run time)")
PY
