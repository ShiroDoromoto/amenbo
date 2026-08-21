#!/usr/bin/env bash
# build-pkg-mac.sh — package an already-built amenbo GUI .app into a macOS .pkg
# installer that drops the app in ~/Applications and puts the bundled CLI on PATH
# (one installer = GUI + CLI). This is the mac arm of the unified installer;
# the Windows NSIS and Linux deb/rpm packagers are the peers.
#
# Per-user install: the payload lands under the user's home domain —
# GUI at ~/Applications/<app>.app, CLI symlinked to ~/.local/bin/amenbo — so no
# elevation is needed and the executables sit where a per-user self-update can
# replace them without sudo. A productbuild distribution wrapper enables the
# currentUserHome install domain only; a bare component pkg (pkgbuild) has no
# domain and Installer.app would default to the admin-only system domain.
#
# The .app already ships the CLI as a Tauri sidecar at Contents/MacOS/amenbo,
# so PATH exposure is just a symlink created in a postinstall script —
# ~/.local/bin/amenbo → the sidecar inside the installed .app. A symlink (not a
# copy) keeps the CLI and GUI atomically in sync across updates. The postinstall
# also adds ~/.local/bin to the user's login PATH (idempotent).
#
# Signing model: the .app inside already carries the Developer ID Application
# signature and the stapled notarization ticket that codesign-release-mac.sh and
# notarize-mac.sh applied just before this step (pkgbuild does not re-sign or rewrite
# the payload, so both carry through). This script then signs the *container* with the
# **Developer ID Installer** identity — a separate certificate, because an installer
# package is not code and a codeSigning cert cannot sign one. That signature is what
# makes the .pkg tamper-evident on its way to a user; the caller notarizes and staples
# the finished package afterwards. With MAC_SIGN_RELEASE unset the container stays
# unsigned and the .app keeps tauri's ad-hoc signature, so a local build needs no
# signing setup at all.
#
# Usage: build-pkg-mac.sh <app-path> <out-pkg> <version> [arch]
#
# arch (arm64|amd64) is what the payload is *claimed* to be — the .pkg carries no
# arch of its own, so a stale bundle from an earlier build would ship under the other
# arch's name and only fail on the user's Mac. Given, we check the slices we ship.
set -euo pipefail

# shellcheck source=scripts/mac-signing-lib.sh
. "$(dirname "$0")/mac-signing-lib.sh"

APP="${1:?usage: build-pkg-mac.sh <app-path> <out-pkg> <version> [arch]}"
OUT="${2:?usage: build-pkg-mac.sh <app-path> <out-pkg> <version> [arch]}"
VERSION="${3:?usage: build-pkg-mac.sh <app-path> <out-pkg> <version> [arch]}"
ARCH="${4:-}"

[ "$(uname -s)" = "Darwin" ] || { echo "✗ build-pkg-mac.sh is macOS-only (needs pkgbuild)"; exit 1; }
command -v pkgbuild >/dev/null 2>&1 || { echo "✗ pkgbuild not found (Xcode command line tools)"; exit 1; }
command -v productbuild >/dev/null 2>&1 || { echo "✗ productbuild not found (Xcode command line tools)"; exit 1; }
[ -d "$APP" ] || { echo "✗ app bundle not found: $APP"; exit 1; }

# The sidecar CLI the postinstall symlink will target; fail loudly if the sidecar
# bundling regressed rather than shipping a .pkg whose PATH link dangles.
APP_NAME="$(basename "$APP")"
[ -f "$APP/Contents/MacOS/amenbo" ] || { echo "✗ CLI sidecar missing at $APP/Contents/MacOS/amenbo — did the tauri build stage it?"; exit 1; }

# The hourly tick's agent, which the app registers out of its own bundle. It is written
# per build (scripts/write-tick-plist.sh) and bundled by the entry the Makefile passes tauri, so a
# build made without either ships an app that can never register the tick and says only that it is
# "not running from inside" a bundle. Nothing else looks, so this does.
ls "$APP/Contents/Library/LaunchAgents/"*.plist >/dev/null 2>&1 || { echo "✗ the hourly tick's launchd agent is missing from $APP/Contents/Library/LaunchAgents — was the bundle built through make (gui / dist-gui-mac)?"; exit 1; }

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
COMPONENT="$STAGE.component.pkg"
COMPONENT_EXP="$STAGE.component.expanded"
DIST="$STAGE.dist.xml"
cleanup() { rm -rf "$STAGE" "$SCRIPTS" "$PLIST" "$COMPONENT" "$COMPONENT_EXP" "$DIST"; }
trap cleanup EXIT

# Payload: the .app, installed under ~/Applications via the home-domain wrapper below.
cp -R "$APP" "$STAGE/"

# The app's launch name (bundle name without .app) — used to quit/relaunch the
# running GUI below. Channel-agnostic: "amenbo" for prod, "amenbo (dev)" for dev.
APP_LAUNCH_NAME="${APP_NAME%.app}"

