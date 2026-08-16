#!/usr/bin/env bash
# check-ci-aggregate.sh — keep the merge gate's `needs:` complete.
#
# _ci.yml ends in one aggregate job that every other job feeds, and that job's name is the single check
# the merge target requires. It waits for what its `needs:` names and nothing else — so a job added
# without a line there is a job the required check never waits for. The run is green, the merge goes
# through, and nothing anywhere says the job was not counted. That is the one failure mode a red build
# cannot report, which is why it is asked here instead.
#
# Two things are checked. That the two lists agree: every job in the file is named in the aggregate's
# `needs:`, and that list names only jobs that exist. And that the aggregate still runs on `always()`:
# without it a failed dependency SKIPS the aggregate rather than failing it, and a skipped required
# check is not a red one.
#
# Usage: guards/check-ci-aggregate.sh    (no args; reads the workflow below)
# Exit codes: 0 = the gate waits for every job, 1 = it drifted.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
workflow=$root/.github/workflows/_ci.yml

if [ ! -f "$workflow" ]; then
  echo "✗ ci aggregate: $workflow is missing — did the merge gate move?" >&2
  exit 1
fi

python3 - "$workflow" <<'PY'
import re, sys

# The aggregate job's id. It is written here rather than guessed from the file, so that deleting or
# renaming the gate fails this guard instead of quietly leaving it with nothing to compare.
AGGREGATE = "all-green"

path = sys.argv[1]
lines = open(path).read().splitlines()

# The job ids are the keys indented two spaces under `jobs:`, which runs to the end of the file.
# Everything deeper belongs to a job's body, and a comment can sit at any column, including zero.
jobs, aggregate, current, in_jobs = [], [], None, False
for line in lines:
    if not in_jobs:
        in_jobs = line.rstrip() == "jobs:"
        continue
    if re.match(r"^[A-Za-z_]", line):
        break
    key = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
    if key:
        current = key.group(1)
        jobs.append(current)
    elif current == AGGREGATE:
        aggregate.append(line)

if AGGREGATE not in jobs:
    print(f"✗ ci aggregate: _ci.yml has no `{AGGREGATE}` job.\n    That is the name the merge target "
          "requires — renaming it takes the required check with it. Rename it back, or move the "
          "ruleset and this guard with it.", file=sys.stderr)
    sys.exit(1)

# `needs:` in either shape — a flow list on the one line, or a block list under it.
needs, collecting = [], False
for line in aggregate:
    head = re.match(r"^    needs:\s*(.*)$", line)
    if head:
        rest = head.group(1).strip()
        if rest.startswith("["):
            needs += re.findall(r"[A-Za-z0-9_-]+", rest)
        else:
            collecting = True
    elif collecting:
        item = re.match(r"^      - ([A-Za-z0-9_-]+)\s*$", line)
        if item:
            needs.append(item.group(1))
        elif line.strip() and not line.lstrip().startswith("#"):
            collecting = False

ok = True
expected = [j for j in jobs if j != AGGREGATE]

missing = [j for j in expected if j not in needs]
if missing:
    ok = False
    print(f"✗ ci aggregate: {', '.join(missing)} — no line in `{AGGREGATE}`'s needs.\n"
          "    The required check does not wait for them, so it goes green without their verdict. "
          "Add them to the needs list in .github/workflows/_ci.yml.", file=sys.stderr)

stale = [n for n in needs if n not in expected]
if stale:
    ok = False
    print(f"✗ ci aggregate: `{AGGREGATE}` needs {', '.join(stale)}, which _ci.yml has no job for.\n"
          "    A run cannot start with a needs entry pointing nowhere — the whole workflow fails to "
          "load. Drop them, or restore the jobs.", file=sys.stderr)

if not any(re.match(r"^    if:.*always\(\)", line) for line in aggregate):
    ok = False
    print(f"✗ ci aggregate: `{AGGREGATE}` no longer runs on `always()`.\n"
          "    Without it a failed job skips the aggregate instead of failing it, and a skipped "
          "required check is not a red one.", file=sys.stderr)

if ok:
    print(f"✓ ci aggregate: the merge gate waits for all {len(expected)} jobs in _ci.yml")
sys.exit(0 if ok else 1)
PY
