//! **What the Viewer carries is sealed here and opened on the phone**, and nothing in between holds
//! the key — which is what makes a place the user merely rents safe to leave the backlog in
//! (`AMB-D-585`, ported from the `viewer` plugin `AMB-D-884` retired).
//!
//! The store on this machine stays in plaintext. Sealing is the carrier's job, not the store's.
//!
//! The cipher is XChaCha20-Poly1305 with a 256-bit key, and **one record is one ciphertext**. Per
//! record rather than one blob for the lot: the store updates a row at a time and the phone decrypts
//! only the rows that moved, neither of which a single ciphertext allows.
//!
//! **XChaCha20 rather than AES-GCM, for the nonce.** Sealing happens once per record, so the count
//! climbs without bound, and AES-GCM's 96-bit nonce is narrow enough that a random one eventually
//! repeats — which breaks the cipher outright. XChaCha20's is 192 bits: draw it at random every time
//! and a repeat is not something to keep a counter against.
//!
//! The envelope is what this settles, because the phone has to open what it seals:
//!
//! - **The nonce and the ciphertext travel side by side**, as two values, never concatenated into
//!   one. The record that carries them already names them apart, so a blob would only be something to
//!   split again at the far end.
//! - **Both are base64url, unpadded.** The records are JSON and the pairing code writes the key the
//!   same way, so one alphabet covers everything that leaves here.
//! - **The ciphertext ends with its 16-byte Poly1305 tag**, exactly as the cipher emits it. Nothing
//!   is stripped, added or reordered, so any implementation of XChaCha20-Poly1305 opens it without
//!   being told about this module.
//! - **The key the record is filed under goes into the tag**, as additional data — its own bytes, as
//!   it is written. It stays in the clear beside the ciphertext, but a record moved to another key no
//!   longer opens. Leave it out and whoever can write to the server can serve one task's contents
//!   under another task's key: still unable to read either, and still able to make the phone show the
//!   wrong thing.

use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use sha2::{Digest as _, Sha256};

use crate::error::{Error, Result};
use crate::model::SecretArea;
use crate::store::Store;

/// The key the cipher takes and setup draws: 256 bits.
pub const KEY_SIZE: usize = 32;

/// The nonce XChaCha20 takes: 192 bits.
const NONCE_SIZE: usize = 24;

/// One record's envelope, as it travels: two values, written the way everything that leaves here is
/// written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sealed {
    /// The nonce this record was sealed under, base64url without padding.
    pub nonce: String,
    /// The ciphertext with its Poly1305 tag on the end, base64url without padding.
    pub ciphertext: String,
}

/// The configured key, in the form the cipher wants, so a run that seals hundreds of records
/// schedules it once.
pub struct Sealer {
    cipher: XChaCha20Poly1305,
    fingerprint: String,
}

/// What a sealer says of itself is the fingerprint and nothing else. A key reaches a log the moment
/// something holding it is written out with the derived spelling, and this is the one thing here that
/// would be — so the key is named rather than shown, which is what the fingerprint is for.
impl std::fmt::Debug for Sealer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sealer").field("fingerprint", &self.fingerprint).finish_non_exhaustive()
    }
}

impl Sealer {
    /// Take the key as it is configured — 32 bytes written as base64url — and make something that can
    /// seal records.
    ///
    /// **Padding is optional on the way in.** The key is copied between a pairing code, the store and
    /// whatever a person pastes it into, and refusing one of the two spellings would fail a key that
    /// is correct.
    ///
    /// **No failure here quotes the key, or any part of it.** These sentences reach a log, and a log
    /// that echoes the key hands over the one secret this design has.
    pub fn new(encoded_key: &str) -> Result<Sealer> {
        let written = encoded_key.trim().trim_end_matches('=');
        let key = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(written)
            .map_err(|_| Error::invalid("the Viewer's encryption key is not base64url"))?;
        if key.len() != KEY_SIZE {
            return Err(Error::invalid(format!(
                "the Viewer's encryption key is {} bytes and the cipher takes {KEY_SIZE}",
                key.len()
            )));
        }
        let cipher = XChaCha20Poly1305::new_from_slice(&key)
            .map_err(|_| Error::invalid("the Viewer's encryption key was refused by the cipher"))?;
        Ok(Sealer { cipher, fingerprint: fingerprint_of(&key) })
    }

