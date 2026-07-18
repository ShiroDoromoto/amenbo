#!/usr/bin/env bash
# codesign-release-mac.sh — replace the .app's build-time ad-hoc signature with the
# STABLE self-signed release identity, so the code signature (and its Designated
# Requirement) is fixed across versions. macOS keys a notification authorization to
# the app's signature; a fixed leaf is what lets the "allow notifications" grant
# survive an update. Run after `npm run tauri build`, before the .app is packed into
# the .pkg (pkgbuild does not re-sign the payload, so the signature carries through).
#
# The identity name comes from the environment (the release CI sets it from a secret):
#   MAC_SIGN_IDENTITY — the signing identity's common name (e.g. amenbo-release-signing),
#                       already imported into a keychain by import-signing-cert-mac.sh.
#
# MAC_SIGN_IDENTITY unset → clean no-op: the .app keeps tauri's ad-hoc self-signature,
# so a local `make dist-gui-mac` builds without any signing setup.
set -euo pipefail

APP="${1:?usage: codesign-release-mac.sh <app-bundle>}"

[ "$(uname -s)" = "Darwin" ] || { echo "→ codesign-release-mac.sh is macOS-only; nothing to do."; exit 0; }

if [ -z "${MAC_SIGN_IDENTITY:-}" ]; then
  echo "→ MAC_SIGN_IDENTITY unset — leaving the ad-hoc self-signature on $APP."
  exit 0
fi
[ -d "$APP" ] || { echo "✗ app bundle not found: $APP"; exit 1; }

echo "→ codesigning $APP with the stable release identity: $MAC_SIGN_IDENTITY"
# --deep signs the nested code too (the GUI binary and the CLI sidecar); mirrors the
# dev path in codesign-local.sh. Self-signed and un-notarized, so no timestamp.
codesign --force --deep --sign "$MAC_SIGN_IDENTITY" "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"
echo "→ signed. Authority / flags:"
codesign -dv --verbose=4 "$APP" 2>&1 | grep -iE 'Authority|Signature|flags' || true
