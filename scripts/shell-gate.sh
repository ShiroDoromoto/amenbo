#!/usr/bin/env bash
# shell-gate.sh — lint every tracked shell script, and the shell embedded in the workflows.
#
# Why the version floor:
#   The verdict is only shared if both sides run the same linter. This repo's shell relies on
#   `# shellcheck disable=SC2329` (guards/check-doc-refs.sh), and SC2329 was split out of SC2317
#   in 0.10.0 — so a shellcheck older than that does not understand the directive and reports the
#   very warning the directive suppresses. CI already installs the current stable release;
#   this closes the mirror hole on the developer's machine: an old shellcheck must say so and stop,
#   not hand out a different verdict from CI's.
#
# Why actionlint gets no floor:
#   A floor earns its upkeep only where an OLD linter hands out a verdict CI would not — shellcheck
#   is exactly that case, because the repo depends on a directive a given version introduced.
#   actionlint has no such dependency here: the repo carries no actionlint config and no ignore
#   directives, and CI installs the newest actionlint on every run. A stale one on this machine can
#   therefore only MISS a check that CI then catches — red in CI, never a wrong green on main. What
#   the gate owes instead is the version it actually ran, so a split verdict is traceable rather
#   than a mystery. Both linters say so below.
#
# Usage: scripts/shell-gate.sh <shell-file>...
set -euo pipefail

# The oldest shellcheck that knows SC2329. Raise it when the repo starts depending on a newer check.
MIN_MAJOR=0
MIN_MINOR=10

if [ $# -eq 0 ]; then
    echo "usage: shell-gate.sh <shell-file>..." >&2
    exit 2
fi

command -v shellcheck >/dev/null 2>&1 || {
    echo "shellcheck is required: brew install shellcheck (or https://www.shellcheck.net)"
    exit 1
}

version=$(shellcheck --version | awk '/^version:/ { print $2; exit }')
major=${version%%.*}
rest=${version#*.}
minor=${rest%%.*}

if [ -z "$version" ] || ! [ "$major" -eq "$major" ] 2>/dev/null || ! [ "$minor" -eq "$minor" ] 2>/dev/null; then
    # An unparseable version (a custom build) is not a reason to block the gate — the linter still
    # runs. Say it out loud so a wrong verdict is at least traceable.
    echo "! could not read shellcheck's version ($MIN_MAJOR.$MIN_MINOR or newer is required). Running anyway" >&2
elif [ "$major" -eq "$MIN_MAJOR" ] && [ "$minor" -lt "$MIN_MINOR" ]; then
    echo "✗ shellcheck $version is too old ($MIN_MAJOR.$MIN_MINOR or newer is required)."
    echo "  This repo's shell depends on the SC2329 disable directive, split out of SC2317 in 0.10.0,"
    echo "  and an older shellcheck does not understand the directive and splits the verdict from CI's (which runs stable)."
    echo "  Update: brew upgrade shellcheck (or https://github.com/koalaman/shellcheck/releases)"
    exit 1
fi

# `-x` follows the `source=` directives, so a script's shared helpers resolve.
echo "→ shellcheck ${version:-?}"
shellcheck -x "$@"

command -v actionlint >/dev/null 2>&1 || {
    echo "actionlint is required: brew install actionlint (or https://rhysd.github.io/actionlint/)"
    exit 1
}

# The shell embedded in the workflows' own `run:` blocks — actionlint hands each one to the linter
# on PATH, i.e. the same one the floor above just vetted.
# (Never open a comment line with the word `shellcheck`: it is parsed as a directive, SC1072/SC1073.)
echo "→ actionlint $(actionlint -version | head -1)"
actionlint