    /// The sealer this device's key makes, or `None` where setup has not run.
    ///
    /// Absence is not a failure here: a device that has never stood a server up has no key, and that
    /// is a state to explain rather than an error to raise. A key that is present and wrong is the
    /// other thing, and that comes back as one.
    pub fn for_device(store: &Store) -> Result<Option<Sealer>> {
        let Some(key) = store.secret_value(
            None,
            SecretArea::Viewer,
            None,
            super::ENCRYPTION_KEY,
        )?
        else {
            return Ok(None);
        };
        if key.trim().is_empty() {
            return Ok(None);
        }
        Sealer::new(&key).map(Some)
    }

    /// What names this key without being it — see [`fingerprint_of`].
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Seal one record, and hand back the two halves of its envelope ready to be written as they are.
    ///
    /// `filed_under` is the key the record travels under. It is bound into the tag rather than
    /// encrypted — it has to stay readable for the server to update a row, and binding it is what
    /// stops the row being moved.
    ///
    /// The nonce is drawn fresh on every call and never derived from the record: two records sealed
    /// under one nonce and one key would give away both plaintexts to anyone holding the pair.
    pub fn seal(&self, filed_under: &str, record: &[u8]) -> Sealed {
        let mut raw = [0u8; NONCE_SIZE];
        getrandom::fill(&mut raw).expect("failed to draw OS randomness");
        let sealed = self
            .cipher
            .encrypt(&XNonce::from(raw), Payload { msg: record, aad: filed_under.as_bytes() })
            // The cipher refuses only what it cannot fit, and what is handed here is one record of a
            // backlog. There is nothing a caller could do about it either: the alternative to sealing
            // is not sending the record in the clear.
            .expect("XChaCha20-Poly1305 sealed a record");
        let written = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        Sealed { nonce: written.encode(raw), ciphertext: written.encode(sealed) }
    }
}

