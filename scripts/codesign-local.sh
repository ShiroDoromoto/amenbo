#!/usr/bin/env bash
# codesign-local.sh — sign the installed amenbo binary with a STABLE local
# self-signed code-signing identity so the macOS keychain stops re-prompting.
#
# Why this exists:
#   `make install-dev` / `gui-dev` produce an ad-hoc (linker-signed) binary. macOS
#   keychain pins its "Always Allow" ACL to that binary's *CDHash*, which changes on
#   every rebuild — so the at-rest-key access dialog re-appears every dev cycle. Signing
#   with a stable cert changes the ACL match from "CDHash equals" to "certificate
#   leaf equals", so one "Always Allow" survives all future rebuilds signed with
#   the same cert. (This serves the dev builds only: the prod CLI is not built
#   locally — it ships inside the unified installer.)
#
# This is macOS-only dev tooling. On non-macOS, or when the identity has not been
# set up, it is a clean no-op so Linux/CI builds are unaffected.
#
# Usage:
#   codesign-local.sh sign  <binary> <code-sign-identifier>   # plain binary
#   codesign-local.sh sign  <app.app>                         # .app bundle (deep)
#   codesign-local.sh setup            # create the identity once (idempotent)
set -euo pipefail

IDENTITY="amenbo-codesign"   # common-name / label of the self-signed cert

is_macos() { [ "$(uname -s)" = "Darwin" ]; }
have()     { command -v "$1" >/dev/null 2>&1; }
have_identity() { security find-certificate -c "$IDENTITY" >/dev/null 2>&1; }

cmd_sign() {
  local target="${1:?usage: codesign-local.sh sign <binary-or-.app> [identifier]}"
  local ident="${2:-}"
  # Only meaningful where codesign + the keychain live.
  is_macos || return 0
  have codesign || return 0
  if ! have_identity; then
    echo "→ codesign: identity '$IDENTITY' not set up — leaving ad-hoc signature."
    echo "  (run 'make codesign-cert' once to stop the keychain re-prompting each rebuild)"
    return 0
  fi
  if [ -d "$target" ]; then
    # A .app bundle (e.g. the Tauri GUI): sign inside-out with --deep and keep the
    # bundle's own CFBundleIdentifier. The keychain "Always Allow" ACL keys on the
    # cert leaf, not the identifier, so a stable leaf is all that's needed — the
    # dev GUI is otherwise ad-hoc (linker-signed) and re-prompts every rebuild.
    codesign --force --deep --sign "$IDENTITY" "$target"
    echo "→ codesigned bundle '$target' (stable identity: $IDENTITY)"
  else
    # A plain binary (the CLI): pin the code-sign identifier the caller expects.
    codesign --force --sign "$IDENTITY" ${ident:+--identifier "$ident"} "$target"
    echo "→ codesigned '${ident:-$target}' (stable identity: $IDENTITY)"
  fi
}

cmd_setup() {
  if ! is_macos; then echo "codesign-cert is macOS-only; nothing to do."; return 0; fi
  have security openssl codesign || { echo "need security+openssl+codesign"; return 1; }
  if have_identity; then
    echo "→ identity '$IDENTITY' already present — nothing to do."
    return 0
  fi
  local tmp; tmp="$(mktemp -d)"
  trap 'rm -f "$tmp"/key.pem "$tmp"/legacy.p12' RETURN
  # Self-signed cert with a codeSigning EKU (10y). No CA, digitalSignature only.
  openssl req -x509 -newkey rsa:2048 -keyout "$tmp/key.pem" -out "$tmp/cert.pem" \
    -days 3650 -nodes -subj "/CN=$IDENTITY" \
    -addext "extendedKeyUsage=critical,codeSigning" \
    -addext "basicConstraints=critical,CA:false" \
    -addext "keyUsage=critical,digitalSignature" >/dev/null 2>&1
  # macOS `security` cannot verify OpenSSL 3's default PKCS#12 MAC — use -legacy.
  openssl pkcs12 -export -legacy -macalg sha1 \
    -inkey "$tmp/key.pem" -in "$tmp/cert.pem" \
    -out "$tmp/legacy.p12" -passout pass:setup -name "$IDENTITY" >/dev/null 2>&1
  # -T /usr/bin/codesign lets codesign use the key without a per-sign prompt.
  security import "$tmp/legacy.p12" \
    -k "$HOME/Library/Keychains/login.keychain-db" -P setup -T /usr/bin/codesign
  echo "→ created self-signed code-signing identity '$IDENTITY' in the login keychain."
  echo "  next 'make install-dev' / 'gui-dev' signs with it; approve the keychain dialog once with"
  echo "  'Always Allow' and it will not re-prompt on future rebuilds."
}

case "${1:-}" in
  sign)  shift; cmd_sign "$@" ;;
  setup) cmd_setup ;;
  *) echo "usage: codesign-local.sh sign <binary> <identifier> | setup" >&2; exit 2 ;;
esac
