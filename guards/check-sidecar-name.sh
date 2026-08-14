#!/usr/bin/env bash
# check-sidecar-name.sh — keep the name the app looks for beside itself and the name the bundle
# actually ships the CLI under from drifting apart.
#
# The GUI hands an MCP host a path, not a command word: the host is not a shell and has no PATH of
# the reader's to resolve one in. That path is built by joining a file name onto the directory the
# running binary sits in, and the file it has to land on is the Tauri sidecar — bundled under the
# stem `bundle.externalBin` names, in a config that is one file for every build.
#
# So the name is written twice, in two languages, and nothing else compares them. A rename on either
# side compiles, bundles, and even works in production, where the app's own directory happens to hold
# a file of almost any name one might reach for; what it produces is a manifest pointing at a file
# that is not there, on a machine the author is not sitting at. `Paths::sidecar_file_name()` is the
# Rust side of it and `SIDECAR_NAME` the word it is built from — this guard holds that word against
# the bundle config.
#
# Not scanned here: the mac installer's symlink into `Contents/MacOS/amenbo` names the same file, and
# `guards/check-cli-shim.sh` already holds that one (it is the version-skew invariant's business).
# `app/scripts/prepare-cli-sidecar.mjs` names it too, but a drift there fails the build outright —
# tauri_build refuses a bundle whose declared sidecar is missing.
#
# Usage: guards/check-sidecar-name.sh      (no args; reads the two source files above)
# Exit codes: 0 = both sides name the same file, 1 = they parted.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
conf=$root/app/src-tauri/tauri.conf.json
config_rs=$root/crates/amenbo-core/src/config.rs

rc=0
fail() { echo "✗ sidecar name: $*" >&2; rc=1; }

# The word Rust builds the file name from, read out of the constant that declares it.
declared=""
if [ ! -f "$config_rs" ]; then
    fail "$config_rs is missing — did the config module move?"
else
    declared=$(grep -Eo 'SIDECAR_NAME[^=]*= *"[^"]+"' "$config_rs" | head -1 | sed -E 's/.*"([^"]+)"$/\1/')
    [ -n "$declared" ] || fail "$config_rs no longer declares SIDECAR_NAME.
    That constant is the only place the bundled file's name is written on the Rust side."
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

if [ -n "$declared" ] && [ -n "$bundled" ] && ! printf '%s\n' "$bundled" | grep -qx "$declared"; then
    fail "Rust looks for '$declared' beside the app, but the bundle ships [$(echo "$bundled" | tr '\n' ' ')].
    The manifest the GUI writes would name a file no host can find."
fi

if [ "$rc" -eq 0 ]; then
    echo "✓ sidecar name: the CLI beside the app is looked for under the name the bundle ships it as"
fi
exit "$rc"