/// Name a key: the SHA-256 of it, as 64 lower-case hex characters.
///
/// **It is taken over the key's bytes and never over how the key is written.** Padding is optional
/// wherever this key is copied, so two correct spellings of one key exist, and hashing the text would
/// make them two different keys to whoever is comparing. The bytes have one form.
///
/// **What it is for is the phone finding out that its key does not fit before it fetches.** The server
/// names the key its records were sealed with, and a phone whose own key names something else cannot
/// open a row of it — which is a different thing from a record being damaged, and calls for the
/// opposite move. A hash is what may be said out loud there: that answer is served to anyone holding
/// a read code.
pub fn fingerprint_of(key: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key);
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key written the way setup writes one.
    fn a_key() -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([7u8; KEY_SIZE])
    }

    /// Open an envelope the way the phone does, with nothing but the cipher and what travelled. It is
    /// here rather than in the library because nothing on this side ever opens one — and it is what
    /// says the envelope needs no knowledge of this module to read.
    fn opened(key: &str, filed_under: &str, sealed: &Sealed) -> Vec<u8> {
        let written = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let cipher = XChaCha20Poly1305::new_from_slice(&written.decode(key).unwrap()).unwrap();
        let raw = written.decode(&sealed.nonce).unwrap();
        cipher
            .decrypt(
                &XNonce::try_from(&raw[..]).expect("the nonce is the size it was written at"),
                Payload {
                    msg: &written.decode(&sealed.ciphertext).unwrap(),
                    aad: filed_under.as_bytes(),
                },
            )
            .expect("the phone opens what was sealed for it")
    }

    /// What is sealed comes back, and the envelope is the two values it says it is.
    #[test]
    fn what_the_phone_opens_is_what_was_sealed() {
        let sealer = Sealer::new(&a_key()).expect("a key of the right size");
        let sealed = sealer.seal("task/2812", b"{\"title\":\"\xe6\xa1\x88\"}");

        assert_eq!(opened(&a_key(), "task/2812", &sealed), "{\"title\":\"案\"}".as_bytes());
        assert_eq!(
            base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&sealed.nonce).unwrap().len(),
            NONCE_SIZE
        );
        // The plaintext plus the 16-byte tag, and nothing else: an implementation that was told
        // nothing about this module reads exactly that.
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&sealed.ciphertext)
            .unwrap()
            .len();
        assert_eq!(bytes, "{\"title\":\"案\"}".len() + 16);
    }

    /// A record moved to another key no longer opens. The key stays in the clear, so what stops the
    /// move is the tag it is bound into rather than anything secret about it.
    #[test]
    fn a_record_served_under_another_key_does_not_open() {
        let sealer = Sealer::new(&a_key()).unwrap();
        let sealed = sealer.seal("task/2812", b"the contents of one task");

        let written = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let cipher = XChaCha20Poly1305::new_from_slice(&written.decode(a_key()).unwrap()).unwrap();
        let opened = cipher.decrypt(
            &XNonce::try_from(&written.decode(&sealed.nonce).unwrap()[..]).unwrap(),
            Payload {
                msg: &written.decode(&sealed.ciphertext).unwrap(),
                aad: b"task/2799",
            },
        );
        assert!(opened.is_err(), "a row moved to another key still opens");
    }

    /// Two seals of one record do not share a nonce, and do not look alike.
    #[test]
    fn every_seal_draws_its_own_nonce() {
        let sealer = Sealer::new(&a_key()).unwrap();
        let (one, two) = (sealer.seal("task/1", b"same"), sealer.seal("task/1", b"same"));
        assert_ne!(one.nonce, two.nonce);
        assert_ne!(one.ciphertext, two.ciphertext);
    }

    /// Both spellings of one key are one key. Padding is optional wherever this key is copied, and a
    /// correct key written the other way must not be refused — nor name a different key.
    #[test]
    fn a_key_written_with_padding_is_the_same_key() {
        let padded = base64::engine::general_purpose::URL_SAFE.encode([7u8; KEY_SIZE]);
        assert!(padded.ends_with('='), "this spelling is the one with padding");

        let (bare, with) = (Sealer::new(&a_key()).unwrap(), Sealer::new(&padded).unwrap());
        assert_eq!(bare.fingerprint(), with.fingerprint());

        let sealed = with.seal("task/1", b"written one way");
        assert_eq!(opened(&a_key(), "task/1", &sealed), b"written one way");
    }

    /// A key of the wrong size, or not base64url at all, is refused — and the refusal does not carry
    /// the key.
    #[test]
    fn a_key_that_is_not_a_key_is_refused_without_being_quoted() {
        let short = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([1u8; 16]);
        let said = Sealer::new(&short).expect_err("16 bytes is not a 256-bit key").message_en();
        assert!(said.contains("16 bytes"), "{said}");
        assert!(!said.contains(&short), "the refusal carries the key: {said}");

        let nonsense = "not base64url!!";
        let said = Sealer::new(nonsense).expect_err("that is not base64url").message_en();
        assert!(!said.contains(nonsense), "the refusal carries what was pasted: {said}");
    }

    /// The fingerprint names the bytes, as 64 lower-case hex characters — the same shape the server
    /// compares, and the one the phone holds its own key against.
    #[test]
    fn the_fingerprint_names_the_bytes_and_not_the_spelling() {
        let named = fingerprint_of(&[7u8; KEY_SIZE]);
        assert_eq!(named.len(), 64);
        assert!(named.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)), "{named}");
        assert_eq!(Sealer::new(&a_key()).unwrap().fingerprint(), named);
        assert_ne!(named, fingerprint_of(&[8u8; KEY_SIZE]));
    }

    /// A device that has never stood a server up has no key, and that is a state rather than a
    /// failure. A key that is there and wrong is the other thing.
    #[test]
    fn a_device_with_no_key_has_no_sealer() {
        let dir = amenbo_scratch::scratch("viewer-sealing-device");
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = Store::open_at(crate::config::Paths::at(dir)).unwrap();

        assert!(Sealer::for_device(&store).unwrap().is_none());

        store
            .set_secret(None, SecretArea::Viewer, None, super::super::ENCRYPTION_KEY, Some(&a_key()))
            .unwrap();
        let sealer = Sealer::for_device(&store).unwrap().expect("the key setup left");
        assert_eq!(sealer.fingerprint(), fingerprint_of(&[7u8; KEY_SIZE]));

        store
            .set_secret(None, SecretArea::Viewer, None, super::super::ENCRYPTION_KEY, Some("nope"))
            .unwrap();
        Sealer::for_device(&store).expect_err("a key that is there and wrong is an error");
    }
}
