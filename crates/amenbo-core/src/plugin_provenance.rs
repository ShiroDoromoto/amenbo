//! Provenance verification for a downloaded plugin asset — the trust that turns untrusted network bytes
//! into something Amenbo will run (`AMB-D-371`, `AMB-D-351`).
//!
//! Two fail-closed checks, split by cost and by what they prove:
//!
//! - [`verify_checksum`] — the asset's SHA-256 matches the manifest's `checksum` (`sha256:<hex>`). This is
//!   the **integrity** half, and it is cheap, so it is the check re-run **every time** the on-disk asset
//!   is used, to catch a post-install swap (`AMB-D-351`).
//! - [`verify_signature`] — a minisign signature over the asset verifies against Amenbo's **catalog public
//!   key** (`AMB-D-371`, the catalog-key trust model). This is the **origin** half, and it is heavier, so
//!   it runs **once** at download: the asset was blessed by the CI of the catalog that lists it.
//!
//! [`verify_against`] runs both against the root that catalog answers for — the key Amenbo ships for the
//! official catalog, the key a registration pinned for any other (`AMB-D-389`) — and is the door an
//! install (`AMB-T-2050`) and an update (`AMB-D-359`) call before an asset is ever written enabled. An
//! asset with no signature, or one signed by a key its own catalog is not trusted on, does not verify,
//! and so cannot be installed or enabled (`AMB-D-351`). [`verify_asset`] is the same door with the key as
//! a plain argument, for a test that must sign its own fixtures.
//!
//! **Why the public key ships to every device is safe.** The catalog **private** key lives only in the
//! catalog CI; every Amenbo carries only the **public** key ([`CATALOG_PUBLIC_KEY`]), which can verify
//! but never sign — the same shape as the updater public key in `tauri.conf.json`, and every TLS /
//! OS-code-signing trust store.
//!
//! A registered third-party catalog is verified against **its own** key rather than this one
//! (`AMB-D-389`), so the key is a parameter there and the two pieces that make one readable —
//! [`read_public_key`] out of a published `.pub` file, and [`key_fingerprint`] for the human being
//! asked to consent — live here too, beside the shape they both know.
//!
//! This module is verification only: it does not fetch, download, or store. The caller supplies the bytes
//! (from the network at install, or from disk at run) and the manifest fields.

use crate::error::{Error, ErrorCode, Msg, Result};
use minisign_verify::{PublicKey, Signature};
use sha2::{Digest, Sha256};

/// The one checksum algorithm Amenbo understands in a manifest. Pinned to SHA-256 so a manifest cannot
/// name a weaker digest and quietly downgrade the integrity check.
const CHECKSUM_PREFIX: &str = "sha256:";

/// Amenbo's **catalog public key** — the single trust root for plugin assets (`AMB-D-371`). Key id
/// `6272CBB782CB57A0`, deliberately not the updater's key (`2F151276522ADC1D`, in `tauri.conf.json`):
/// a plugin and a release are blessed by separate roots, so one compromised root does not carry the other.
///
/// This is the public half, which verifies and cannot sign. The private half exists only as a secret of
/// the catalog CI (`AMB-T-2054`), which signs each published asset and then re-verifies that signature
/// against its own copy of this key before writing `catalog.json` — so "the key Amenbo ships" and "the key
/// that signed" are proven to be one key on every catalog run. The catalog repository holds the identical
/// value in `catalog-key.pub`.
///
/// Rotating it takes a new Amenbo release: an asset signed by a key no installed Amenbo carries verifies
/// nowhere. That is the fail-closed direction — a key Amenbo does not know can never bless anything.
pub const CATALOG_PUBLIC_KEY: &str = "RWSgV8uCt8tyYg74JbwBblWoE+g7bxSGvK8blkKW7gUo3EuBXaqy5oMR";

