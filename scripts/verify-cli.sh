#!/usr/bin/env bash
# verify-cli.sh — run a freshly built CLI against a throwaway store (`make verify`).
#
# Why a script and not a Makefile recipe:
#   This is the body of `make verify`, lifted out of the Makefile. A recipe is shell too, but
#   shell nobody lints: `shell-gate` only sees files that exist as `*.sh`, and make's `$$`
#   escaping sits between the author and what the shell finally gets. That matters here more
#   than anywhere else in the Makefile — the recipe ends in `rm -rf` over two variables. An
#   unset one (a typo, a renamed variable) expands to nothing and the command becomes
#   `rm -rf` of whatever is left. As a file, shellcheck reads it.
#
# The isolation is the whole point, and it is two
# things, both required:
#   (1) AMENBO_HOME=throwaway dir — the ONLY thing that keeps the run out of the real app-data
#       tree. An isolated CWD alone does not: `init` with no `.amenbo` pointer in sight creates
#       a store under the real app-data root.
#   (2) a throwaway CWD with no `.amenbo` ancestor — so a run inside the repo cannot grab the
#       production pointer.
#
# Usage: scripts/verify-cli.sh <cli-binary> [args...]        (KEEP=1 to inspect the dirs after)
set -euo pipefail

if [ $# -lt 1 ]; then
    echo "usage: verify-cli.sh <cli-binary> [args...]" >&2
    exit 2
fi
bin=$1
shift

[ -x "$bin" ] || { echo "✗ verify: CLI not built at $bin"; exit 1; }

home=$(mktemp -d)
cwd=$(mktemp -d)

rc=0
( cd "$cwd" && env AMENBO_HOME="$home" "$bin" "$@" ) || rc=$?

if [ "${KEEP:-0}" = "1" ]; then
    echo "verify: kept home=$home cwd=$cwd" >&2
else
    rm -rf "$home" "$cwd"
fi
exit "$rc"
