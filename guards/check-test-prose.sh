#!/usr/bin/env bash
# check-test-prose.sh — block Japanese test prose from re-entering the tree.
# Test prose is developer-facing English: the name a test is declared under, and
# the reason a test is ignored. An outside contributor reads the tests first, so
# these must not drift back into Japanese. The comment guard cannot see this face
# (it is a string literal, not a comment), so it needs a guard of its own.
#
# Covered (the faces where a match cannot be anything but prose):
#   1. JS test names — the first argument of describe/it/test/bench, in the files
#      that actually run: vitest's (src/**/*.test.{ts,tsx}, per app/vitest.config.ts's
#      `include`) and node --test's (guards/**/*.test.mjs, per the CI step).
#   2. Rust ignore reasons — the string in `#[ignore = "..."]`.
#
# NOT covered, deliberately — both are the same shape: a fixture and the prose
# share one construct, so no pattern tells them apart without false hits, and the
# face is left to human/AI judgement.
#   - Rust `assert!` messages and TS `expect(x, msg)`. A deliberate Japanese
#     fixture (a task title, a person's name — the proof that Japanese input goes
#     through) sits inside the very same assert.
#   - `it.each([...])("name %s")` and friends, whose name is not the first
#     argument. Reaching it would mean taking the first literal that follows the
#     call instead, and that lands on the each-table — where a Japanese fixture is
#     just as legitimate as it is inside an assert.
#
# Only the string literal is judged, never the whole line: prose says "run it
# (the staged one)" all over, and a test body may legitimately hold a Japanese
# fixture on the same line as an English test name.
#
# The name may sit on the line after the call, so a literal that has not opened
# by end of line is looked for at the start of the next one — but only when that
# line really is the next one (same file, next number), which a --staged hunk
# boundary is not. Otherwise pressing Enter would walk straight through.
#
# Detection runs under LC_ALL=C, so the CJK ranges are written as the UTF-8 byte
# sequences they encode (checked identical on BSD awk, mawk, gawk and busybox
# awk): U+3040-U+30FF (kana) and U+4E00-U+9FFF (han).
#
# Usage:
#   check-test-prose.sh --staged         scan staged additions (git diff --cached)  [default]
#   check-test-prose.sh [--] FILE...     scan the given files
#   check-test-prose.sh -                scan stdin (both rules apply)
#
# Exit codes: 0 = clean, 1 = Japanese test prose matched, 2 = bad usage.

set -euo pipefail

MODE="staged"
FILES=()

while [ $# -gt 0 ]; do
  case "$1" in
    --staged) MODE="staged"; shift ;;
    -) MODE="stdin"; shift ;;
    --) shift; MODE="files"; FILES+=("$@"); break ;;
    -*) echo "check-test-prose.sh: unknown flag: $1" >&2; exit 2 ;;
    *) MODE="files"; FILES+=("$1"); shift ;;
  esac
done

# Which rule a file is judged by; a file matching none is not scanned at all.
kind_of() {
  case "$1" in
    *.test.ts|*.test.tsx|*.test.mjs) echo "js" ;;
    *.rs) echo "rs" ;;
    *) echo "" ;;
  esac
}

# Invoked indirectly, by the trap below — shellcheck does not follow that call.
# shellcheck disable=SC2329
cleanup() { rm -f "${STREAM:-}" 2>/dev/null || true; }
trap cleanup EXIT

# Build the scan stream as 'LOCATION<TAB>KIND<TAB>CONTENT' lines.
STREAM="$(mktemp)"

emit_staged() {
  local file kind
  while IFS= read -r -d '' file; do
    kind="$(kind_of "$file")"
    [ -n "$kind" ] || continue
    git diff --cached --unified=0 --no-color -- "$file" | awk -v f="$file" -v k="$kind" '
      /^\+\+\+/ { next }
      /^@@/ {
        if (match($0, /\+[0-9]+/)) { ln = substr($0, RSTART + 1, RLENGTH - 1) + 0 }
        next
      }
      /^\+/ {
        printf "%s:%d\t%s\t%s\n", f, ln, k, substr($0, 2)
        ln++
      }
    '
  done < <(git diff --cached --name-only --diff-filter=ACM -z)
}

