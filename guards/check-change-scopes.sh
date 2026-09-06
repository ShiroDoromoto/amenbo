#!/usr/bin/env bash
# check-change-scopes.sh — keep the GUI's fold of the change feed complete.
#
# The store's change feed names the table each row came from, and the GUI folds those
# dataset names into the scopes it invalidates (`DATASET_SCOPES` in app/src/core/changes.ts).
# A dataset with no entry there cannot be folded, so every write to that table falls to
# "gap" — a full re-read of everything on screen. That is the safe side, which is exactly
# why it is invisible: the screen stays correct, and only the cost moves. Two tables lived
# in that state unnoticed until someone measured (the plugin gate and its settings).
#
# Nothing else watches this seam: the two sides are written in different languages, and
# both compile and pass their own tests while they disagree. So this guard asks the
# question directly — is every dataset the feed can name folded, and does the map name
# only tables that exist?
#
# The producer is two lists, not one. The records are what `Record::new` names; beside them
# the feed carries a few of this device's own plain tables, named in `FEED_PLAIN_TABLES`
# (they are on the feed so a screen here hears them change, and on no road out of the
# store). A guard reading only the first would call the second stale.
#
# Usage: guards/check-change-scopes.sh    (no args; reads the two source files below)
# Exit codes: 0 = the two sides agree, 1 = they drifted.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
records=$root/crates/amenbo-core/src/store_engine/record.rs
schema=$root/crates/amenbo-core/src/store_engine/schema.rs
scopes=$root/app/src/core/changes.ts

for f in "$records" "$schema" "$scopes"; do
  if [ ! -f "$f" ]; then
    echo "✗ change scopes: $f is missing — did the feed's producer or its consumer move?" >&2
    exit 1
  fi
done

python3 - "$records" "$schema" "$scopes" <<'PY'
import re, sys

records, schema, scopes = sys.argv[1], sys.argv[2], sys.argv[3]

# The producer, first half: every `Record::new("<dataset>", …)` in core.
emitted = set(re.findall(r'Record::new\(\s*"([a-z_]+)"', open(records).read()))
# The producer, second half: the plain tables the feed carries beside the records.
plain = re.search(r'FEED_PLAIN_TABLES:\s*&\[&str\]\s*=\s*&\[(.*?)\];', open(schema).read(), re.S)
if not plain:
    print("✗ change scopes: FEED_PLAIN_TABLES could not be read — the feed's second producer "
          "list moved. Fix the guard rather than deleting it.", file=sys.stderr)
    sys.exit(1)
emitted |= set(re.findall(r'"([a-z_]+)"', plain.group(1)))

# The consumer: the keys of DATASET_SCOPES, up to its closing brace.
body = open(scopes).read()
block = re.search(r'const DATASET_SCOPES[^{]*\{(.*?)\n\};', body, re.S)
folded = set(re.findall(r'^\s*([a-z_]+):', block.group(1), re.M)) if block else set()

if not emitted or not folded:
    print("✗ change scopes: could not read one of the two sides (a shape this guard "
          "parses by hand changed) — fix the guard rather than deleting it.", file=sys.stderr)
    sys.exit(1)

ok = True
missing = sorted(emitted - folded)
if missing:
    ok = False
    print(f"✗ change scopes: the feed can name {', '.join(missing)}, and DATASET_SCOPES has no "
          "entry for them.\n    Every write to those tables costs a full re-read (gap). Add them to "
          "app/src/core/changes.ts, and give the scope a receiver in query.invalidateScopes.",
          file=sys.stderr)
stale = sorted(folded - emitted)
if stale:
    ok = False
    print(f"✗ change scopes: DATASET_SCOPES folds {', '.join(stale)}, which core no longer emits.\n"
          "    A renamed table leaves a live entry pointing nowhere and its new name unfolded.",
          file=sys.stderr)

if ok:
    print(f"✓ change scopes: all {len(emitted)} datasets the feed names are folded")
sys.exit(0 if ok else 1)
PY
