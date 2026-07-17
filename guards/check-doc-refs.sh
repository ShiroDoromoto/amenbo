#!/usr/bin/env bash
# check-doc-refs.sh — block references to internal working notes from entering
# the tree. Internal design notes are not shipped; pointers to them (full paths
# and "docs NN §X" short refs) must never appear in source, CI, or user-facing
# strings. Prose reminders alone do not hold, so this is a mechanical guard
# rather than a rule someone is asked to remember.
#
# The patterns it looks for are FIXED and live right here, so the guard needs no
# setup and no list kept anywhere else: a clone is green out of the box.
#
# Detection: a pointer to a SPECIFIC internal note —
#   1. a path with a target under the notes tree;
#   2. "doc"/"docs" + ('/' or ' ') + a number (a short "doc <N>" / "docs <N>" ref);
#   3. a bare section ref anchored on the section sign: "§<N>" (this also catches
#      the once-common "<NN> §<N>" combo via its "§<N>" tail). Anchoring on §
#      sidesteps the false-positive minefield of bare two-digit numbers (years,
#      counts, HTTP status). Bare backtick numbers like `24` are deliberately NOT
#      matched for the same reason — § is the reliable anchor.
# The bare directory mention (the notes folder with nothing after it) is NOT
# matched, so an operational "do not commit the notes folder" instruction (as in
# .gitignore) passes without an exemption — while a short "doc <N>" / "§<N>" ref
# hiding in such a file is still caught.
#
# Allowed (NOT scanned): the docs/ tree itself (the notes live here) and this
# guard script (which necessarily contains the pattern).
#
# Usage:
#   check-doc-refs.sh --staged         scan staged additions (git diff --cached)  [default]
#   check-doc-refs.sh [--] FILE...     scan the given files
#   check-doc-refs.sh -                scan stdin
#
# Exit codes: 0 = clean, 1 = a reference matched, 2 = bad usage.

set -euo pipefail

MODE="staged"
FILES=()

while [ $# -gt 0 ]; do
  case "$1" in
    --staged) MODE="staged"; shift ;;
    -) MODE="stdin"; shift ;;
    --) shift; MODE="files"; FILES+=("$@"); break ;;
    -*) echo "check-doc-refs.sh: unknown flag: $1" >&2; exit 2 ;;
    *) MODE="files"; FILES+=("$1"); shift ;;
  esac
done

# Internal-note reference: a notes path WITH a target (a char after the folder
# slash), or "doc"/"docs" + ('/' or ' ') + a number, or a §-anchored section ref
# ("§<N>", which also catches the "<NN> §<N>" combo). A bare folder mention is
# excluded so operational "do not commit the notes folder" instructions pass.
REGEX='docs/wip/[0-9A-Za-z]|docs?[ /][0-9]|§ *[0-9]'

# Paths exempt from scanning (relative to repo root). docs/ holds the notes
# themselves; this script necessarily contains the pattern.
is_allowed() {
  case "$1" in
    docs/*|guards/check-doc-refs.sh) return 0 ;;
    *) return 1 ;;
  esac
}

# Invoked indirectly, by the trap below — shellcheck does not follow that call.
# shellcheck disable=SC2329
cleanup() { rm -f "${STREAM:-}" 2>/dev/null || true; }
trap cleanup EXIT

# Build the scan stream as 'LOCATION<TAB>CONTENT' lines.
STREAM="$(mktemp)"

emit_staged() {
  local file
  while IFS= read -r -d '' file; do
    is_allowed "$file" && continue
    git diff --cached --unified=0 --no-color -- "$file" | awk -v f="$file" '
      /^\+\+\+/ { next }
      /^@@/ {
        if (match($0, /\+[0-9]+/)) { ln = substr($0, RSTART + 1, RLENGTH - 1) + 0 }
        next
      }
      /^\+/ {
        printf "%s:%d\t%s\n", f, ln, substr($0, 2)
        ln++
      }
    '
  done < <(git diff --cached --name-only --diff-filter=ACM -z)
}

emit_files() {
  local f
  for f in "$@"; do
    [ -f "$f" ] || continue
    is_allowed "$f" && continue
    # Skip binary files — CI passes the whole tree (images included). 'grep -I'
    # treats a binary file as non-matching, so a non-empty file that fails to
    # match any line is binary.
    if [ -s "$f" ] && ! LC_ALL=C grep -Iq . "$f" 2>/dev/null; then continue; fi
    LC_ALL=C awk -v f="$f" '{ printf "%s:%d\t%s\n", f, NR, $0 }' "$f"
  done
}

emit_stdin() {
  awk '{ printf "(stdin):%d\t%s\n", NR, $0 }'
}

case "$MODE" in
  staged) emit_staged > "$STREAM" ;;
  files)  emit_files "${FILES[@]}" > "$STREAM" ;;
  stdin)  emit_stdin > "$STREAM" ;;
esac

# Match the CONTENT field only (so a path that contains "docs/" is not a false
# hit). The reference text is not sensitive, so the offending line is shown.
HITS="$(LC_ALL=C awk -F'\t' -v re="$REGEX" '$2 ~ re { print }' "$STREAM" || true)"

if [ -n "$HITS" ]; then
  echo "check-doc-refs.sh: internal-note reference matched — drop the pointer (keep the technical explanation; rewrite doc-dependent wording to be self-contained)." >&2
  while IFS= read -r line; do
    echo "  $line" >&2
  done <<< "$HITS"
  exit 1
fi

exit 0
