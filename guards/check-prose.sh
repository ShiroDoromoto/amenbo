#!/usr/bin/env bash
# check-prose.sh — hold the tracked files that carry no code to the same
# vocabulary as the comments: no reference to this project's own tracker, and
# English only.
#
# The comment guard reads comments, so whatever a file says outside one is seen
# by no one. Two faces are hit, and both are handed to layer 2 whole
# (`esorp check --text`) — the same declaration the comments and the commit
# message are judged by:
#
#   1. Markdown prose. esorp.yaml's `sgml` family reads `<!-- -->` and nothing
#      else, so a reference written in the prose of README.md, the most-read
#      document here, is invisible to it.
#   2. Config and manifest values. The `hash` family reads the `#` comments of
#      .yml/.toml, and JSON has no comment to read at all — so a value is
#      invisible whatever it says, the `description` of a published Cargo.toml
#      included. A comment scanner cannot see a value; only reading the file
#      whole can.
#
# Reading a file whole means its code-ish parts are read as prose too — a fenced
# block, a key, a version. That is the price of the face, and it is affordable
# here precisely because these files hold no code that must speak Japanese: the
# one place the reference form may appear is a code span, which esorp.yaml's
# internal-ref already encodes (its pattern excludes a backtick on either side).
#
# Source is deliberately absent, and stays absent. Its literals are half of a
# bilingual product — `Msg::new`'s EN/JA pairs, i18n's per-language dictionaries, a Japanese
# test fixture — where Japanese is the feature, not a leak. No pattern separates
# those from a genuine slip, so english-only cannot be pointed at them. What is
# judged here is the complement: the files where any Japanese at all is a slip.
#
# That complement holds while these formats carry no data. A fixture — Japanese
# sample rows parked in .json rather than in the .ts they live in today — would be
# the one file here where Japanese is right, and this guard would call it wrong.
# The tree holds no such file, so the line is not drawn around one; should that
# change, narrow the set rather than teach the guard an exception.
#
# Two files are left out, each for a reason that cannot rot into a list:
#   - esorp.yaml, the declaration itself. Its rules have to spell the words they
#     forbid, so it fails every one it defines — no-history's pattern alone trips
#     no-history, english-only and internal-ref. check-doc-refs.sh carves itself
#     out for the same reason.
#   - Lock files, which are generated. A rule about how we write cannot bind a
#     file we do not write, and a violation there would be unfixable by hand.
#
# The whole tree is judged, not the changed part: these files are clean, so a
# violation anywhere is the commit's to answer for. The comment guard reads its
# own face the same way.
#
# esorp is optional: an outside contributor without it commits as before. A
# caller that means this to be a gate has to install it (CI's comment-gate does).
#
# Usage:
#   check-prose.sh              scan every tracked file of the covered kinds  [default]
#   check-prose.sh [--] FILE... scan the given files
#
# Exit codes: 0 = clean, 1 = a violation matched, 2 = bad usage.

set -uo pipefail

FILES=()
while [ $# -gt 0 ]; do
  case "$1" in
    --) shift; FILES+=("$@"); break ;;
    -*) echo "check-prose.sh: unknown flag: $1" >&2; exit 2 ;;
    *) FILES+=("$1"); shift ;;
  esac
done

if ! command -v esorp >/dev/null 2>&1; then
  echo "→ esorp not installed — prose guard skipped (see CONTRIBUTING.md)"
  exit 0
fi

if [ ${#FILES[@]} -eq 0 ]; then
  while IFS= read -r -d '' f; do
    case "$f" in
      esorp.yaml|*-lock.json|*.lock) continue ;;
    esac
    FILES+=("$f")
  done < <(git ls-files -z '*.md' '*.toml' '*.yml' '*.yaml' '*.json' '*.mod')
fi

rc=0
for f in "${FILES[@]}"; do
  [ -f "$f" ] || continue
  # The report names a line but not the file, so the file is named here. Layer 1
  # does not apply to a body handed in this way, and esorp says so on every run;
  # that line is noise repeated once per file, so it is dropped.
  if ! out="$(esorp check --text "$f" 2>&1)"; then
    rc=1
    echo "check-prose.sh: $f" >&2
    grep -v '^Only layer 2' <<< "$out" >&2
  fi
done

exit "$rc"
