#!/usr/bin/env bash
# import-signing-cert-mac.sh — load the Apple Developer ID release signing identities
# into a throwaway keychain so the mac build can sign the .app (Developer ID
# Application) and the .pkg (Developer ID Installer), and so Apple's notary service
# will accept them. Runs on the release CI mac runner before the bundle is built.
#
# Why Developer ID rather than the self-signed identity this replaced: a
# notification authorization persists against the app's Designated Requirement, and a
# Developer ID DR pins to the TEAM ID, not to one certificate's own hash. The
# certificate can then be renewed or reissued without resetting every user's grant,
# which a self-signed leaf could not survive. Notarization is the other half: it
# clears Gatekeeper's first-run warning outright.
#
# Reads from the environment (GitHub Actions secrets):
#   MAC_SIGN_RELEASE              — non-empty switches the whole signing path on
#   MAC_DEVELOPER_ID_P12_BASE64   — one .p12 carrying BOTH Developer ID identities
#                                   (cert + private key), base64
#   MAC_DEVELOPER_ID_P12_PASSWORD — that .p12's export password
#
# MAC_SIGN_RELEASE unset → clean no-op, so a local `make dist-gui-mac` and a fork
# without secrets both still build. With the switch on but no .p12, the identities are
# expected to be in a keychain already (the local verification path on the release
# maintainer's own Mac); the signing scripts fail loudly if they are not.
set -euo pipefail

# shellcheck source=scripts/mac-signing-lib.sh
. "$(dirname "$0")/mac-signing-lib.sh"

[ "$(uname -s)" = "Darwin" ] || { echo "→ import-signing-cert-mac.sh is macOS-only; nothing to do."; exit 0; }

if ! mac_sign_release_on; then
  echo "→ MAC_SIGN_RELEASE unset — leaving the ad-hoc signature (no release identity to import)."
  exit 0
fi

if [ -z "${MAC_DEVELOPER_ID_P12_BASE64:-}" ]; then
  echo "→ no MAC_DEVELOPER_ID_P12_BASE64 — expecting the Developer ID identities to be in a keychain already."
  exit 0
fi
: "${MAC_DEVELOPER_ID_P12_PASSWORD:?MAC_DEVELOPER_ID_P12_PASSWORD is required when MAC_DEVELOPER_ID_P12_BASE64 is set}"

KEYCHAIN="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/amenbo-release-signing.keychain-db"
KC_PASS="$(uuidgen)"                       # ephemeral; the runner is destroyed after the job
TMPD="$(mktemp -d)"; P12="$TMPD/release.p12"; trap 'rm -rf "$TMPD"' EXIT

# Whitespace-strip then decode as one line, so a wrapped base64 secret still decodes.
printf '%s' "$MAC_DEVELOPER_ID_P12_BASE64" | tr -d '[:space:]' | openssl base64 -d -A > "$P12"

# A dedicated keychain, not the login one: nothing else on the runner is touched.
security create-keychain -p "$KC_PASS" "$KEYCHAIN"
security set-keychain-settings -lut 21600 "$KEYCHAIN"   # don't auto-lock mid-build
security unlock-keychain -p "$KC_PASS" "$KEYCHAIN"
# -f pkcs12 is explicit: `security` sniffs format from content and can miss a .p12
# (SecKeychainItemImport "Unknown format"), so name it rather than rely on detection.
# productbuild signs the installer, so it is named alongside codesign as an allowed tool.
security import "$P12" -f pkcs12 -k "$KEYCHAIN" -P "$MAC_DEVELOPER_ID_P12_PASSWORD" \
  -T /usr/bin/codesign -T /usr/bin/productbuild -T /usr/bin/security
# Let codesign/productbuild use the private keys without an interactive prompt.
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KC_PASS" "$KEYCHAIN" >/dev/null
# Prepend our keychain to the search list, keeping the existing ones. Parse each
# existing entry into its own array element (strip the quotes and leading spaces
# `security` prints), so `-s` receives clean, individually-quoted paths — reparsing
# the output as one word-split string mangles the list.
existing=()
while IFS= read -r line; do
  line="${line//\"/}"                       # drop the surrounding quotes
  line="${line#"${line%%[![:space:]]*}"}"   # drop leading whitespace
  line="${line%"${line##*[![:space:]]}"}"   # drop trailing whitespace
  [ -n "$line" ] && existing+=("$line")
done < <(security list-keychains -d user)
security list-keychains -d user -s "$KEYCHAIN" "${existing[@]}"

# Both identities must be here before the build starts. A .p12 exported with only the
# Application half would otherwise sign the .app and then fail at the installer, after
# the whole bundle build — so check now, while the failure is still cheap and legible.
missing=0
for kind in "Developer ID Application" "Developer ID Installer"; do
  if mac_signing_identity "$kind" >/dev/null; then
    echo "  ✓ $kind"
  else
    echo "  ✗ $kind — not in the imported .p12" >&2
    missing=1
  fi
done
[ "$missing" -eq 0 ] || { echo "✗ the .p12 must carry BOTH Developer ID identities." >&2; exit 1; }

echo "→ imported the Developer ID signing identities into $KEYCHAIN"
