#!/usr/bin/env bash
# check-workflow-run-names.sh — keep a `workflow_run` trigger pointed at a workflow that exists.
#
# `workflow_run.workflows` names the workflow it waits on by its `name:`, not by its filename. So a
# rename on the other side breaks the trigger, and breaks it in the one way nothing reports: the
# workflow simply never fires. No run is created, so there is no red to see and nothing to notify —
# the syntax is still valid, and every other gate stays green. The symptom arrives days later as
# "the bot's pull requests stopped merging themselves".
#
# Two things are checked, and both are the same failure. That every name a `workflow_run` references
# is some workflow's `name:`. And that the workflow it names can actually start: one whose only
# trigger is `workflow_call` is a body something else runs, so waiting on it is waiting on a run that
# will never be reported under that name.
#
# A file with no `name:` at all is an error too, for the same reason: GitHub falls back to showing
# the file path, which is not a string a `workflow_run` can match.
#
# Usage: guards/check-workflow-run-names.sh    (no args; reads .github/workflows)
# Exit codes: 0 = every reference resolves, 1 = one of them names nothing that can fire.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
dir=$root/.github/workflows

if [ ! -d "$dir" ]; then
  echo "✗ workflow_run names: $dir is missing — did the workflows move?" >&2
  exit 1
fi

python3 - "$dir" <<'PY'
import pathlib, re, sys

directory = pathlib.Path(sys.argv[1])
files = sorted(directory.glob("*.yml")) + sorted(directory.glob("*.yaml"))


def unquote(text):
    text = text.strip()
    if len(text) >= 2 and text[0] == text[-1] and text[0] in "\"'":
        return text[1:-1]
    return text


def declared_name(lines):
    """The `name:` a run is reported under. Top level, so column zero."""
    for line in lines:
        head = re.match(r"^name:\s*(\S.*)$", line)
        if head:
            return unquote(head.group(1))
    return None


def triggers(lines):
    """The keys under the top-level `on:` — what can start this workflow."""
    found, inside = [], False
    for line in lines:
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        if re.match(r"^on:\s*$", line):
            inside = True
            continue
        if not inside:
            continue
        if re.match(r"^\S", line):
            break
        key = re.match(r"^  ([A-Za-z_]+):", line)
        if key:
            found.append(key.group(1))
    return found


def references(lines):
    """Every name listed under a `workflow_run:`'s `workflows:` key, in either list shape."""
    found, inside, depth, collecting = [], False, 0, False
    for line in lines:
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        opener = re.match(r"^(\s*)workflow_run:\s*$", line)
        if opener:
            inside, depth, collecting = True, len(opener.group(1)), False
            continue
        if inside and len(line) - len(line.lstrip()) <= depth:
            inside, collecting = False, False
        if not inside:
            continue
        key = re.match(r"^\s*workflows:\s*(.*)$", line)
        if key:
            rest = key.group(1).strip()
            if rest.startswith("[") and rest.endswith("]"):
                found += [unquote(x) for x in rest[1:-1].split(",") if x.strip()]
                collecting = False
            else:
                collecting = True
            continue
        if collecting:
            item = re.match(r"^\s*-\s*(\S.*)$", line)
            if item:
                found.append(unquote(item.group(1)))
            else:
                collecting = False
    return found


names, starts, unnamed, refs = {}, {}, [], []
for path in files:
    lines = path.read_text().splitlines()
    name = declared_name(lines)
    if name is None:
        unnamed.append(path.name)
        continue
    names[name] = path.name
    starts[name] = [t for t in triggers(lines) if t != "workflow_call"]
    refs += [(path.name, ref) for ref in references(lines)]

ok = True

if unnamed:
    ok = False
    print(f"✗ workflow_run names: {', '.join(unnamed)} declare no `name:`.\n"
          "    A run with no name is reported under its file path, which no `workflow_run` can "
          "match. Give each one a name.", file=sys.stderr)

for source, ref in refs:
    if ref not in names:
        ok = False
        print(f"✗ workflow_run names: {source} waits on `{ref}`, which is no workflow's `name:`.\n"
              "    Nothing fires and nothing goes red — the trigger is simply never reached. Name "
              "one that exists, or restore the name that moved.", file=sys.stderr)
    elif not starts[ref]:
        ok = False
        print(f"✗ workflow_run names: {source} waits on `{ref}` ({names[ref]}), which only has "
              "`workflow_call`.\n    A body nothing starts on its own is never reported under that "
              "name, so the wait never ends. Wait on the entry that runs it.", file=sys.stderr)

if ok:
    print(f"✓ workflow_run names: all {len(refs)} reference(s) name a workflow that can fire")
sys.exit(0 if ok else 1)
PY