# postinstall: (1) expose the bundled CLI on PATH via a symlink into the installed
# app, (2) retire an older *system-wide* install (one-time, admin-gated) so the new
# per-user build is the one that runs, (3) put ~/.local/bin on the user's login PATH,
# then (4) quit the old GUI and relaunch the freshly installed one. Without (4), a GUI
# running during the update keeps showing the "update available" banner until the user
# manually quits and reopens it.
# This is a per-user (currentUserHome) install, so the script runs AS the console
# user — not root — and lands in that user's GUI session directly: no
# `launchctl asuser` is needed to reach it. The one exception is step (2): the old
# system copy is root-owned, so its removal elevates via a single osascript admin
# prompt — the only elevation in the per-user lifetime (self-update never asks). The
# installer passes $2 = the resolved install directory (~/Applications); its parent is
# the user's home.
# Order matters: quit first (else `open` just re-activates the stale instance
# instead of launching the new binary), pause for the quit to land, then open. The
# relaunch/PATH block is best-effort — wrapped so no failure can abort postinstall
# (which would fail the install), with `set -e` scoped to the CLI symlink that must
# succeed. The distributed .app is Developer ID signed and stapled, so this relaunch
# meets no Gatekeeper prompt; a locally built, unsigned one still can.
cat > "$SCRIPTS/postinstall" <<POSTINSTALL
#!/bin/bash
set -e
# \$2 is the resolved install directory (~/Applications); its parent is the user's
# home. Deriving HOME_DIR from \$2 rather than \$HOME keeps this correct even if the
# environment's \$HOME is not the target user's.
APP_DIR="\$2"
HOME_DIR="\$(dirname "\$APP_DIR")"
BIN_DIR="\$HOME_DIR/.local/bin"
mkdir -p "\$BIN_DIR"
ln -sf "\$APP_DIR/$APP_NAME/Contents/MacOS/amenbo" "\$BIN_DIR/amenbo"

