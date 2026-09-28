#!/usr/bin/env bash
# extract-shipped-cli.sh — take the CLI a mac .pkg ships out of it, and leave no Amenbo.app behind.
#
# Why not simply expand the .pkg and run the CLI where it sits:
#   The expanded payload is an Amenbo.app with the production bundle id (work.amenbo.app). The app the
#   maintainer runs registers a background job, work.amenbo.tick, that runs `Contents/MacOS/amenbo tick
#   run` at the top of every hour — and it names that program by the bundle id, not by a path. A newer
#   Amenbo.app lying anywhere on the machine is a candidate, so the job starts the release under test
#   with no AMENBO_HOME, it opens the real store, and it migrates it past what the running app can open.
#   That happened twice during a release check. So the bundle lives only as long as the copy takes, and
#   what is handed on is a bare binary, which nothing can start by bundle id.
#
#   The bare binary is the same bytes, and passes the shipped-build check the verification harness asks
#   of it.
#
# Usage: scripts/extract-shipped-cli.sh <pkg> <out>
#   <out> is the path the CLI is written to; its directory must exist.
set -euo pipefail

PKG="${1:?usage: extract-shipped-cli.sh <pkg> <out>}"
OUT="${2:?usage: extract-shipped-cli.sh <pkg> <out>}"

[ "$(uname -s)" = "Darwin" ] || { echo "✗ extract-shipped-cli.sh is macOS-only (pkgutil)"; exit 1; }
[ -f "$PKG" ] || { echo "✗ installer not found: ${PKG}"; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# The .pkg is a product archive (productbuild wraps a component pkg for the per-user home domain), so
# the payload sits under a nested <component>.pkg/Payload rather than at the top — find it either way.
pkgutil --expand-full "$PKG" "$WORK/pkg" >/dev/null
CLI="$(find "$WORK/pkg" -type f -path '*/Amenbo.app/Contents/MacOS/amenbo' 2>/dev/null | head -1)"
[ -n "$CLI" ] && [ -x "$CLI" ] || { echo "✗ no CLI sidecar inside the .pkg (the bundle is broken)"; exit 1; }

cp "$CLI" "$OUT"
rm -rf "$WORK/pkg"
# Asked with a throwaway AMENBO_HOME, like every other call of a build under test: nothing it runs
# is pointed at the real store.
mkdir -p "$WORK/home"
echo "→ shipped CLI at $OUT ($(AMENBO_HOME="$WORK/home" AMENBO_UPDATE_CHECK=0 "$OUT" --version))"
