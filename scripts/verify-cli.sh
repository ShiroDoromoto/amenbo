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
# One isolation, one command is the shape that costs: a store worth reading is one several commands
# built, and a store the next call cannot see is a harness rewritten by hand for every check. So the
# isolation also takes a *sequence*:
#   SCRIPT=<file> runs the file's lines through the same throwaway pair, in order, stopping at the
#   first line that fails and naming it. A line is the CLI's arguments and nothing else — the binary
#   is prepended, quoting is the shell's, blank lines and `#` lines are skipped — which is what keeps
#   a step readable as the command it would have been typed as.
# The two doors are exclusive: ARGS is one command, SCRIPT is a sequence, and a call that names both
# has not said which it means.
#
# Usage: scripts/verify-cli.sh <cli-binary> [args...]
#        (KEEP=1 to inspect the dirs after; INIT=1 to run against a bound store;
#         SCRIPT=<file> to run a sequence instead of one command)
set -euo pipefail

if [ $# -lt 1 ]; then
    echo "usage: verify-cli.sh <cli-binary> [args...]" >&2
    exit 2
fi
bin=$1
shift

[ -x "$bin" ] || { echo "✗ verify: CLI not built at $bin"; exit 1; }

script=${SCRIPT:-}
if [ -n "$script" ]; then
    if [ $# -gt 0 ]; then
        echo "✗ verify: SCRIPT and ARGS name different runs — pass one of them" >&2
        exit 2
    fi
    # Resolved before anything cd's: the path was written from where make was run, not from the
    # throwaway CWD the sequence executes in.
    case $script in /*) ;; *) script="$PWD/$script" ;; esac
    [ -r "$script" ] || { echo "✗ verify: SCRIPT is not readable: $script" >&2; exit 2; }
fi

# Run a sequence: each line is the CLI's arguments, echoed as it goes so a long run reads back as
# the steps it took. `eval` is what gives a line the quoting rules it was written with — a title
# holding spaces is one argument, exactly as at a prompt.
run_sequence() {
    local file=$1 lineno=0 line trimmed
    while IFS= read -r line || [ -n "$line" ]; do
        lineno=$((lineno + 1))
        trimmed=${line#"${line%%[![:space:]]*}"}
        case $trimmed in '' | '#'*) continue ;; esac
        printf '→ %s\n' "$trimmed" >&2
        if ! eval "\"\$bin\" $trimmed"; then
            printf '✗ verify: line %d failed: %s\n' "$lineno" "$trimmed" >&2
            return 1
        fi
    done < "$file"
}

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
if [ -n "$script" ]; then
    ( cd "$cwd" && export AMENBO_HOME="$home" && run_sequence "$script" ) || rc=$?
else
    ( cd "$cwd" && env AMENBO_HOME="$home" "$bin" "$@" ) || rc=$?
fi

if [ "${KEEP:-0}" = "1" ]; then
    echo "verify: kept home=$home cwd=$cwd" >&2
else
    rm -rf "$home" "$cwd"
fi
exit "$rc"
