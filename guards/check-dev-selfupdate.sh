#!/usr/bin/env bash
# check-dev-selfupdate.sh — keep the development build out of the self-update business.
#
# The updater endpoint compiled into every GUI bundle is production's manifest, and a dev build's
# version is normally *behind* what that manifest names. So a dev build that is allowed to take an
# offer downloads production and installs it over itself: identifier, executable name and app-data
# all become production's, and the window a developer keeps clicking is no longer the one under
# test. It happened once, and the shape of the mistake is that nothing looks wrong until afterwards.
#
# The CLI has the same opening in a different shape: `~/.cargo/bin/amenbo-dev` is a plain file with
# none of the markers that make `self_update` refuse a bundled CLI, so `amenbo-dev update --apply`
# would fetch the shipped CLI and overwrite itself with it — after which the dev command opens
# production's app-data.
#
# The channel is the answer, and `amenbo_core::config::Paths::is_dev_channel()` is where it is kept.
# This guard holds two different things to it, because the two sides are shaped differently.
#
# **The GUI (a scan).** Two independent halves fail closed there — the plugin that performs the swap
# is not registered (`lib.rs`), and the upstream release the answer is computed from is withheld
# (`commands.rs`) — and any *new* reach would be a third route: **any function in the GUI crate that
# reaches for the updater plugin or an upstream update check has to consult the channel in the same
# function.**
#
# **The CLI and core (a required list).** Here the refusal lives inside the primitives themselves —
# the query is withheld from the channel (`update_check::is_disabled`) and the swap is refused
# (`self_update::apply`) — so every caller, present or future, is covered by construction and a scan
# for reaches would only demand redundant asks. What is left to hold is that the gates are still
# there: each function below must consult the channel in its own body.
#
# It is a source scan because there is no other way to see it. The channel is stamped in at build
# time (`AMENBO_APP_NAME`), so a test compiled on the production channel exercises the production
# answer and nothing else; the dev branch is never the branch under test. And the failure is silent
# on production, which is the only place CI ever runs the app.
#
# What is not read, in either part:
#
#   - comment lines (`//`, `///`, `//!`) — a doc link naming the check is prose, not a call.
#   - `use` lines — importing a name grants nothing; the call is what this is about.
#   - the front end (`app/src`), which cannot grant itself the capability: it can only invoke what
#     the Rust side registered and render what the Rust side computed, both of which are held here.
#
# Usage: guards/check-dev-selfupdate.sh   (no args)
# Exit codes: 0 = every reach is guarded by the channel and every required gate is in place, 1 = one
# is not.
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

# The CLI/core side: the functions that must ask the channel themselves, as `file:function`. Each is a
# gate every caller depends on, so its removal is the regression this half exists to catch.
required=(
    "crates/amenbo-core/src/update_check.rs:is_disabled"   # withholds the upstream query
    "crates/amenbo-core/src/self_update.rs:apply"          # refuses the in-place swap
    "crates/amenbo-cli/src/main.rs:update_cmd"             # words the refusal for `update`
    "crates/amenbo-cli/src/main.rs:self_update_cmd"        # words it for `update --apply`
)

missing=""
for entry in "${required[@]}"; do
    file="${entry%%:*}"
    fn="${entry##*:}"
    path="$root/$file"
    [ -f "$path" ] || { echo "✗ dev self-update: $file is missing — did it move?" >&2; exit 1; }
    verdict=$(awk -v want="$fn" '
        /^[[:space:]]*\/\// { next }
        /^[[:space:]]*use[[:space:]]/ { next }
        /^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?(async[[:space:]]+)?(unsafe[[:space:]]+)?fn[[:space:]]/ {
            blk++
            if ($0 ~ ("fn[[:space:]]+" want "[(<]")) { target[blk] = 1; seen = 1 }
        }
        /is_dev_channel/ { guard[blk] = 1 }
        END {
            if (!seen) { print "gone"; exit }
            for (b in target) if (!(b in guard)) { print "ungated"; exit }
            print "ok"
        }
    ' "$path")
    case "$verdict" in
        ok) ;;
        gone) missing="$missing${missing:+$'\n'}  $file: fn $fn is no longer there" ;;
        *) missing="$missing${missing:+$'\n'}  $file: fn $fn no longer asks the channel" ;;
    esac
done

if [ -n "$missing" ]; then
    echo "✗ dev self-update: a gate the CLI's self-update depends on is not in place." >&2
    echo "$missing" >&2
    echo "  Each of these must call amenbo_core::config::Paths::is_dev_channel() in its own body —" >&2
    echo "  amenbo-dev is a plain file, so an update it applies overwrites it with the shipped CLI." >&2
    echo "  If the gate genuinely moved, move this list with it." >&2
    exit 1
fi

echo "✓ dev self-update: every updater reach in the GUI is gated on the channel, and the CLI's ${#required[@]} gates are in place"
