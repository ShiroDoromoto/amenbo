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
# The throwaway CWD is bound to nothing, which is a shape of its own: an AI reaches only the project
# its folder names, so every read that draws a reach comes back out_of_reach there. Both shapes are
# worth being able to run — the refusal itself is behaviour under test — so the binding is opt-in
# rather than assumed:
#   INIT=1 raises a store in the throwaway CWD first (`init`, as a human sets a folder up), and the
#   command that follows runs against a bound folder — which is the only way `--actor ai` reaches
#   anything here. Without it the CWD stays unbound, and an unbound folder is what gets exercised.
#
# Usage: scripts/verify-cli.sh <cli-binary> [args...]
#        (KEEP=1 to inspect the dirs after; INIT=1 to run against a bound store)
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

if [ "${INIT:-0}" = "1" ]; then
    # Setting a folder up is a human's act, so that is the facet it is done under. Its stdout goes to
    # stderr: the caller's own command owns stdout, and a JSON reader must not find a banner in front
    # of the document it asked for.
    if ! ( cd "$cwd" && env AMENBO_HOME="$home" "$bin" init --name verify --quiet --actor human ) >&2; then
        echo "✗ verify: INIT=1 could not raise a store in $cwd" >&2
        rm -rf "$home" "$cwd"
        exit 1
    fi
fi

rc=0
( cd "$cwd" && env AMENBO_HOME="$home" "$bin" "$@" ) || rc=$?

if [ "${KEEP:-0}" = "1" ]; then
    echo "verify: kept home=$home cwd=$cwd" >&2
else
    rm -rf "$home" "$cwd"
fi
exit "$rc"
