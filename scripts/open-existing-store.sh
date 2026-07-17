#!/usr/bin/env bash
# open-existing-store.sh — refuse to publish a build that cannot open the store people already have.
#
# What it guards:
#   Every other gate (make test / CI / GUI e2e) creates its store from scratch, so all of them are
#   blind to the one thing a release can break irreversibly: a store written by the *previous*
#   version.
#
#   So: take the binary this release actually ships (extracted from the built .pkg, not a local
#   `cargo build`), point it at a *copy* of the real app-data, and make it open that store and read
#   it back.
#
#   The question is not whether some migration step runs — it is the one underneath it: **can the
#   build we are about to ship open the store that is already out there?** A column whose type
#   changed, a table the read layer now needs — nothing in the suite sees those, because every other
#   store it opens was made by this same build. If any step
#   fails, the release stops here.
#
# It never touches the real store: everything happens in a clone under a throwaway AMENBO_HOME.
#
# Usage: scripts/open-existing-store.sh <pkg> [app-data-dir]
#   KEEP=1  keep the clone (and the extracted binary) for inspection
set -euo pipefail

PKG="${1:?usage: open-existing-store.sh <pkg> [app-data-dir]}"
PROD_HOME="${2:-$HOME/Library/Application Support/work.amenbo.amenbo}"

[ "$(uname -s)" = "Darwin" ] || { echo "✗ open-existing-store.sh is macOS-only (the release runs on the mac)"; exit 1; }
[ -f "$PKG" ] || { echo "✗ installer not found: ${PKG} (run 'make dist-gui-mac' first)"; exit 1; }

if [ ! -d "$PROD_HOME" ]; then
    echo "→ existing-store check: prod store $PROD_HOME is absent, so nothing to do (no store on this machine to open)"
    exit 0
fi

WORK="$(mktemp -d)"
cleanup() {
    if [ "${KEEP:-0}" = "1" ]; then
        echo "  (KEEP=1: left $WORK in place)"
    else
        rm -rf "$WORK"
    fi
}
trap cleanup EXIT

# The binary the users will get, not the one this checkout can build: the .pkg carries the CLI as a
# sidecar inside the .app (build-pkg-mac.sh), and `pkgutil --expand-full` unpacks the payload.
echo "→ existing-store check: extract the shipped CLI from $PKG"
pkgutil --expand-full "$PKG" "$WORK/pkg" >/dev/null
CLI="$WORK/pkg/Payload/amenbo.app/Contents/MacOS/amenbo"
[ -x "$CLI" ] || { echo "✗ no CLI sidecar inside the .pkg: ${CLI} (the bundle is broken)"; exit 1; }

# Clone the real app-data. logs/ and the archives are not store state — they are large and nothing
# here reads them — but everything the store *is* comes across as-is: the point is to hand this build
# the exact shape that is out there.
CLONE="$WORK/home"
echo "→ clone the prod store: $PROD_HOME → $CLONE"
mkdir -p "$CLONE"
rsync -a --exclude 'logs/' --exclude 'backups/' --exclude '*.amenbo-backup' "$PROD_HOME/" "$CLONE/"

# AMENBO_UPDATE_CHECK=0: this must not reach the network, and its cache is not AMENBO_HOME-scoped, so
# a check here would write into the real one.
#
# AMENBO_ACTOR=human, and set here rather than inherited: this asks whether *the build* can open the
# store people already have, which is the release operator's question, not an agent's. An AI's reach is
# its folder's binding, so running the readback as `ai` makes the verdict turn on where the
# release happened to be invoked from — a bound repo passes, an unbound worktree fails with
# `out_of_reach`, and the script reads that as a broken store and stops a healthy release. The gate
# must judge the store, not the caller's CWD. (Nothing of the store's content is seen either way: the
# readback's output goes to /dev/null.)
run() {
    AMENBO_HOME="$CLONE" AMENBO_UPDATE_CHECK=0 AMENBO_ACTOR=human "$CLI" "$@"
}

# Every CLI command takes the **write-path** open (`Store::open()`), so these three already exercise
# the open that stamps `format_version` and the queries the read layer feeds on: `doctor` walks the
# whole store, `status` and `project list` go through the ordinary read model. A column whose type
# moved, a table the read layer now needs — any of that surfaces here.
echo "→ open the prod store with the shipped build and read it back (doctor / status / project list)"
for cmd in doctor status "project list"; do
    # shellcheck disable=SC2086  # intentionally unquoted so the argument splits
    run $cmd >/dev/null || {
        echo "✗ the shipped build cannot open / read back the prod store (amenbo $cmd). **Stop the release** (shipping it makes every user unable to launch — the same failure as 0.1.7)"
        exit 1
    }
done

echo "✓ existing-store check: the shipped build opens the store now out in the wild and reads it back"