/// Verify `bytes` hash to the digest the manifest recorded in `checksum` — the `sha256:<hex>` integrity
/// half of provenance (`AMB-D-351`). Cheap enough to re-run on every use of the on-disk asset, which is
/// how a post-install swap is caught.
///
/// Fail-closed: an unknown algorithm prefix, a malformed hex digest, or a digest that does not match the
/// bytes all refuse. The digest is public (it lives in the manifest), so the comparison need not be
/// constant-time.
pub fn verify_checksum(bytes: &[u8], checksum: &str) -> Result<()> {
    let hex = checksum.strip_prefix(CHECKSUM_PREFIX).ok_or_else(|| {
        Error::Invalid(
            Msg::new(format!(
                "unsupported checksum format (expected '{CHECKSUM_PREFIX}<hex>'): {checksum}"
            ))
            .coded(ErrorCode::InvalidPluginChecksumFormat)
            .with("checksum", checksum),
        )
    })?;
    let expected = decode_sha256_hex(hex)?;
    let actual = Sha256::digest(bytes);
    if actual.as_slice() != expected.as_slice() {
        return Err(Error::Invalid(
            Msg::new("asset checksum mismatch: the bytes are not what the manifest recorded")
                .coded(ErrorCode::InvalidPluginChecksumMismatch),
        ));
    }
    Ok(())
}

/// Decode a 64-char hex string into the 32 SHA-256 bytes. Rejects anything but exactly 64 hex digits.
fn decode_sha256_hex(hex: &str) -> Result<[u8; 32]> {
    let hex = hex.trim();
    if hex.len() != 64 {
        return Err(Error::Invalid(
            Msg::new(format!("a sha256 digest is 64 hex chars, got {}", hex.len()))
                .coded(ErrorCode::InvalidPluginChecksumLength)
                .with("length", hex.len()),
        ));
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| {
            Error::Invalid(
                Msg::new(format!("checksum digest is not valid hex: {hex}"))
                    .coded(ErrorCode::InvalidPluginChecksumNotHex)
                    .with("digest", hex),
            )
        })?;
    }
    Ok(out)
}

/// Verify a minisign `signature` (the full `.minisig` text) over `bytes` against `public_key` (a minisign
/// base64 public key) — the origin half of provenance (`AMB-D-371`, catalog-key trust model). A pass means
/// the bytes were signed by whoever holds the matching private key, i.e. the Amenbo catalog CI.
///
/// Fail-closed: a malformed key, a malformed signature, or a signature that does not verify against the
/// bytes and the key all refuse.
pub fn verify_signature(bytes: &[u8], signature: &str, public_key: &str) -> Result<()> {
    let pk = PublicKey::from_base64(public_key).map_err(|e| {
        Error::Invalid(
            Msg::new(format!("invalid catalog public key: {e}"))
                .coded(ErrorCode::InvalidPluginKeyMalformed)
                .with("reason", e),
        )
    })?;
    let sig = Signature::decode(signature).map_err(|e| {
        Error::Invalid(
            Msg::new(format!("malformed plugin signature: {e}"))
                .coded(ErrorCode::InvalidPluginSignatureMalformed)
                .with("reason", e),
        )
    })?;
    let key = key_named(public_key);
    pk.verify(bytes, &sig, false).map_err(|e| {
        Error::Invalid(
            Msg::new(format!("plugin signature does not verify against {key}: {e}"))
                .coded(ErrorCode::InvalidPluginSignatureMismatch)
                .with("key", &key)
                .with("reason", e),
        )
    })
}

/// How a refusal names the key it checked against.
///
/// "The catalog key" was one key when there was one; there is now the key Amenbo ships and the key each
/// registered catalog was pinned with (`AMB-D-389`), and which of them was tried is what tells a reader
/// what they are looking at: an official asset that fails is a broken publish, while a registered
/// catalog's is a publisher signing with something other than what its own catalog offered. The
/// fingerprint is the handle both sides can quote — the same short form a registration showed.
fn key_named(public_key: &str) -> String {
    if public_key == CATALOG_PUBLIC_KEY {
        return "the Amenbo catalog key".to_string();
    }
    match key_fingerprint(public_key) {
        Ok(fp) => format!("the key pinned for the catalog this plugin came from ({fp})"),
        Err(_) => "the key its catalog was pinned with".to_string(),
    }
}

