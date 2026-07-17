#!/usr/bin/env bash
# build-pkg-mac.sh — package an already-built amenbo GUI .app into a macOS .pkg
# installer that drops the app in /Applications and puts the bundled CLI on PATH
# (one installer = GUI + CLI). This is the mac arm of the unified installer;
# the Windows NSIS and Linux deb/rpm packagers are the peers.
#
# The .app already ships the CLI as a Tauri sidecar at Contents/MacOS/amenbo,
# so PATH exposure is just a symlink created in a postinstall script —
# /usr/local/bin/amenbo → the sidecar inside the installed .app. A symlink (not a
# copy) keeps the CLI and GUI atomically in sync across updates.
#
# Signing model (mirrors the dmg): the .pkg is an unsigned *container*; the .app
# and its nested binaries inside are signed during the tauri build (the release
# identity is a codeSigning cert, which `productsign` cannot use for installer
# signing). Self-signed, not notarized — Gatekeeper warns on first open
# (right-click → Open), the accepted stance. The end user's keychain
# "Always Allow" ACL keys on the .app/CLI cert leaf, which the tauri build set —
# not on the installer package signature.
#
# Usage: build-pkg-mac.sh <app-path> <out-pkg> <version> [arch]
#
# arch (arm64|amd64) is what the payload is *claimed* to be — the .pkg carries no
# arch of its own, so a stale bundle from an earlier build would ship under the other
# arch's name and only fail on the user's Mac. Given, we check the slices we ship.
set -euo pipefail

APP="${1:?usage: build-pkg-mac.sh <app-path> <out-pkg> <version> [arch]}"
OUT="${2:?usage: build-pkg-mac.sh <app-path> <out-pkg> <version> [arch]}"
VERSION="${3:?usage: build-pkg-mac.sh <app-path> <out-pkg> <version> [arch]}"
ARCH="${4:-}"

[ "$(uname -s)" = "Darwin" ] || { echo "✗ build-pkg-mac.sh is macOS-only (needs pkgbuild)"; exit 1; }
command -v pkgbuild >/dev/null 2>&1 || { echo "✗ pkgbuild not found (Xcode command line tools)"; exit 1; }
[ -d "$APP" ] || { echo "✗ app bundle not found: $APP"; exit 1; }

# The sidecar CLI the postinstall symlink will target; fail loudly if the sidecar
# bundling regressed rather than shipping a .pkg whose PATH link dangles.
APP_NAME="$(basename "$APP")"
[ -f "$APP/Contents/MacOS/amenbo" ] || { echo "✗ CLI sidecar missing at $APP/Contents/MacOS/amenbo — did the tauri build stage it?"; exit 1; }

# Both binaries we ship must carry the slice this .pkg claims: the GUI (amenbo-app) and
# the sidecar CLI (amenbo) it symlinks onto PATH.
if [ -n "$ARCH" ]; then
  case "$ARCH" in
    arm64) want=arm64 ;;
    amd64) want=x86_64 ;;
    *) echo "✗ unknown arch: $ARCH (expected arm64 or amd64)"; exit 1 ;;
  esac
  for bin in amenbo-app amenbo; do
    archs="$(lipo -archs "$APP/Contents/MacOS/$bin")"
    case " $archs " in
      *" $want "*) ;;
      *) echo "✗ arch mismatch: $APP/Contents/MacOS/$bin is [$archs] but this .pkg is $ARCH ($want)"; exit 1 ;;
    esac
  done
  echo "  arch: $ARCH — GUI and bundled CLI both carry the $want slice"
fi

STAGE="$(mktemp -d)"
SCRIPTS="$(mktemp -d)"
PLIST="$STAGE.plist"
cleanup() { rm -rf "$STAGE" "$SCRIPTS" "$PLIST"; }
trap cleanup EXIT

# Payload: the .app installed under /Applications.
cp -R "$APP" "$STAGE/"

# The app's launch name (bundle name without .app) — used to quit/relaunch the
# running GUI below. Channel-agnostic: "amenbo" for prod, "amenbo (dev)" for dev.
APP_LAUNCH_NAME="${APP_NAME%.app}"

# postinstall: (1) expose the bundled CLI on PATH via a symlink into the installed
# app, then (2) quit the old GUI and relaunch the freshly installed one.
# Without (2), a GUI running during the update keeps showing the "update available"
# banner until the user manually quits and reopens it.
# postinstall runs as *root*, so root's own Apple Events / `open` never reach the
# console user's GUI session — BOTH the quit and the relaunch must go through
# `launchctl asuser <uid>` (uid = the logged-in console user) to land in that
# session. Order matters: quit first (else `open -a` just re-activates the stale
# instance instead of launching the new binary), pause for the quit to land, then
# open. The whole block is best-effort — wrapped so no failure can abort postinstall
# (which would fail the install), with `set -e` scoped to the CLI symlink that must
# succeed. Self-signed/un-notarized, so the relaunched app may hit Gatekeeper on
# first open — verified at release time.
cat > "$SCRIPTS/postinstall" <<POSTINSTALL
#!/bin/bash
set -e
BIN_DIR=/usr/local/bin
mkdir -p "\$BIN_DIR"
ln -sf "/Applications/$APP_NAME/Contents/MacOS/amenbo" "\$BIN_DIR/amenbo"

# best-effort: clear the stale banner by relaunching the new build.
{
  uid=\$(stat -f %u /dev/console 2>/dev/null || true)
  if [ -n "\$uid" ]; then
    # Graceful quit inside the user's session (root's own osascript can't reach it),
    # with a signal fallback, then relaunch the freshly installed build.
    launchctl asuser "\$uid" osascript -e 'quit app "$APP_LAUNCH_NAME"' 2>/dev/null || true
    pkill -x amenbo 2>/dev/null || true
    sleep 1
    launchctl asuser "\$uid" open -a "/Applications/$APP_NAME" || true
  fi
} || true

exit 0
POSTINSTALL
chmod +x "$SCRIPTS/postinstall"

# Component plist: force the app bundle non-relocatable. pkgbuild defaults
# any bundle it packages to relocatable, so the installer honours an existing
# LaunchServices registration for the same bundle id (work.amenbo.app) and lands
# the payload *there* instead of /Applications. On an end-user machine the id is
# unregistered so it still installs to /Applications, but on a dev box where a
# target/…/bundle/macos/amenbo.app is registered the .pkg redirects into target/.
# --analyze emits a component list (one dict per bundle); flip BundleIsRelocatable
# to false on each so --install-location is honoured unconditionally.
pkgbuild --analyze --root "$STAGE" "$PLIST" >/dev/null
i=0
while /usr/libexec/PlistBuddy -c "Print :$i:BundleIsRelocatable" "$PLIST" >/dev/null 2>&1; do
  /usr/libexec/PlistBuddy -c "Set :$i:BundleIsRelocatable false" "$PLIST"
  i=$((i+1))
done

mkdir -p "$(dirname "$OUT")"
pkgbuild \
  --root "$STAGE" \
  --component-plist "$PLIST" \
  --install-location /Applications \
  --scripts "$SCRIPTS" \
  --identifier work.amenbo.app.installer \
  --version "$VERSION" \
  "$OUT"

echo "→ built $OUT (installs $APP_NAME to /Applications, symlinks CLI to /usr/local/bin/amenbo)"
echo "  unsigned container (self-signed .app inside); Gatekeeper warns on first open (right-click → Open)"
