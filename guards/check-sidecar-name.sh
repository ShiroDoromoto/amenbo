#!/usr/bin/env bash
# check-sidecar-name.sh — keep the names the app looks for beside itself and the names the bundle
# actually ships its two sidecars under from drifting apart.
#
# Two files are shipped beside the app and both are reached as a path rather than as a command word.
# The GUI hands an MCP host a path, because the host is not a shell and has no PATH of the reader's
# to resolve one in; and it hands git a path in `GIT_ASKPASS`, because git runs the file rather than
# looking a word up. Each path is built by joining a file name onto the directory the running binary
# sits in, and the file it has to land on is a Tauri sidecar — bundled under the stem
# `bundle.externalBin` names.
#
# The stem is not one word for every build. The Windows installer puts the bundled CLI on PATH, so a
# stem shared across channels puts production, the shared dev build and every theme preview there
# under one name, resolved by whichever was installed first. Each build therefore ships the CLI under
# its own app-data name, which is also what `Paths::command_name()` answers — and the name is written
# in three languages that nothing else compares:
#
#   * Rust     — `PRODUCTION_APP_NAME`, the default `APP_NAME` takes when nothing overrides it.
#     `Paths::sidecar_file_name()` and `Paths::askpass_file_name()` are that word and that word plus
#     `-askpass`, which is why one constant answers for both files.
#   * The base bundle config — `bundle.externalBin`, which is what a production build ships.
#   * The Makefile — `GUI_DEV_CONFIG`'s own `externalBin`, over `GUI_DEV_DATA`, which is the app-data
#     name the same build compiles in as `AMENBO_APP_NAME`. A dev or theme build that split one and
#     not the other would bundle a file its own guidance does not name.
#
# A rename on any side still bundles, still builds, and in production still lands on a file that
# happens to be there; what it produces is a manifest pointing at a file that is not, on a machine
# the author is not sitting at.
#
# Not scanned here: the mac installer's symlink into `Contents/MacOS/amenbo` names the same file, and
# `guards/check-cli-shim.sh` already holds that one (it is the version-skew invariant's business).
# `app/scripts/prepare-sidecars.mjs` names it too, but a drift there fails the build outright —
# tauri_build refuses a bundle whose declared sidecar is missing.
#
# Usage: guards/check-sidecar-name.sh      (no args; reads the three sources above)
# Exit codes: 0 = every side names the same file, 1 = they parted.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
conf=$root/app/src-tauri/tauri.conf.json
config_rs=$root/crates/amenbo-core/src/config.rs
makefile=$root/Makefile

rc=0
fail() { echo "✗ sidecar name: $*" >&2; rc=1; }

# The word Rust falls back to when no build overrides the channel — production's name.
declared=""
if [ ! -f "$config_rs" ]; then
    fail "$config_rs is missing — did the config module move?"
else
    declared=$(grep -Eo 'PRODUCTION_APP_NAME[^=]*= *"[^"]+"' "$config_rs" | head -1 | sed -E 's/.*"([^"]+)"$/\1/')
    [ -n "$declared" ] || fail "$config_rs no longer declares PRODUCTION_APP_NAME.
    That constant is what a production build bundles, reads and answers to."
fi

# The stems Tauri bundles a sidecar under, one per line.
bundled=""
if [ ! -f "$conf" ]; then
    fail "$conf is missing — the GUI bundle config moved?"
else
    bundled=$(python3 - "$conf" <<'PY'
import json, os, sys

with open(sys.argv[1]) as f:
    conf = json.load(f)
for entry in conf.get("bundle", {}).get("externalBin") or []:
    print(os.path.basename(str(entry)))
PY
    )
fi

if [ -n "$declared" ] && [ -n "$bundled" ]; then
    if ! printf '%s\n' "$bundled" | grep -qx "$declared"; then
        fail "Rust looks for '$declared' beside the app, but the bundle ships [$(echo "$bundled" | tr '\n' ' ')].
    The manifest the GUI writes would name a file no host can find."
    fi
    if ! printf '%s\n' "$bundled" | grep -qx "$declared-askpass"; then
        fail "the bundle ships no '$declared-askpass' [$(echo "$bundled" | tr '\n' ' ')].
    git would be pointed at a helper that is not there, and a call needing a password would fail
    with no dialog — the failure this build is meant to have stopped happening."
    fi
fi

# The dev channel's own override, and the app-data name it has to agree with. Both are make
# variables, so make itself is asked rather than the text being parsed twice.
if [ ! -f "$makefile" ]; then
    fail "$makefile is missing — where the dev channel's names are composed."
else
    dev_data=$(make -s -C "$root" gui-dev-names | sed -n 's/^app_name=//p')
    dev_stem=$(make -s -C "$root" gui-dev-names | sed -n 's/^config=//p' | python3 -c '
import json, os, sys

conf = json.loads(sys.stdin.read())
for entry in conf.get("bundle", {}).get("externalBin") or []:
    print(os.path.basename(str(entry)))
')
    if [ -z "$dev_stem" ]; then
        fail "GUI_DEV_CONFIG in the Makefile no longer overrides bundle.externalBin.
    A dev or theme build would ship the CLI as '$declared', putting every channel on the Windows PATH under one name."
    else
        if ! printf '%s\n' "$dev_stem" | grep -qx "$dev_data"; then
            fail "the dev channel bundles [$(echo "$dev_stem" | tr '\n' ' ')] but compiles in '$dev_data' as its own name.
    Its guidance would tell a reader to type a command the bundle does not carry."
        fi
        if ! printf '%s\n' "$dev_stem" | grep -qx "$dev_data-askpass"; then
            fail "the dev channel bundles [$(echo "$dev_stem" | tr '\n' ' ')] and no '$dev_data-askpass'.
    Either it ships no helper at all, or it ships production's — which is the one name on PATH that
    two installed channels would both answer to."
        fi
    fi
fi

if [ "$rc" -eq 0 ]; then
    echo "✓ sidecar name: every channel ships the CLI and the askpass helper under the names it answers to"
fi
exit "$rc"