/// Verify both halves of provenance on a freshly downloaded asset (`AMB-D-371`) — the door an install
/// (`AMB-T-1979`) or update (`AMB-D-359`) calls before the asset is trusted. Fail-closed:
///
/// - `signature` of `None` is refused outright — an unsigned asset has no origin Amenbo can vouch for
///   (`AMB-D-351`).
/// - the signature must verify against `public_key` (origin: the key its catalog is trusted on).
/// - the checksum must match the bytes (integrity: what the manifest recorded).
///
/// Order is deliberate: signature (origin) before checksum (integrity), so an asset from an untrusted
/// source is rejected on origin, not merely on a digest it could itself have computed.
pub fn verify_asset(
    bytes: &[u8],
    signature: Option<&str>,
    checksum: &str,
    public_key: &str,
) -> Result<()> {
    let signature = signature.ok_or_else(|| {
        Error::Invalid(
            Msg::new(
                "plugin asset is unsigned: it carries no signature from the catalog listing it, so its origin cannot be verified",
            )
            .coded(ErrorCode::InvalidPluginUnsigned),
        )
    })?;
    verify_signature(bytes, signature, public_key)?;
    verify_checksum(bytes, checksum)?;
    Ok(())
}

/// The key one asset is verified against: Amenbo's own for the official catalog, or the key a registered
/// catalog was pinned with (`AMB-D-389`).
///
/// It exists to keep "which key" out of a caller's hands now that there is more than one. There is no way
/// to build one from a string outside this crate, and inside it only the catalog layer does — from the
/// registration that was consented to, never from anything an install was told. So the widening
/// (`AMB-D-371` had one root; there are now several) does not become "any key will do": a trust root
/// still comes only from where trust was given.
#[derive(Clone, Debug)]
pub struct TrustRoot(String);

impl TrustRoot {
    /// The root Amenbo ships — the official catalog's, and the only one that needs no registration.
    pub fn official() -> TrustRoot {
        TrustRoot(CATALOG_PUBLIC_KEY.to_string())
    }

    /// The root a registration pinned. Crate-private on purpose: the only caller is the catalog layer,
    /// handing on a key a person agreed to (see the type's note).
    pub(crate) fn pinned(public_key: String) -> TrustRoot {
        TrustRoot(public_key)
    }

    /// This root's fingerprint, for saying *which* key an asset was checked against.
    pub fn fingerprint(&self) -> Option<String> {
        key_fingerprint(&self.0).ok()
    }
}

/// Verify both halves of provenance against the root this asset's catalog answers for (`AMB-D-389`) —
/// the door an install or an update calls once catalogs are more than one.
///
/// Same fail-closed rules as [`verify_asset`], and the same order; what a caller cannot do is choose the
/// key, because a [`TrustRoot`] is not something it can make up.
pub fn verify_against(
    bytes: &[u8],
    signature: Option<&str>,
    checksum: &str,
    root: &TrustRoot,
) -> Result<()> {
    verify_asset(bytes, signature, checksum, &root.0)
}

/// The base64 public key out of a minisign `.pub` file — what a catalog publishes beside its
/// `catalog.json` for a registration to pin (`AMB-D-389`).
///
/// minisign writes two lines, a comment then the key, but the file travels by copy and paste and comes
/// back with a wrapped comment, a missing one, or a trailing blank line. So every line is tried and the
/// first one that is a usable key is the key. Fail-closed: a document with no key line in it is an
/// error, never "this catalog publishes no key" — that answer belongs to a catalog that serves nothing
/// at the address at all, and reading a broken file as an absence would quietly drop a pin the
/// publisher meant to offer.
pub fn read_public_key(text: &str) -> Result<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && PublicKey::from_base64(line).is_ok())
        .map(str::to_string)
        .ok_or_else(|| {
            Error::invalid("no minisign public key in the document (expected the key line of a .pub file)")
        })
}