emit_files() {
  local f kind
  for f in "$@"; do
    [ -f "$f" ] || continue
    kind="$(kind_of "$f")"
    [ -n "$kind" ] || continue
    LC_ALL=C awk -v f="$f" -v k="$kind" '{ printf "%s:%d\t%s\t%s\n", f, NR, k, $0 }' "$f"
  done
}

emit_stdin() {
  awk '{ printf "(stdin):%d\tany\t%s\n", NR, $0 }'
}

case "$MODE" in
  staged) emit_staged > "$STREAM" ;;
  files)  emit_files "${FILES[@]}" > "$STREAM" ;;
  stdin)  emit_stdin > "$STREAM" ;;
esac

HITS="$(LC_ALL=C awk -F'\t' '
  BEGIN {
    # Kana U+3040-U+30FF and han U+4E00-U+9FFF, as UTF-8 bytes (see the header).
    CJK = "\343[\201-\203]|\344[\270-\277]|[\345-\351]"
    # A test declaration: describe/it/test/bench, any modifier chain (.only,
    # .skip, .concurrent), then the open paren its name follows. The leading
    # class rejects a longer identifier and a method call, so `submit(` and
    # `re.test(` are not mistaken for a declaration.
    JS = "(^|[^A-Za-z0-9_$.])(describe|it|test|bench)(\\.[A-Za-z]+)*[ \t]*\\("
    RS_IGN = "#\\[ignore[ \t]*=[ \t]*"
  }
  # The string literal opening at or after position i, "" when none opens there.
  # Sets RAN_OFF when the line ended before any literal opened — the name is then
  # on the next line, which is the caller`s to chase.
  # Bytes are copied through verbatim, so a multi-byte character survives intact
  # (UTF-8 is self-synchronising: no continuation byte can be a quote or a
  # backslash).
  function literal_at(s, i,   q, c, out) {
    RAN_OFF = 0
    while (i <= length(s) && substr(s, i, 1) ~ /[ \t]/) i++
    if (i > length(s)) { RAN_OFF = 1; return "" }
    q = substr(s, i, 1)
    if (q != "\"" && q != "'"'"'" && q != "`") return ""
    out = ""
    for (i++; i <= length(s); i++) {
      c = substr(s, i, 1)
      if (c == "\\") { i++; continue }
      if (c == q) break
      out = out c
    }
    return out
  }
  function report(loc, what, lit) { printf "%s\t%s: %s\n", loc, what, lit }
  # Report the first literal that follows `re` on this line and holds CJK. Leaves
  # PENDING[what] set when a call`s name has not opened by end of line.
  function scan(loc, s, re, what,   pos, at, lit) {
    PENDING[what] = 0
    for (pos = 1; pos <= length(s); pos = at) {
      if (!match(substr(s, pos), re)) return
      at = pos + RSTART - 1 + RLENGTH
      lit = literal_at(s, at)
      if (lit ~ CJK) { report(loc, what, lit); return }
      if (RAN_OFF) { PENDING[what] = 1; return }
    }
  }
  # A carried-over name counts only on the line that truly follows the one that
  # opened the call: same file, next number. A --staged stream is added lines
  # only, so a hunk boundary must not carry (CI scans the whole tree anyway).
  function chase(loc, s, what,   lit) {
    if (!PENDING[what] || !CONTIGUOUS) return
    lit = literal_at(s, 1)
    if (lit ~ CJK) report(loc, what, lit)
  }
  {
    # Split LOCATION into file and line at its last colon.
    i = length($1)
    while (i > 0 && substr($1, i, 1) != ":") i--
    file = substr($1, 1, i - 1); lineno = substr($1, i + 1) + 0
    CONTIGUOUS = (file == prev_file && lineno == prev_line + 1)
    prev_file = file; prev_line = lineno
  }
  $2 == "js" || $2 == "any" { chase($1, $3, "test name");     scan($1, $3, JS, "test name") }
  $2 == "rs" || $2 == "any" { chase($1, $3, "ignore reason"); scan($1, $3, RS_IGN, "ignore reason") }
' "$STREAM" || true)"

if [ -n "$HITS" ]; then
  echo "check-test-prose.sh: Japanese test prose matched — write it in English (a Japanese fixture is fine; the name/reason is not)." >&2
  while IFS= read -r line; do
    echo "  $line" >&2
  done <<< "$HITS"
  exit 1
fi

exit 0
