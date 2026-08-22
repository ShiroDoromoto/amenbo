#!/usr/bin/env bash
# check-gui-inputs.sh — keep the gui layer's filter as wide as the GUI's own inputs.
#
# The GUI job is gated on the `gui` layer of .github/paths-filters.yml, which reads as if the front
# end were confined to app/. It is not. The Rust↔TS parity tests hold a hand-written TypeScript list
# against the Rust it mirrors, and they read that Rust straight out of the tree with Vite's `?raw` —
# so those Rust files decide the job's verdict just as much as anything under app/.
#
# Leave one out and the layer excuses the change that breaks it: a PR that edits only Rust is green
# with the GUI job SKIPPED, and the parity failure surfaces later, on main, under whichever unrelated
# PR next touches app/. Nothing reports it at the time — a skipped job is not a red one, and the
# lists on both sides of the parity still compile and still pass their own crates' tests.
#
# So the correspondence is asserted rather than remembered: every path an app/src file reaches for
# outside app/ has to be matched by some pattern under `gui:`.
#
# Usage: guards/check-gui-inputs.sh   (no args; reads the filter and app/src)
# Exit codes: 0 = every outside input is gated, 1 = the filter and the imports drifted.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

filters=.github/paths-filters.yml

if [ ! -f "$filters" ]; then
  echo "✗ gui inputs: $filters is missing — did the layer definition move?" >&2
  exit 1
fi

python3 - "$filters" <<'PY'
import os, re, subprocess, sys

path = sys.argv[1]

# The patterns under `gui:`. A key sits at column zero and so does a comment, so the block ends at
# the next line that starts with neither a space nor a `#`.
patterns, in_gui = [], False
for line in open(path).read().splitlines():
    if not in_gui:
        in_gui = line.rstrip() == "gui:"
        continue
    if line and not line[0].isspace() and not line.startswith("#"):
        break
    item = re.match(r"^\s*-\s*'([^']+)'\s*$", line)
    if item:
        patterns.append(item.group(1))

if not patterns:
    print(f"✗ gui inputs: {path} has no `gui:` layer, or none this guard can read.\n"
          "    That is the filter the GUI job is gated on. Fix the guard rather than deleting it.",
          file=sys.stderr)
    sys.exit(1)

# A relative specifier, in the two shapes the front end reads a file with: a `?raw` import of one
# file, and an `import.meta.glob` over many. Both are resolved against the importing file.
RAW = re.compile(r"""from\s*["'](\.[^"']*)\?raw["']""")
GLOB = re.compile(r"""import\.meta\.glob\(\s*["'](\.[^"']+)["']""")

outside = {}
tracked = subprocess.run(["git", "ls-files", "-z", "--", "app/src"],
                         capture_output=True, check=True, text=True).stdout
for f in tracked.split("\0"):
    if not f:
        continue
    source = open(f).read()
    for spec in RAW.findall(source) + GLOB.findall(source):
        resolved = os.path.normpath(os.path.join(os.path.dirname(f), spec))
        if resolved.startswith("app/"):
            continue  # already covered by the layer's own 'app/**'
        outside.setdefault(resolved, []).append(f)

if not outside:
    print("✗ gui inputs: nothing under app/src reads a file from outside app/.\n"
          "    This guard exists because the parity tests do. Either they were removed — in which "
          "case remove their entries from the `gui:` filter and this guard together — or the shape "
          "they are written in changed, and the guard has to learn it.", file=sys.stderr)
    sys.exit(1)

# The glob dialect the layer file is written in — `**` crosses directories, `*` and `?` stay inside
# one segment — spelled the same way scripts/changed-facets.sh spells it, because a filter pattern
# has to mean here exactly what it means to the two readers that gate on it.
def to_regex(glob):
    out, i = "", 0
    while i < len(glob):
        if glob.startswith("**/", i):
            out += "(?:[^/]+/)*"
            i += 3
        elif glob.startswith("**", i):
            out += ".*"
            i += 2
        elif glob[i] == "*":
            out += "[^/]*"
            i += 1
        elif glob[i] == "?":
            out += "[^/]"
            i += 1
        else:
            out += re.escape(glob[i])
            i += 1
    return re.compile("^" + out + "$")

gates = [to_regex(p) for p in patterns]

def gated(target):
    # A target read through `import.meta.glob` is itself a glob, and one glob does not match another.
    # It stands in as the narrowest concrete path of its shape: enough for a filter that covers the
    # directory to say so, and not enough for one that covers only some names in it to pass by
    # accident.
    probe = target.replace("**", "any/any").replace("*", "any").replace("?", "a")
    return any(g.match(probe) for g in gates)

missing = sorted(t for t in outside if not gated(t))
if missing:
    for target in missing:
        readers = ", ".join(sorted(outside[target]))
        print(f"✗ gui inputs: {target} is read by {readers}, and no pattern under `gui:` matches it.",
              file=sys.stderr)
    print(f"    A change to it would skip the GUI job, and the break would land on main under an "
          f"unrelated PR. Add it to the `gui:` layer in {path}.", file=sys.stderr)
    sys.exit(1)

print(f"✓ gui inputs: all {len(outside)} sources the GUI reads from outside app/ open the gui gate")
PY
