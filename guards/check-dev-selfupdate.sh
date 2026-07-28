#!/usr/bin/env bash
# check-dev-selfupdate.sh — keep the development build out of the self-update business.
#
# The updater endpoint compiled into every GUI bundle is production's manifest, and a dev build's
# version is normally *behind* what that manifest names. So a dev build that is allowed to take an
# offer downloads production and installs it over itself: identifier, executable name and app-data
# all become production's, and the window a developer keeps clicking is no longer the one under
# test. It happened once, and the shape of the mistake is that nothing looks wrong until afterwards.
#
# The channel is the answer, and `amenbo_core::config::Paths::is_dev_channel()` is where it is kept.
# Two independent halves fail closed on it — the plugin that performs the swap is not registered
# (`lib.rs`), and the upstream release the answer is computed from is withheld (`commands.rs`) — and
# this guard is what holds both to it: **any function in the GUI crate that reaches for the updater
# plugin or an upstream update check has to consult the channel in the same function.**
#
# It is a source scan because there is no other way to see it. The channel is stamped in at build
# time (`AMENBO_APP_NAME`), so a test compiled on the production channel exercises the production
# answer and nothing else; the dev branch is never the branch under test. And the failure is silent
# on production, which is the only place CI ever runs the app.
#
# What is scanned: the Rust of the GUI crate (`app/src-tauri/src`). What is not:
#
#   - comment lines (`//`, `///`, `//!`) — a doc link naming the check is prose, not a call.
#   - `use` lines — importing a name grants nothing; the call is what this is about.
#   - the front end (`app/src`), which cannot grant itself the capability: it can only invoke what
#     the Rust side registered and render what the Rust side computed, both of which are held here.
#   - the standalone CLI's own self-update (`crates/amenbo-core/src/self_update.rs`), which replaces
#     a different file by a different route and is not this guard's subject.
#
# Usage: guards/check-dev-selfupdate.sh   (no args; scans the GUI crate's src tree)
# Exit codes: 0 = every reach is guarded by the channel, 1 = one is not.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
tree="$root/app/src-tauri/src"

[ -d "$tree" ] || { echo "✗ dev self-update: $tree is missing — did the GUI crate move?" >&2; exit 1; }

# What counts as reaching for self-update: the updater plugin itself, and the upstream release query
# whose answer becomes `updateAvailable`.
reach='tauri_plugin_updater|update_check::check'

found=""
while IFS= read -r -d '' file; do
    # Walk the file function by function. A hit is unguarded when the function holding it never asks
    # `is_dev_channel`; where in the function it asks does not matter, only that it does.
    hits=$(awk -v reach="$reach" '
        /^[[:space:]]*\/\// { next }
        /^[[:space:]]*use[[:space:]]/ { next }
        /^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?(async[[:space:]]+)?(unsafe[[:space:]]+)?fn[[:space:]]/ { blk++ }
        /is_dev_channel/ { guard[blk] = 1 }
        $0 ~ reach { hits[blk] = hits[blk] sprintf("%d: %s\n", FNR, $0) }
        END { for (b in hits) if (!(b in guard)) printf "%s", hits[b] }
    ' "$file" | sort -n)
    if [ -n "$hits" ]; then
        found="$found${found:+$'\n'}${file#"$root"/}
$hits"
    fi
done < <(find "$tree" -name '*.rs' -type f -print0)

if [ -n "$found" ]; then
    echo "✗ dev self-update: the GUI reaches for self-update without asking which channel it is." >&2
    echo "$found" >&2
    echo "  Gate it on amenbo_core::config::Paths::is_dev_channel() in the same function — a dev" >&2
    echo "  build that takes an offer installs production over the bundle being tested." >&2
    exit 1
fi

echo "✓ dev self-update: every updater reach in the GUI is gated on the channel"