# best-effort: retire any old system-wide install, register PATH, and clear the stale
# banner by relaunching the new build.
{
  # One-time migration off an older *system-wide* install. Older releases installed to
  # /Applications/<app> + /usr/local/bin/amenbo (root-owned); the per-user build we just
  # staged must be the one that runs, and /usr/local/bin precedes ~/.local/bin on the
  # stock PATH, so a leftover old CLI would shadow the new one. New is already in place
  # (the symlink above); now retire the old. Root owns it, so this needs one admin prompt
  # — the only elevation in the per-user lifetime; self-update never asks.
  # Idempotent: a fresh or already-migrated user has neither target, so nothing prompts.
  # Channel-safe: OLD_SYS_APP is the app we just installed, by name, under /Applications
  # (a dev bundle never matches the prod system paths); the CLI link is retired only when
  # it resolves into THAT bundle, never a sibling channel's.
  OLD_SYS_APP="/Applications/$APP_NAME"
  OLD_SYS_CLI="/usr/local/bin/amenbo"
  RM_CMD=""
  if [ -d "\$OLD_SYS_APP" ]; then RM_CMD="rm -rf \\"\$OLD_SYS_APP\\""; fi
  if [ -L "\$OLD_SYS_CLI" ]; then
    dest="\$(readlink "\$OLD_SYS_CLI" 2>/dev/null || true)"
    case "\$dest" in
      "\$OLD_SYS_APP"/*)
        [ -n "\$RM_CMD" ] && RM_CMD="\$RM_CMD; "
        RM_CMD="\${RM_CMD}rm -f \\"\$OLD_SYS_CLI\\""
        ;;
    esac
  fi
  if [ -n "\$RM_CMD" ]; then
    export RM_CMD
    /usr/bin/osascript -e 'do shell script (system attribute "RM_CMD") with administrator privileges with prompt "amenbo has moved to a per-user install. Enter your password once to remove the old system-wide copy; future updates need no password."' 2>/dev/null || true
  fi

  # Put ~/.local/bin on the login PATH, once. macOS defaults to zsh; also cover bash.
  # A marker line keeps re-runs (every update) from appending duplicates.
  marker="# added by amenbo installer"
  for rc in "\$HOME_DIR/.zprofile" "\$HOME_DIR/.bash_profile"; do
    if [ ! -e "\$rc" ] || ! grep -qF "\$marker" "\$rc"; then
      printf '\n%s\n%s\n' "\$marker" 'export PATH="\$HOME/.local/bin:\$PATH"' >> "\$rc"
    fi
  done

  # Graceful quit, with a signal fallback, then relaunch the freshly installed build.
  osascript -e 'quit app "$APP_LAUNCH_NAME"' 2>/dev/null || true
  pkill -x amenbo 2>/dev/null || true
  sleep 1
  # Launch with the installer's own TMPDIR dropped. This postinstall runs inside the pkg sandbox
  # (/private/tmp/PKInstallSandbox.<x>/tmp) which the installer removes the moment the install finishes,
  # and open hands the app the environment it was called with -- open(1): "opened applications inherit
  # environment variables just as if you had launched the application directly through its full path".
  # Without this the freshly installed app, and every plugin it starts, is pointed at a directory that is
  # about to be deleted (AMB-T-3461). The app disowns a dead TMPDIR on its own too; this keeps the
  # installer from handing one over in the first place.
  env -u TMPDIR open -a "\$APP_DIR/$APP_NAME" || true
} || true

exit 0
POSTINSTALL
chmod +x "$SCRIPTS/postinstall"

# Component plist: force the app bundle non-relocatable. pkgbuild defaults
# any bundle it packages to relocatable, so the installer honours an existing
# LaunchServices registration for the same bundle id (work.amenbo.app) and lands
# the payload *there* instead of /Applications. On an end-user machine the id is
# unregistered so it still installs to /Applications, but on a dev box where a
# target/…/bundle/macos/Amenbo.app is registered the .pkg redirects into target/.
# --analyze emits a component list (one dict per bundle); flip BundleIsRelocatable
# to false on each so --install-location is honoured unconditionally.
pkgbuild --analyze --root "$STAGE" "$PLIST" >/dev/null
i=0
while /usr/libexec/PlistBuddy -c "Print :$i:BundleIsRelocatable" "$PLIST" >/dev/null 2>&1; do
  /usr/libexec/PlistBuddy -c "Set :$i:BundleIsRelocatable false" "$PLIST"
  i=$((i+1))
done

mkdir -p "$(dirname "$OUT")"

# Component pkg: the payload + scripts. --install-location /Applications is
# interpreted relative to the chosen install domain, so under the home domain the
# wrapper selects below it lands at ~/Applications.
pkgbuild \
  --root "$STAGE" \
  --component-plist "$PLIST" \
  --install-location /Applications \
  --scripts "$SCRIPTS" \
  --identifier work.amenbo.app.installer \
  --version "$VERSION" \
  "$COMPONENT"

# pkgbuild stamps the component `auth="root"`, which would make even the home-domain
# install prompt for admin credentials — defeating the point of a no-elevation
# per-user install. Rewrite it to `auth="none"` so the payload lands under ~/ and the scripts run as
# the user with no password. Requires an expand/patch/flatten round-trip since the
# built pkg is a flat xar and pkgbuild has no auth flag.
pkgutil --expand "$COMPONENT" "$COMPONENT_EXP" >/dev/null
/usr/bin/sed -i '' 's/ auth="root"/ auth="none"/' "$COMPONENT_EXP/PackageInfo"
grep -q ' auth="none"' "$COMPONENT_EXP/PackageInfo" || { echo "✗ failed to set auth=none on the component (per-user install would demand admin)"; exit 1; }
rm -f "$COMPONENT"
pkgutil --flatten "$COMPONENT_EXP" "$COMPONENT"

# Distribution wrapper: enable ONLY the currentUserHome domain, so the install is
# per-user (no elevation) and the payload resolves under ~/. A bare component pkg
# carries no domain and Installer.app would fall back to the admin-only system
# domain. `customize="never"` hides the install-type picker: one fixed choice.
cat > "$DIST" <<DISTXML
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
  <title>amenbo</title>
  <domains enable_anywhere="false" enable_currentUserHome="true" enable_localSystem="false"/>
  <options customize="never"/>
  <choices-outline>
    <line choice="default"/>
  </choices-outline>
  <choice id="default" title="amenbo">
    <pkg-ref id="work.amenbo.app.installer"/>
  </choice>
  <pkg-ref id="work.amenbo.app.installer" version="$VERSION">$(basename "$COMPONENT")</pkg-ref>
</installer-gui-script>
DISTXML

# Sign the product archive with the Developer ID Installer identity when the release
# signing path is on. --sign is given to productbuild rather than run as a separate
# productsign pass: one step, and no window in which an unsigned .pkg exists at the
# release-declared path where a later failure could leave it to be picked up.
sign_args=()
if mac_sign_release_on; then
  installer_identity="$(mac_signing_identity_or_die "Developer ID Installer")"
  sign_args=(--sign "$installer_identity")
fi

productbuild \
  --distribution "$DIST" \
  --package-path "$(dirname "$COMPONENT")" \
  ${sign_args[@]+"${sign_args[@]}"} \
  "$OUT"

echo "→ built $OUT (per-user: installs $APP_NAME to ~/Applications, symlinks CLI to ~/.local/bin/amenbo)"
if mac_sign_release_on; then
  # Report what the signature actually says, not what we asked for. The chain must
  # terminate at Apple's root; a self-signed container would still "have a signature".
  pkgutil --check-signature "$OUT"
else
  echo "  unsigned container (ad-hoc .app inside); Gatekeeper warns on first open (right-click → Open)"
fi
