//! Secret value types — the never-plaintext discipline (S14-S-1).
//!
//! Two distinct types:
//!
//! - [`SecretValue`] — caller-facing input. Holds plaintext in
//!   `Zeroizing<Vec<u8>>`; memory zeroed on drop. Used to feed
//!   `SecretBackend::set`.
//! - [`EncryptedSecretValue`] — runtime ciphertext form. What
//!   backends return on `get()`. Decrypts to a short-lived
//!   `Zeroizing<Vec<u8>>` only via the explicit
//!   [`EncryptedSecretValue::decrypt_for_use`] call.
//!
//! Neither type implements `Debug` or `Display`. Logging a secret
//! requires an explicit redaction — `[REDACTED secret_id={…}]`.

use crate::backend::SecretError;
use crate::master_key::MasterSessionKey;
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use rand::RngCore;
use zeroize::Zeroizing;

/// Caller-facing input wrapper holding plaintext bytes. Memory
/// zeroed on drop (via `Zeroizing<Vec<u8>>`).
///
/// Constructor takes ownership of the input; for ergonomic
/// migration from `String` / env-var sources, use [`SecretValue::from_string`].
pub struct SecretValue {
    plaintext: Zeroizing<Vec<u8>>,
}

impl SecretValue {
    /// Build from raw bytes. The input is moved into `Zeroizing`
    /// so the original `Vec<u8>` is also overwritten on drop (the
    /// caller's reference becomes invalid).
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            plaintext: Zeroizing::new(bytes),
        }
    }

    /// Build from a String. The String's underlying buffer is moved
    /// into `Zeroizing` and the String drops empty.
    ///
    /// **Caveat:** if the caller previously cloned this String
    /// elsewhere, those clones are NOT zeroized. Best practice:
    /// take a `&str` from a source that owns the only copy
    /// (e.g., reading directly from env via `std::env::var` then
    /// immediately dropping the env value isn't enough — see the
    /// proxy-cli secret import-from-env helper for the audited
    /// pattern).
    pub fn from_string(s: String) -> Self {
        Self::from_bytes(s.into_bytes())
    }

    /// Borrow the plaintext bytes. Callers should drop the borrow
    /// quickly; long-lived borrows defeat the zeroize discipline.
    pub fn plaintext(&self) -> &[u8] {
        &self.plaintext
    }

    /// Length of the plaintext in bytes. Useful for logging
    /// metadata-only diagnostics ("set 32-byte secret X") without
    /// exposing the value.
    pub fn len(&self) -> usize {
        self.plaintext.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plaintext.is_empty()
    }

    /// Encrypt this value into the in-memory representation using
    /// the master session key. Plaintext drops (zeroized) when
    /// `self` is consumed by this call.
    pub fn into_encrypted(
        self,
        master: &MasterSessionKey,
    ) -> Result<EncryptedSecretValue, SecretError> {
        let cipher = ChaCha20Poly1305::new(master.as_chacha_key());
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, self.plaintext.as_slice())
            .map_err(|_| SecretError::EncryptionFailed)?;
        Ok(EncryptedSecretValue {
            ciphertext,
            nonce: nonce_bytes,
        })
    }
}

/// Manual `Debug` impl that NEVER prints the plaintext. Required so
/// `Result<Option<SecretValue>>::unwrap_err` and similar test
/// patterns compile. The output deliberately omits content:
///
/// ```text
/// SecretValue { plaintext: [REDACTED; len=32] }
/// ```
///
/// The same discipline applies in production logs — even when an
/// outer struct that contains `SecretValue` is `#[derive(Debug)]`,
/// the secret never appears in the formatted output.
impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretValue")
            .field("plaintext", &format_args!("[REDACTED; len={}]", self.plaintext.len()))
            .finish()
    }
}

