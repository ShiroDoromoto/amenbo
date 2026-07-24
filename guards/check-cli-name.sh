#!/usr/bin/env bash
# check-cli-name.sh — keep the CLI from telling anyone to type a command it does not answer to.
#
# The name to type is a channel fact, not a constant: a production build installs `amenbo`, a dev
# build installs `amenbo-dev`, and `Paths::command_name()` is the one place that knows which. Every
# string that words a command for someone to run has to take it from there.
#
# A hardcoded name is invisible to every other gate. It compiles, it reads correctly in the source,
# and the production build even prints it correctly — the sentence only turns into a lie on the dev
# channel, where an AI (or a human) is sent to a command that is not installed. Tests do not catch it
# either: they run on the production channel, where the hardcoded spelling and the real one coincide.
# So the only way to see it is to ask the source directly.
#
# What is scanned: the Rust that words user-facing text (errors, hints, notices, the doctor's fixes)
# in amenbo-core and amenbo-cli. What is not:
#
#   - comment lines (`//`, `///`, `//!`) — prose about the code, not output. Note that clap turns the
#     doc comments in `cli.rs` into `--help` text, which this guard therefore does not reach.
#   - everything from a file's first `#[cfg(test)]` — a test that pins the production spelling is
#     asserting the production channel's answer, which is exactly right.
#   - `crates/amenbo-core/src/agent.rs` — the agent spec is authored with the production spelling on
#     purpose, in its runnable lines and its prose alike, and retargeted as the spec is handed out;
#     tests in that file hold the rule.
#
# Recall is bounded by the list of command words below, and that is the intended trade: a name
# followed by a real subcommand is a command someone is being told to type, while `amenbo` followed
# by anything else is prose about the product ("a newer amenbo (1.9) is available"), which must keep
# the product's name. A command word missing from the list only means one fewer catch, never a false
# alarm — which is why `version` is left out of it: it doubles as a plain noun ("a minimum amenbo
# version"), and a guard that makes an author rewrite correct prose is worse than one that catches
# one case fewer.
#
# Usage: guards/check-cli-name.sh          (no args; scans the two crates' src trees)
# Exit codes: 0 = every worded command takes its name from command_name(), 1 = one is hardcoded.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
trees=("$root/crates/amenbo-core/src" "$root/crates/amenbo-cli/src")
skip="$root/crates/amenbo-core/src/agent.rs"

# The top-level command words, as the CLI registers them.
words='agent|update|config|whoami|init|bind|unbind|status|activity|sync-guide|doctor|validate|lint|hooks|project|dimension|task|comment|decision|attach|export|backup|restore|hard-erase|plugin'

rc=0
found=""

for tree in "${trees[@]}"; do
    [ -d "$tree" ] || { echo "✗ cli name: $tree is missing — did the crates move?" >&2; exit 1; }
    while IFS= read -r -d '' file; do
        [ "$file" = "$skip" ] && continue
        # Cut the test module off, drop comment lines, and keep the line numbers of what is left.
        hits=$(awk '/^[[:space:]]*#\[cfg\(test\)\]/ { exit } { print FNR ":" $0 }' "$file" |
            grep -vE '^[0-9]+:[[:space:]]*//' |
            grep -E "amenbo (${words})([^-_a-zA-Z0-9]|$)" || true)
        if [ -n "$hits" ]; then
            found="$found${found:+$'\n'}${file#"$root"/}
$hits"
            rc=1
        fi
    done < <(find "$tree" -name '*.rs' -type f -print0)
done

if [ "$rc" -ne 0 ]; then
    echo "✗ cli name: a command is worded with the production CLI's name instead of this build's." >&2
    echo "$found" >&2
    echo "  Take the name from amenbo_core::config::Paths::command_name() — on the dev channel the" >&2
    echo "  hardcoded spelling names a command that is not installed." >&2
    exit 1
fi

echo "✓ cli name: every worded command takes its name from command_name()"
