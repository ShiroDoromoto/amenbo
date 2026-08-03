#!/usr/bin/env bash
# changed-facets.sh — name the layers this working copy's change touches.
#
# `make gate` runs the stages a change can have moved, and this is what tells it which. The layers
# are not defined here: they are read from .github/paths-filters.yml, the same declaration CI hands
# to dorny/paths-filter, so the local gate and CI answer the same question from one file.
#
# The order of judgment is what makes the answer safe. Every changed path is held to the layers
# first. What matches none of them is held to `ungated`, the file's declaration of what nothing has
# to answer for (prose, pictures, the forms GitHub shows). A path that is in neither is unknown —
# and one unknown path is enough to print `full` alone, so a new crate or directory is judged by the
# whole gate until someone places it on a layer. CI's filters simply leave an unmatched path false,
# so this side is the one that holds the fallback.
#
# The subject is the whole change, not the last commit: everything this branch has that the base
# does not, plus what is not committed yet (staged, unstaged, untracked). The base is `main`, or
# whatever GATE_BASE names.
#
# Usage: scripts/changed-facets.sh          (prints one layer name per line, or `full`)
# Exit codes: 0 = the listing is the answer, 1 = the layer file could not be read.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

filters=.github/paths-filters.yml
base=${GATE_BASE:-main}

[ -f "$filters" ] || { echo "✗ gate layers: $filters is missing — CI reads it too." >&2; exit 1; }

# Where this branch left the base. With no such ref (a detached checkout, a clone without main) there
# is no change to narrow to, so the whole gate is the honest answer.
if ! merge_base=$(git merge-base HEAD "$base" 2>/dev/null); then
    echo "→ gate layers: no merge base with '$base' — nothing to narrow by, so: full" >&2
    echo full
    exit 0
fi

changed=$(
    {
        git diff --name-only "$merge_base" HEAD
        git diff --name-only HEAD
        git ls-files --others --exclude-standard
    } | sort -u
)

# The list travels in the environment, not on stdin: stdin is where the reader below comes from.
CHANGED="$changed" python3 - "$filters" <<'PY'
import os, re, sys

filters = sys.argv[1]
changed = [line for line in os.environ["CHANGED"].split("\n") if line]

# The layer file is read by hand, so the parser is strict: a line it does not recognise is a shape
# it was not written for, and answering anyway would mean answering wrongly.
def read_layers(path):
    layers, current = {}, None
    for lineno, raw in enumerate(open(path), 1):
        line = raw.rstrip("\n")
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        key = re.match(r"^([A-Za-z_][A-Za-z0-9_]*):$", line)
        if key:
            current = key.group(1)
            layers[current] = []
            continue
        item = re.match(r"^\s+- '([^']*)'$", line)
        if item and current is not None:
            layers[current].append(item.group(1))
            continue
        sys.exit(f"✗ gate layers: {path}:{lineno} is a shape this reader does not know: {line!r}\n"
                 "    Fix the reader rather than loosening it — a pattern it skips is a layer that "
                 "stops opening.")
    return layers

# The glob dialect the layer file is written in: `**` crosses directories, `*` and `?` stay inside
# one segment. It is the part of picomatch's grammar these patterns use, and no more.
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

layers = read_layers(filters)
if "ungated" not in layers or len(layers) < 3:
    sys.exit(f"✗ gate layers: {filters} holds no layers to judge by — fix it rather than deleting "
             "this reader.")

patterns = {name: [to_regex(g) for g in globs] for name, globs in layers.items()}
exempt = patterns.pop("ungated")

hit, unknown = set(), []
for path in changed:
    names = [name for name, pats in patterns.items() if any(p.match(path) for p in pats)]
    if names:
        hit.update(names)
    elif not any(p.match(path) for p in exempt):
        unknown.append(path)

if unknown:
    print("→ gate layers: on no layer and not declared exempt, so the whole gate runs:", file=sys.stderr)
    for path in unknown:
        print(f"    {path}", file=sys.stderr)
    print("full")
else:
    for name in sorted(hit):
        print(name)
PY