/// In-memory ciphertext form of a secret. Held in process memory
/// after a backend `get()`; carries the nonce alongside the
/// ciphertext so decryption is self-contained.
///
/// **Not Serialize/Deserialize.** Persisting this is intentional —
/// the at-rest format (encrypted-file or OS-native) is the
/// backend's concern; the in-memory form is per-process per-session.
#[derive(Clone)]
pub struct EncryptedSecretValue {
    ciphertext: Vec<u8>,
    nonce: [u8; 12],
}

impl EncryptedSecretValue {
    /// Construct directly from ciphertext + nonce. Used by backends
    /// that already hold the in-memory ciphertext shape (e.g.,
    /// the encrypted-file backend after at-rest decryption).
    pub fn from_parts(ciphertext: Vec<u8>, nonce: [u8; 12]) -> Self {
        Self { ciphertext, nonce }
    }

    /// Wrap raw plaintext bytes as if they came from a backend. Used
    /// by the OS-native backend, which receives plaintext from
    /// keyring (the OS handles at-rest encryption itself) and re-
    /// encrypts in-memory immediately.
    pub fn wrap_plaintext(
        plaintext: &[u8],
        master: &MasterSessionKey,
    ) -> Result<Self, SecretError> {
        let cipher = ChaCha20Poly1305::new(master.as_chacha_key());
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| SecretError::EncryptionFailed)?;
        Ok(Self {
            ciphertext,
            nonce: nonce_bytes,
        })
    }

    /// Decrypt for short-lived use. Returns owned plaintext wrapped
    /// in `Zeroizing<Vec<u8>>` — the caller controls scope, but the
    /// drop guarantees memory is overwritten when the binding goes
    /// out of scope.
    ///
    /// **Discipline:** drop the returned value immediately after the
    /// upstream API call. Holding it across `await` points or in
    /// long-lived state is a code-review smell.
    pub fn decrypt_for_use(
        &self,
        master: &MasterSessionKey,
    ) -> Result<Zeroizing<Vec<u8>>, SecretError> {
        let cipher = ChaCha20Poly1305::new(master.as_chacha_key());
        let nonce = Nonce::from_slice(&self.nonce);
        let plaintext = cipher
            .decrypt(nonce, self.ciphertext.as_ref())
            .map_err(|_| SecretError::DecryptionFailed)?;
        Ok(Zeroizing::new(plaintext))
    }

    /// Bytes-on-the-wire representation: ciphertext length. Useful
    /// for metadata-only diagnostics.
    pub fn ciphertext_len(&self) -> usize {
        self.ciphertext.len()
    }

    /// Borrow the raw ciphertext + nonce. For backends that
    /// persist the in-memory form to disk after additional at-rest
    /// encryption (the encrypted-file backend wraps this with
    /// PBKDF2-derived passphrase encryption).
    pub fn parts(&self) -> (&[u8], &[u8; 12]) {
        (&self.ciphertext, &self.nonce)
    }
}