/// The fingerprint Amenbo shows for a public key, and the handle the publisher can quote back: the
/// minisign key id, 16 uppercase hex.
///
/// It is the id minisign itself writes into the `.pub` file's comment line
/// (`untrusted comment: minisign public key 6272CBB782CB57A0`), which is why it is the fingerprint here
/// — a publisher can read it off their own key file and put it in their README, and a user comparing
/// the two is comparing the same string, not two encodings of one.
///
/// What the pin holds is the **whole key**, not this; the fingerprint is the short form a human is
/// shown while consenting (`AMB-D-389`), and the comparison Amenbo makes later is over the key itself.
pub fn key_fingerprint(public_key: &str) -> Result<String> {
    use base64::Engine as _;
    // Parse first: the fingerprint is read out of raw bytes, so the key has to be a key before its
    // bytes mean anything.
    PublicKey::from_base64(public_key).map_err(|e| {
        Error::invalid(format!("invalid catalog public key: {e}"))
    })?;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(public_key.trim())
        .map_err(|e| Error::invalid(format!("public key is not base64: {e}")))?;
    // `Ed` + an 8-byte little-endian key id + the 32-byte key; minisign prints the id big-endian.
    Ok(raw[2..10].iter().rev().map(|b| format!("{b:02X}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // A fixed minisign test vector, generated offline with the `minisign` CLI (an empty-password keypair):
    //   printf 'amenbo-plugin-worktree-v1-payload' > asset.bin
    //   minisign -S -s sec.key -m asset.bin
    // The key here is a throwaway test key, NOT the production catalog key (that is generated with the
    // catalog CI, `AMB-T-1978`). This proves the verification logic against a real minisign signature
    // without pulling a signing crate into the build.
    const TEST_PUBKEY: &str = "RWSw3wZ34b1PMyHu4KajlLhV0SdlMAgQGefo4pFIxv7MgRoWSVpCVXSE";
    const ASSET: &[u8] = b"amenbo-plugin-worktree-v1-payload";
    const ASSET_SHA256: &str = "sha256:9584d9efc185f9f04bdf2256aafd4cfd46912c21a8b8396687f320d63d2a3f6e";
    const ASSET_SIG: &str = "untrusted comment: signature from minisign secret key\n\
RUSw3wZ34b1PM9x5mHClDjv2yuWNccMVMkz+HzDYzn589GSGZrbwCyud3qvFDHKP1IM7jyeG1GPOBHrELMvyftBbQaoLKXs7rQ8=\n\
trusted comment: timestamp:1784752236\tfile:asset.bin\thashed\n\
wSmRtS1I2Ego34wQdpELeHd1RezvOk7TmUTIDBwudsIU9GYIv0hKJtROtXPoyKDCXETlV0Wkj25hwMF2mYKOAQ==\n";
    // A signature by the SAME key over DIFFERENT bytes (b"tampered-payload") — a valid minisign signature
    // that must not verify against ASSET.
    const OTHER_SIG: &str = "untrusted comment: signature from minisign secret key\n\
RUSw3wZ34b1PMzCou1mB2wBI+YRDgOTbT/XzhaTvkR9LLmpDg9E2EF7kgLdRpF12dPABd8tQUxZ8dhUG3kfy5sHP3Q/sOTaKYQQ=\n\
trusted comment: timestamp:1784752238\tfile:other.bin\thashed\n\
yO4MZq6nO8TD4ypgwfYImIKz9E1tM3szwA/S9CRXLrH30HP+gQHXcL12wngoJy9uCBgHuaIsrnRo17T3+mxcCg==\n";

    // ---- checksum ----

    #[test]
    fn checksum_matches_the_bytes() {
        verify_checksum(ASSET, ASSET_SHA256).unwrap();
    }

    #[test]
    fn checksum_is_case_insensitive_hex() {
        verify_checksum(ASSET, &ASSET_SHA256.to_uppercase().replace("SHA256:", "sha256:")).unwrap();
    }

    #[test]
    fn a_swapped_byte_fails_the_checksum() {
        let mut tampered = ASSET.to_vec();
        tampered[0] ^= 0x01;
        let err = verify_checksum(&tampered, ASSET_SHA256).unwrap_err();
        assert!(format!("{err:?}").contains("checksum"), "the mismatch is named");
    }

    #[test]
    fn a_non_sha256_prefix_is_refused() {
        let err = verify_checksum(ASSET, "md5:abc").unwrap_err();
        assert!(format!("{err:?}").contains("checksum format"), "the bad algorithm is named");
    }

    #[test]
    fn a_bare_digest_with_no_prefix_is_refused() {
        // No algorithm prefix at all — fail-closed, not a lenient "assume sha256".
        assert!(verify_checksum(ASSET, &ASSET_SHA256[7..]).is_err());
    }

    #[test]
    fn a_wrong_length_digest_is_refused() {
        assert!(verify_checksum(ASSET, "sha256:dead").is_err());
    }

    #[test]
    fn a_non_hex_digest_is_refused() {
        let bad = format!("sha256:{}", "z".repeat(64));
        assert!(verify_checksum(ASSET, &bad).is_err());
    }

    // ---- signature ----

    #[test]
    fn a_genuine_signature_verifies() {
        verify_signature(ASSET, ASSET_SIG, TEST_PUBKEY).unwrap();
    }

    #[test]
    fn a_signature_over_other_bytes_does_not_verify() {
        // OTHER_SIG is a real signature by the same key, but over different bytes.
        let err = verify_signature(ASSET, OTHER_SIG, TEST_PUBKEY).unwrap_err();
        assert!(format!("{err:?}").contains("does not verify"), "the failed verify is named");
    }

    #[test]
    fn a_tampered_asset_does_not_verify() {
        let mut tampered = ASSET.to_vec();
        tampered[0] ^= 0x01;
        assert!(verify_signature(&tampered, ASSET_SIG, TEST_PUBKEY).is_err());
    }

    #[test]
    fn a_wrong_public_key_does_not_verify() {
        // A different, well-formed minisign public key (another throwaway key). The signature is genuine
        // but was not made by this key's private half.
        let other_pubkey = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
        assert!(verify_signature(ASSET, ASSET_SIG, other_pubkey).is_err());
    }

    #[test]
    fn a_malformed_public_key_is_refused() {
        assert!(verify_signature(ASSET, ASSET_SIG, "not-a-key").is_err());
    }

    #[test]
    fn a_malformed_signature_is_refused() {
        assert!(verify_signature(ASSET, "not a minisig", TEST_PUBKEY).is_err());
    }

    // ---- verify_asset (both halves) ----

    #[test]
    fn a_signed_asset_with_a_matching_checksum_passes() {
        verify_asset(ASSET, Some(ASSET_SIG), ASSET_SHA256, TEST_PUBKEY).unwrap();
    }

    #[test]
    fn an_unsigned_asset_is_refused_before_any_checksum() {
        // None signature — the origin cannot be vouched for, regardless of the checksum.
        let err = verify_asset(ASSET, None, ASSET_SHA256, TEST_PUBKEY).unwrap_err();
        assert!(format!("{err:?}").contains("unsigned"), "the missing signature is the reason");
    }

    #[test]
    fn a_good_signature_but_wrong_checksum_is_refused() {
        let wrong = format!("sha256:{}", "0".repeat(64));
        assert!(verify_asset(ASSET, Some(ASSET_SIG), &wrong, TEST_PUBKEY).is_err());
    }

    #[test]
    fn a_good_checksum_but_bad_signature_is_refused() {
        // Origin is checked before integrity: a wrong-origin asset is rejected even with a correct digest.
        assert!(verify_asset(ASSET, Some(OTHER_SIG), ASSET_SHA256, TEST_PUBKEY).is_err());
    }

    /// A key that is not the one Amenbo ships is named by its fingerprint (`AMB-D-389`). Reading a
    /// registered catalog's refusal as "the Amenbo catalog key" sends the reader after the wrong thing:
    /// what failed is the pin their own consent put there.
    #[test]
    fn a_refusal_names_the_pinned_key_it_checked_against() {
        let err = verify_asset(ASSET, Some(OTHER_SIG), ASSET_SHA256, TEST_PUBKEY).unwrap_err();
        let text = format!("{err:?}");
        assert!(
            text.contains(&key_fingerprint(TEST_PUBKEY).unwrap()),
            "the fingerprint of the key it checked against: {text}"
        );
        assert!(!text.contains("amenbo catalog key"), "and not the one Amenbo ships: {text}");
    }

    // ---- the embedded catalog key ----

    #[test]
    fn the_embedded_catalog_key_is_a_usable_minisign_key() {
        // A typo in the constant would otherwise surface only at the first real install, on a user's
        // machine, as "invalid catalog public key".
        PublicKey::from_base64(CATALOG_PUBLIC_KEY).expect("the embedded key parses");
    }

    #[test]
    fn the_embedded_catalog_key_is_the_catalog_key_and_not_the_updater_key() {
        // A minisign public key is `Ed` + an 8-byte little-endian key id + the 32-byte key. Reading the id
        // back pins *which* key is embedded, so swapping in the updater key (2F151276522ADC1D) — the one
        // other minisign key in this repository — fails here rather than silently moving the trust root.
        use base64::Engine as _;
        let raw = base64::engine::general_purpose::STANDARD
            .decode(CATALOG_PUBLIC_KEY)
            .expect("the key is base64");
        assert_eq!(&raw[0..2], b"Ed", "a minisign Ed25519 key");
        let key_id: String = raw[2..10].iter().rev().map(|b| format!("{b:02X}")).collect();
        assert_eq!(key_id, "6272CBB782CB57A0", "the catalog key from `catalog-key.pub`");
    }

    #[test]
    fn the_catalog_door_refuses_an_asset_signed_by_any_other_key() {
        // The test key is a real minisign key with a real signature over these exact bytes — everything
        // but the one root Amenbo trusts. This is the whole point of embedding a key.
        let err = verify_asset(ASSET, Some(ASSET_SIG), ASSET_SHA256, CATALOG_PUBLIC_KEY).unwrap_err();
        assert!(format!("{err:?}").contains("does not verify"), "refused on origin");
        assert!(
            format!("{err:?}").contains("the Amenbo catalog key"),
            "and it says which key it was checked against: {err:?}"
        );
    }

    #[test]
    fn the_catalog_door_refuses_an_unsigned_asset() {
        let err = verify_asset(ASSET, None, ASSET_SHA256, CATALOG_PUBLIC_KEY).unwrap_err();
        assert!(format!("{err:?}").contains("unsigned"), "the missing signature is the reason");
    }

    // ---- a published key, and the fingerprint shown for it (`AMB-D-389`) ----

    /// What minisign writes: a comment line, then the key. Both are read off a real file, so the
    /// fingerprint here is checked against the id minisign itself put in the comment.
    const PUB_FILE: &str =
        "untrusted comment: minisign public key 6272CBB782CB57A0\nRWSgV8uCt8tyYg74JbwBblWoE+g7bxSGvK8blkKW7gUo3EuBXaqy5oMR\n";

    #[test]
    fn the_key_line_is_read_out_of_a_published_pub_file() {
        assert_eq!(read_public_key(PUB_FILE).unwrap(), CATALOG_PUBLIC_KEY);
    }

    /// The file travels by copy and paste: a missing comment, a blank line, trailing spaces. None of
    /// those is a different key, so none of them may read as one.
    #[test]
    fn a_key_survives_the_wrapping_it_arrives_in() {
        assert_eq!(read_public_key(CATALOG_PUBLIC_KEY).unwrap(), CATALOG_PUBLIC_KEY);
        let messy = format!("\n  untrusted comment: whatever  \n  {CATALOG_PUBLIC_KEY}  \n\n");
        assert_eq!(read_public_key(&messy).unwrap(), CATALOG_PUBLIC_KEY);
    }

    /// A document with no key in it is an error, not an absence — see [`read_public_key`].
    #[test]
    fn a_document_that_holds_no_key_is_refused() {
        assert!(read_public_key("").is_err());
        assert!(read_public_key("untrusted comment: minisign public key\nnot-a-key\n").is_err());
        assert!(read_public_key("<!doctype html><title>404</title>").is_err(), "an error page is not a key");
    }

    /// The fingerprint is the id minisign prints, so a publisher quoting their own `.pub` comment and a
    /// user reading Amenbo's prompt are comparing one string.
    #[test]
    fn the_fingerprint_is_the_key_id_minisign_shows() {
        assert_eq!(key_fingerprint(CATALOG_PUBLIC_KEY).unwrap(), "6272CBB782CB57A0");
        assert_ne!(
            key_fingerprint(TEST_PUBKEY).unwrap(),
            key_fingerprint(CATALOG_PUBLIC_KEY).unwrap(),
            "two keys, two fingerprints"
        );
        assert!(key_fingerprint("not-a-key").is_err());
    }
}
