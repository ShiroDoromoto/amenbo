#!/usr/bin/env bash
# check-product-name.sh — hold the product's name to its spelling wherever we write prose.
#
# Prose that means the product writes `Amenbo`, while the development-side names stay lowercase —
# package names, bundle ids, constants, the repository, the wordmark, and `amenbo` as a command
# someone types. The same word therefore has two correct spellings, and which one is right is decided
# by what the sentence is doing — which no compiler, test or spell-checker reads. A slip is green
# everywhere: the build passes, the screen renders, the docs publish. Only a guard that looks at the
# prose itself can see it.
#
# It is worth guarding rather than fixing once because the name is written thousands of times over
# nineteen languages and every document here. Straightening it by hand holds until the next edit.
#
# WHAT IS SCANNED — the surfaces where the whole file is prose, so a match cannot be anything else:
#
#   1. Tracked `*.md`. Prose only: fenced blocks are skipped whole and code spans are cut out of the
#      lines that remain, which is where a command, a path and a package name are already written.
#   2. `app/src/core/i18n/locales/*.ts` — the nineteen dictionaries. Every value is a sentence shown
#      on screen, and a code span inside one is cut the same way.
#
# WHAT IS NOT, and why no list of exceptions would fix it:
#
#   - Rust and TypeScript source. `amenbo` is a constant, a channel name, a managed-block marker, a
#     JSON key, a bundle id and a test fixture there, all spelled correctly in lowercase, and they
#     sit in string literals beside the sentences. Pointing the guard at that face means teaching it
#     twenty exceptions — a list that rots into a suppression file, and one that would have to name
#     the very lines an author is most likely to write a slip on. The prose in those files is held by
#     review, as the wording of every message already is.
#   - `verification/scenarios/*.yaml`. A scenario is prose and data in one document: its `title` is a
#     sentence, its `author: amenbo` is a catalog value. Same shape, same answer.
#   - `devtool/fixtures/`. Captured from the outside world, so what it says is its producer's to
#     write. check-prose.sh carves it out for the same reason.
#
# WHAT IS STEPPED OVER inside the scanned prose:
#
#   - A code span, and everything inside a fenced block.
#   - `amenbo` followed by a top-level command word (guards/cli-command-words.txt): the reader is
#     being told what to type, and the command keeps its name. Recall is bounded by that list, and
#     that is the intended trade — check-cli-name.sh reads the same file from the other side, where
#     the same shape is the thing it must catch. A word missing from the list costs one catch, never
#     a false alarm.
#   - A name that is part of a longer token — `.amenbo`, `amenbo-core`, `amenbo.work`, `{amenbo}`,
#     `amenbo/devtool`. Those are the development-side names, which stay lowercase, and the
#     characters around them are what says so.
#   - A README's level-1 heading. That is the wordmark, and a wordmark is a piece of design rather
#     than a spelling, so it stays lowercase.
#
# Usage: guards/check-product-name.sh    (no args; scans the tracked prose surfaces)
# Exit codes: 0 = every prose mention spells the product Amenbo, 1 = one is lowercase.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

words=$(sed 's/#.*//' guards/cli-command-words.txt | tr -s '[:space:]' '\n' | grep -v '^$' | paste -sd'|' -)
[ -n "$words" ] || { echo "✗ product name: guards/cli-command-words.txt names no command" >&2; exit 1; }

files=()
while IFS= read -r -d '' f; do
    case "$f" in devtool/fixtures/*) continue ;; esac
    files+=("$f")
done < <(git ls-files -z '*.md' 'app/src/core/i18n/locales/*.ts')

[ ${#files[@]} -gt 0 ] || { echo "✗ product name: no prose file was found — did the tree move?" >&2; exit 1; }

# The name's own boundary: the characters that, on either side, make it part of a longer token.
# `awk` has no look-around, so the boundary is matched as a character and the command form is cut out
# of the line first (replaced by a space, so the cut cannot join two tokens into one).
if ! hits=$(awk -v words="$words" '
    function bare(s,   cut) {
        cut = s
        gsub("amenbo (" words ")([^A-Za-z0-9_-]|$)", " ", cut)
        return cut ~ "(^|[^A-Za-z0-9._/@{}_-])amenbo([^A-Za-z0-9._/@{}_-]|$)"
    }
    FNR == 1 {
        fenced = 0; span = 0
        md = (FILENAME ~ /\.md$/)
        readme = (FILENAME ~ /(^|\/)README\.md$/)
    }
    md && /^[ \t]*(```|~~~)/ { fenced = !fenced; span = 0; next }
    md && fenced { next }
    # A code span cannot cross a blank line, so an unclosed backtick stops there rather than
    # swallowing the rest of the document.
    md && /^[ \t]*$/ { span = 0; next }
    md && readme && /^# / { next }
    {
        masked = ""
        for (i = 1; i <= length($0); i++) {
            c = substr($0, i, 1)
            if (c == "`") { span = !span; continue }
            if (!span) masked = masked c
        }
        if (bare(masked)) print FILENAME ":" FNR ": " $0
    }
' "${files[@]}"); then
    echo "✗ product name: the guard could not read the prose surfaces" >&2
    exit 1
fi

if [ -n "$hits" ]; then
    echo "✗ product name: prose spells the product lowercase — write it Amenbo." >&2
    echo "$hits" >&2
    echo "  A command to type, a path, a package or the wordmark stays lowercase — put it in a code" >&2
    echo "  span, or write the command word after it, and this guard steps over it." >&2
    exit 1
fi

echo "✓ product name: every prose mention spells the product Amenbo"
