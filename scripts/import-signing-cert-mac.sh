#!/usr/bin/env bash
# import-signing-cert-mac.sh — load the stable, self-signed release code-signing
# identity into a throwaway keychain so `codesign` can sign the mac .app during the
# build (scripts/codesign-release-mac.sh). Runs on the release CI mac runner before
# the bundle is built; the identity is Apple-independent (a self-signed codeSigning
# cert), so the leaf stays fixed across versions and the end user's notification
# authorization survives updates.
#
# Reads the identity from the environment (GitHub Actions secrets):
#   MAC_SIGN_P12_BASE64   — the .p12 (cert + private key), base64
#   MAC_SIGN_P12_PASSWORD — the .p12 export password
#
# No MAC_SIGN_P12_BASE64 → clean no-op (a local `make dist-gui-mac` with no secret
# leaves tauri's build-time ad-hoc signature untouched; only the release CI signs).
set -euo pipefail

[ "$(uname -s)" = "Darwin" ] || { echo "→ import-signing-cert-mac.sh is macOS-only; nothing to do."; exit 0; }

if [ -z "${MAC_SIGN_P12_BASE64:-}" ]; then
  echo "→ MAC_SIGN_P12_BASE64 unset — leaving the ad-hoc signature (no release identity to import)."
  exit 0
fi
: "${MAC_SIGN_P12_PASSWORD:?MAC_SIGN_P12_PASSWORD is required when MAC_SIGN_P12_BASE64 is set}"

KEYCHAIN="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/amenbo-release-signing.keychain-db"
KC_PASS="$(uuidgen)"                       # ephemeral; the runner is destroyed after the job
TMPD="$(mktemp -d)"; P12="$TMPD/release.p12"; trap 'rm -rf "$TMPD"' EXIT

# Whitespace-strip then decode as one line, so a wrapped base64 secret still decodes.
printf '%s' "$MAC_SIGN_P12_BASE64" | tr -d '[:space:]' | openssl base64 -d -A > "$P12"

# A dedicated keychain, not the login one: nothing else on the runner is touched.
security create-keychain -p "$KC_PASS" "$KEYCHAIN"
security set-keychain-settings -lut 21600 "$KEYCHAIN"   # don't auto-lock mid-build
security unlock-keychain -p "$KC_PASS" "$KEYCHAIN"
# -f pkcs12 is explicit: `security` sniffs format from content and can miss a .p12
# (SecKeychainItemImport "Unknown format"), so name it rather than rely on detection.
security import "$P12" -f pkcs12 -k "$KEYCHAIN" -P "$MAC_SIGN_P12_PASSWORD" -T /usr/bin/codesign -T /usr/bin/security
# Let codesign use the private key without an interactive prompt.
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

echo "→ imported the release signing identity into $KEYCHAIN"
echo "  (codesign matches it by common name; see scripts/codesign-release-mac.sh)"