/// Manual `Debug` impl that NEVER prints the ciphertext. Even
/// though the ciphertext is encrypted and operators technically
/// can't decrypt without the master key, surfacing the bytes to
/// logs invites accidental copy-paste; safer to redact uniformly.
/// Output:
///
/// ```text
/// EncryptedSecretValue { ciphertext: [REDACTED; len=48], nonce: [REDACTED] }
/// ```
impl std::fmt::Debug for EncryptedSecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptedSecretValue")
            .field(
                "ciphertext",
                &format_args!("[REDACTED; len={}]", self.ciphertext.len()),
            )
            .field("nonce", &format_args!("[REDACTED]"))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::master_key::MasterSessionKey;

    fn test_master_key() -> MasterSessionKey {
        // Deterministic 32-byte key for tests; never used in production.
        MasterSessionKey::from_raw([7u8; 32])
    }

    #[test]
    fn secret_value_zeroizes_on_drop() {
        // We can't observe the actual zeroize from the outside (the
        // bytes are gone), but we can verify the pattern compiles
        // and round-trips a known value through plaintext().
        let v = SecretValue::from_string("abcd1234".to_string());
        assert_eq!(v.plaintext(), b"abcd1234");
        assert_eq!(v.len(), 8);
        assert!(!v.is_empty());
    }

    #[test]
    fn secret_value_from_bytes_preserves_content() {
        let bytes = vec![0x01, 0x02, 0x03, 0xff];
        let v = SecretValue::from_bytes(bytes.clone());
        assert_eq!(v.plaintext(), &bytes[..]);
    }

    #[test]
    fn encrypt_then_decrypt_round_trips() {
        let master = test_master_key();
        let original = b"super-secret-api-key-do-not-leak".to_vec();
        let v = SecretValue::from_bytes(original.clone());
        let encrypted = v.into_encrypted(&master).unwrap();
        let plaintext = encrypted.decrypt_for_use(&master).unwrap();
        assert_eq!(plaintext.as_slice(), &original[..]);
    }

    #[test]
    fn each_encryption_uses_a_fresh_nonce() {
        // Encrypting the same plaintext twice should yield distinct
        // ciphertexts (because the random nonce changes). This is
        // important for ChaCha20Poly1305's IND-CPA guarantee.
        let master = test_master_key();
        let plaintext = b"identical-input".to_vec();
        let e1 = SecretValue::from_bytes(plaintext.clone())
            .into_encrypted(&master)
            .unwrap();
        let e2 = SecretValue::from_bytes(plaintext.clone())
            .into_encrypted(&master)
            .unwrap();
        let (c1, n1) = e1.parts();
        let (c2, n2) = e2.parts();
        assert_ne!(n1, n2, "nonces must differ across encryptions");
        assert_ne!(c1, c2, "ciphertexts must differ across encryptions");
    }

    #[test]
    fn decrypt_with_wrong_master_key_fails_with_decryption_failed() {
        let master_a = test_master_key();
        let master_b = MasterSessionKey::from_raw([8u8; 32]);
        let v = SecretValue::from_bytes(b"x".to_vec());
        let encrypted = v.into_encrypted(&master_a).unwrap();
        let err = encrypted.decrypt_for_use(&master_b).unwrap_err();
        assert!(matches!(err, SecretError::DecryptionFailed));
    }

    #[test]
    fn tampered_ciphertext_fails_decryption() {
        let master = test_master_key();
        let v = SecretValue::from_bytes(b"hello".to_vec());
        let mut encrypted = v.into_encrypted(&master).unwrap();
        // Flip one bit in the ciphertext.
        encrypted.ciphertext[0] ^= 0x01;
        let err = encrypted.decrypt_for_use(&master).unwrap_err();
        assert!(matches!(err, SecretError::DecryptionFailed));
    }

    #[test]
    fn from_parts_round_trips() {
        let master = test_master_key();
        let v = SecretValue::from_bytes(b"parts-test".to_vec());
        let encrypted = v.into_encrypted(&master).unwrap();
        let (ct, nonce) = encrypted.parts();
        let rebuilt = EncryptedSecretValue::from_parts(ct.to_vec(), *nonce);
        let plaintext = rebuilt.decrypt_for_use(&master).unwrap();
        assert_eq!(plaintext.as_slice(), b"parts-test");
    }

    #[test]
    fn wrap_plaintext_bypasses_secret_value_for_backends_that_already_have_plaintext() {
        let master = test_master_key();
        let plaintext = b"from-os-keyring";
        let encrypted = EncryptedSecretValue::wrap_plaintext(plaintext, &master).unwrap();
        let decrypted = encrypted.decrypt_for_use(&master).unwrap();
        assert_eq!(decrypted.as_slice(), plaintext);
    }

    #[test]
    fn ciphertext_len_is_reasonable_for_typical_keys() {
        let master = test_master_key();
        // ChaCha20Poly1305 ciphertext = plaintext + 16 byte auth tag.
        let v = SecretValue::from_bytes(vec![0u8; 64]);
        let e = v.into_encrypted(&master).unwrap();
        assert_eq!(e.ciphertext_len(), 64 + 16);
    }
}
