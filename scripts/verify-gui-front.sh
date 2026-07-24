#!/usr/bin/env bash
# verify-gui-front.sh — say which frontend a built (or installed) .app carries, and fail when it
# is not the one currently in app/dist.
#
# Why this exists:
#   A GUI dev cycle is `make install-gui-dev` → click the app → look at it. When the bundle carries
#   an older frontend, the app simply looks unchanged — which reads as "my change does not work",
#   so the next hour goes into the implementation instead of the build. The failure is silent, and
#   silence is the whole problem: a stale bundle and a wrong implementation look identical.
#
#   What is *not* the cause (measured against tauri 2.6.3, not assumed): cargo does see the
#   frontend. `generate_context!` embeds every dist file as `include_bytes!`, so they land in the
#   crate's dep-info and a frontend-only edit does force a rebuild. Quitting is synchronous too —
#   `osascript quit` neither launches a stopped app nor returns before it is gone, so the rsync
#   that follows cannot land under a still-running instance. What remains are the causes we cannot
#   enumerate (another session installing over the shared dev app, a partial copy), and the
#   answer to a cause you cannot enumerate is to check the outcome rather than to prevent it.
#
# How it can tell:
#   Tauri compresses the asset *bodies* but keeps their keys as plain strings, so the entry chunk
#   named by dist/index.html (`/assets/index-<hash>.js`) is greppable in the binary — and it *is*
#   the frontend's identity, because vite renames it on every content change.
#
# Usage: verify-gui-front.sh <app-bundle.app> [dist-dir]
set -euo pipefail

APP="${1:?usage: verify-gui-front.sh <app-bundle.app> [dist-dir]}"
DIST="${2:-app/dist}"
INDEX="$DIST/index.html"
BIN="$APP/Contents/MacOS/amenbo-app"

[ -f "$INDEX" ] || { echo "✗ nothing to compare against: $INDEX is missing (build the frontend first)"; exit 1; }
[ -f "$BIN" ] || { echo "✗ not an amenbo app bundle: $BIN is missing"; exit 1; }

# What index.html loads directly = the entry chunk and its stylesheet. Everything else is reached
# through them, so these two are enough to name the build.
wanted=$(grep -Eo '/assets/[A-Za-z0-9._-]+\.(js|css)' "$INDEX" | sort -u)
[ -n "$wanted" ] || { echo "✗ $INDEX names no /assets/… entry — is this a vite build?"; exit 1; }

embedded=$(strings -a "$BIN" | grep -Eo '/assets/[A-Za-z0-9._-]+\.(js|css)' | sort -u || true)
missing=$(comm -23 <(printf '%s\n' "$wanted") <(printf '%s\n' "$embedded"))

if [ -n "$missing" ]; then
  echo "✗ the bundle carries a different frontend than app/dist:"
  echo "$missing" | sed 's/^/    expected (app\/dist): /'
  printf '%s\n' "$embedded" | grep -E '/assets/index-[A-Za-z0-9._-]+\.js' | sed 's/^/    bundled  (the .app): /' || true
  echo "  → $APP"
  echo "  Build it again. If it keeps happening and this is the shared 'amenbo (dev).app', check"
  echo "  that no other session is installing over it — build into the task's own instance instead"
  echo "  (make install-gui-dev AMB-T-ID=<id>), which nobody else can reach."
  exit 1
fi

echo "→ frontend verified: $(printf '%s\n' "$wanted" | grep -E '\.js$' | head -1) ($APP)"
