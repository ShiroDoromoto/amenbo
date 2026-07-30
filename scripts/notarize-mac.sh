#!/usr/bin/env bash
# notarize-mac.sh — submit a signed mac artifact to Apple's notary service, wait for
# the verdict, and staple the resulting ticket onto it.
#
# Notarization is what removes Gatekeeper's first-run warning outright (unlike
# Windows SmartScreen, it is a verdict, not a reputation score), and it puts Apple's
# malware scan in the distribution path. Stapling attaches the ticket to the artifact
# so that verification also succeeds with no network — which matters here, because
# amenbo is local-first and a first launch offline must not look like a rejection.
#
# Usage: notarize-mac.sh <app|pkg> <path>
#   app  — an .app bundle. Submitted as a ditto zip (the notary service takes zip /
#          pkg / dmg, never a bare directory); the TICKET IS STAPLED TO THE .app,
#          not to the throwaway zip.
#   pkg  — an installer package. Submitted and stapled directly.
#
# BOTH are notarized, and the .app is notarized FIRST, because the two travel by
# different routes: the .pkg is the installer, while the .app is separately tarred as
# the updater artifact (amenbo-darwin-<arch>-update.app.tar.gz) that the GUI self-update
# consumes. A ticket stapled only to the .pkg would leave every self-update
# unstapled. Ordering rule the Makefile depends on: staple the .app BEFORE it is
# packaged or tarred, so both copies carry the ticket.
#
# Credentials come from the environment as an App Store Connect API key:
#   MAC_NOTARY_KEY_P8_BASE64 — the .p8 private key, base64
#   MAC_NOTARY_KEY_ID        — the key's Key ID
#   MAC_NOTARY_ISSUER_ID     — the issuer UUID
#
# MAC_SIGN_RELEASE unset → clean no-op (an unsigned local build has nothing to
# notarize). Switched on but with no credentials → a loud warning and a skip, since a
# Developer-ID-signed-but-unnotarized artifact still trips Gatekeeper.
set -euo pipefail

# shellcheck source=scripts/mac-signing-lib.sh
. "$(dirname "$0")/mac-signing-lib.sh"

KIND="${1:?usage: notarize-mac.sh <app|pkg> <path>}"
TARGET="${2:?usage: notarize-mac.sh <app|pkg> <path>}"

[ "$(uname -s)" = "Darwin" ] || { echo "→ notarize-mac.sh is macOS-only; nothing to do."; exit 0; }

if ! mac_sign_release_on; then
  echo "→ MAC_SIGN_RELEASE unset — not notarizing $TARGET."
  exit 0
fi
if ! mac_notary_creds_present; then
  echo "⚠ no notarization credentials — $TARGET is Developer ID signed but NOT notarized (Gatekeeper will still warn)." >&2
  exit 0
fi
[ -e "$TARGET" ] || { echo "✗ artifact not found: $TARGET"; exit 1; }

TMPD="$(mktemp -d)"; trap 'rm -rf "$TMPD"' EXIT
P8="$TMPD/notary.p8"
printf '%s' "$MAC_NOTARY_KEY_P8_BASE64" | tr -d '[:space:]' | openssl base64 -d -A > "$P8"

case "$KIND" in
  app)
    [ -d "$TARGET" ] || { echo "✗ not an .app bundle: $TARGET"; exit 1; }
    # ditto, not zip(1): only ditto preserves the symlinks and extended attributes an
    # .app carries, and a mangled bundle fails notarization for reasons that read
    # nothing like "your zip tool dropped a symlink".
    SUBMIT="$TMPD/notarize.zip"
    /usr/bin/ditto -c -k --keepParent "$TARGET" "$SUBMIT"
    ;;
  pkg)
    [ -f "$TARGET" ] || { echo "✗ not a file: $TARGET"; exit 1; }
    SUBMIT="$TARGET"
    ;;
  *)
    echo "✗ unknown kind: $KIND (expected app or pkg)"; exit 1 ;;
esac

echo "→ notarizing $TARGET"
# --wait blocks until Apple rules. The verdict is read out of the output rather than
# from the exit code: notarytool has historically exited 0 on an Invalid submission,
# so trusting the status alone would ship a rejected artifact under a green build.
out="$(xcrun notarytool submit "$SUBMIT" \
  --key "$P8" --key-id "$MAC_NOTARY_KEY_ID" --issuer "$MAC_NOTARY_ISSUER_ID" \
  --wait --timeout 30m 2>&1)" || true
echo "$out"

submission_id="$(awk '/^ *id: /{print $2; exit}' <<<"$out")"
if ! grep -qE '^ *status: Accepted' <<<"$out"; then
  echo "✗ notarization was not accepted for $TARGET" >&2
  if [ -n "$submission_id" ]; then
    echo "→ notary log for $submission_id:" >&2
    xcrun notarytool log "$submission_id" \
      --key "$P8" --key-id "$MAC_NOTARY_KEY_ID" --issuer "$MAC_NOTARY_ISSUER_ID" >&2 || true
  fi
  exit 1
fi

# Staple the TARGET, never the submitted zip: the zip was only a transport.
echo "→ stapling the ticket onto $TARGET"
xcrun stapler staple "$TARGET"
xcrun stapler validate "$TARGET"

# Gatekeeper's own verdict, which is the thing a user actually meets. `spctl` assesses
# an .app as execution and a .pkg as an install, so the type differs by artifact.
case "$KIND" in
  app) spctl -a -vvv -t exec "$TARGET" ;;
  pkg) spctl -a -vvv -t install "$TARGET" ;;
esac

echo "→ notarized and stapled: $TARGET"
