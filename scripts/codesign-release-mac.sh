#!/usr/bin/env bash
# codesign-release-mac.sh — replace the .app's build-time ad-hoc signature with the
# Apple **Developer ID Application** identity, under the hardened runtime and with a
# secure timestamp. Run after `npm run tauri build`, before the .app is notarized and
# packed into the .pkg (pkgbuild does not re-sign the payload, so both the signature
# and the stapled ticket carry through).
#
# Two things ride on this signature:
#   - Notarization. Apple's notary service REJECTS anything not signed with a
#     Developer ID under `--options runtime` and `--timestamp`; those two flags are
#     not hardening for its own sake, they are the price of admission.
#   - The user's notification authorization. macOS keys the grant to the app's
#     Designated Requirement, and a Developer ID DR pins to the TEAM ID rather than to
#     one certificate's own hash — so renewing the certificate does not reset it,
#     which the self-signed identity this replaced could not manage.
#
# The identity is resolved out of the keychain by its well-known prefix rather than
# named in a secret, so the certificate holder's legal name lives in no config file.
# import-signing-cert-mac.sh puts it there on CI.
#
# MAC_SIGN_RELEASE unset → clean no-op: the .app keeps tauri's ad-hoc self-signature,
# so a local `make dist-gui-mac` builds without any signing setup.
set -euo pipefail

# shellcheck source=scripts/mac-signing-lib.sh
. "$(dirname "$0")/mac-signing-lib.sh"

APP="${1:?usage: codesign-release-mac.sh <app-bundle>}"

[ "$(uname -s)" = "Darwin" ] || { echo "→ codesign-release-mac.sh is macOS-only; nothing to do."; exit 0; }

if ! mac_sign_release_on; then
  echo "→ MAC_SIGN_RELEASE unset — leaving the ad-hoc self-signature on $APP."
  exit 0
fi
[ -d "$APP" ] || { echo "✗ app bundle not found: $APP"; exit 1; }

IDENTITY="$(mac_signing_identity_or_die "Developer ID Application")"

echo "→ codesigning $APP with the Developer ID Application identity (hardened runtime + timestamp)"

# Sign inside-out, not with --deep. Apple deprecated --deep for distribution: it
# applies ONE set of options to everything it reaches, which is exactly wrong once
# nested code needs the hardened runtime in its own right — and a bundle whose nested
# code was sealed with the wrong options is rejected at notarization, long after the
# build. So: every nested Mach-O first (the CLI sidecar `amenbo`, and whatever a
# future bundle layout adds), then the bundle itself last, which seals the main
# executable.
#
# The main executable is deliberately NOT in that first pass. Handed its main
# executable's path, codesign signs the enclosing BUNDLE — so signing it here would
# seal the bundle while the sidecar beside it is still bare, and codesign refuses
# ("code object is not signed at all / In subcomponent"). The asymmetry is easy to
# miss on an arm64 host: arm64 Mach-O must carry a signature to run at all, so the
# linker ad-hoc signs it and the premature seal finds nothing bare. An x86_64 slice
# carries no such signature, so the cross build is where it bites.
main_exe="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP/Contents/Info.plist" 2>/dev/null || true)"
[ -n "$main_exe" ] || { echo "✗ $APP/Contents/Info.plist names no CFBundleExecutable" >&2; exit 1; }

nested=()
while IFS= read -r -d '' f; do
  if [ "$f" = "$APP/Contents/MacOS/$main_exe" ]; then continue; fi
  case "$(file -b "$f")" in
    Mach-O*) nested+=("$f") ;;
  esac
done < <(find "$APP/Contents" -type f -print0)

for f in ${nested[@]+"${nested[@]}"}; do
  echo "  · nested: ${f#"$APP/"}"
  codesign --force --options runtime --timestamp --sign "$IDENTITY" "$f"
done

codesign --force --options runtime --timestamp --sign "$IDENTITY" "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"

echo "→ signed. Authority / flags:"
codesign -dv --verbose=4 "$APP" 2>&1 | grep -iE 'Authority|Timestamp|Signature|flags' || true

# A Team-ID-anchored Designated Requirement is the whole reason for moving off the
# self-signed identity — it is what lets a certificate renewal happen without
# resetting every user's notification grant. So assert it rather than trust it: the DR
# must be anchored to Apple and pinned to a team OU, never to a bare leaf hash.
echo "→ designated requirement:"
dr="$(codesign -d -r- "$APP" 2>/dev/null | sed -n 's/^designated => //p')"
echo "  $dr"
case "$dr" in
  *"anchor apple generic"*"subject.OU"*) ;;
  *)
    echo "✗ the designated requirement is not Team-ID-anchored — a certificate renewal would reset every user's notification authorization." >&2
    exit 1
    ;;
esac
