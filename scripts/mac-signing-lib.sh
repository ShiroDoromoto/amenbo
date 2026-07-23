# shellcheck shell=bash
# mac-signing-lib.sh — sourced helpers shared by the mac release signing scripts
# (import-signing-cert-mac.sh, codesign-release-mac.sh, build-pkg-mac.sh,
# notarize-mac.sh). Not executable on its own.
#
# One switch drives the whole mac release signing path:
#   MAC_SIGN_RELEASE — non-empty turns Developer ID signing on. Unset is a clean
#                      no-op everywhere, so a local `make dist-gui-mac` and a fork
#                      without secrets still build (tauri's ad-hoc signature, unsigned
#                      .pkg, no notarization).
#
# The signing identities are NEVER named in a secret. They are resolved out of the
# keychain by their well-known Developer ID prefix, which is what keeps the
# certificate holder's legal name out of the repository and out of the secret store
# (it still appears in the signature itself, which is inherent to Developer ID).

# True when the Developer ID release signing path is switched on.
mac_sign_release_on() { [ -n "${MAC_SIGN_RELEASE:-}" ]; }

# Resolve one signing identity to its SHA-1 hash, by the prefix that names its kind
# ("Developer ID Application" / "Developer ID Installer"). Signing by hash rather
# than by name is unambiguous: two identities of the same kind (an expiring one
# alongside its replacement) would otherwise make `codesign` pick by a substring.
#
# Prints the hash on stdout; empty output (and a non-zero return) means no valid
# identity of that kind is reachable.
mac_signing_identity() {
  local kind="${1:?usage: mac_signing_identity <identity-name-prefix>}" hash
  # `find-identity -v` lists every VALID identity as `  1) <sha1> "<common name>"`;
  # -p codesigning is not used because it excludes installer identities, which are
  # not code-signing certs.
  hash="$(security find-identity -v 2>/dev/null \
    | sed -n "s/^ *[0-9]*) \([0-9A-F]*\) \"${kind}:.*\"\$/\1/p" \
    | head -1)"
  [ -n "$hash" ] || return 1
  printf '%s' "$hash"
}

# The same, but a missing identity is fatal — used where MAC_SIGN_RELEASE has already
# promised a signature, so falling back to an unsigned artifact would ship the very
# thing the switch was set to prevent.
mac_signing_identity_or_die() {
  local kind="${1:?usage: mac_signing_identity_or_die <identity-name-prefix>}" hash
  if ! hash="$(mac_signing_identity "$kind")"; then
    echo "✗ MAC_SIGN_RELEASE is set but no valid \"${kind}\" identity is in the keychain." >&2
    echo "  CI: check MAC_DEVELOPER_ID_P12_BASE64 carries both Developer ID identities." >&2
    echo "  Local: check \`security find-identity -v\`." >&2
    return 1
  fi
  printf '%s' "$hash"
}

# True when a complete App Store Connect API key is in the environment, which is what
# notarization needs. Partial credentials are treated as fatal rather than as absent:
# a half-set key is a misconfiguration, and silently skipping notarization would ship
# an artifact Gatekeeper still warns on.
mac_notary_creds_present() {
  local n=0
  [ -n "${MAC_NOTARY_KEY_P8_BASE64:-}" ] && n=$((n + 1))
  [ -n "${MAC_NOTARY_KEY_ID:-}" ] && n=$((n + 1))
  [ -n "${MAC_NOTARY_ISSUER_ID:-}" ] && n=$((n + 1))
  case "$n" in
    3) return 0 ;;
    0) return 1 ;;
    *)
      echo "✗ the notarization credentials are only partly set (${n}/3)." >&2
      echo "  MAC_NOTARY_KEY_P8_BASE64, MAC_NOTARY_KEY_ID and MAC_NOTARY_ISSUER_ID go together." >&2
      exit 1
      ;;
  esac
}
