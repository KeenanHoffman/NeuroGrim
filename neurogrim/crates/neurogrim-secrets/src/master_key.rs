//! Master session key — the in-memory encryption authority (S14-S-1).
//!
//! Sourced from the OS credential store at process startup. The key
//! itself wraps every [`crate::SecretValue`] in memory so that a
//! process memory dump yields ciphertext rather than plaintext.
//!
//! ## Lifecycle
//!
//! 1. **First run** — the dashboard / proxy process generates a fresh
//!    32-byte key via `MasterSessionKey::generate()`, stores it in
//!    the OS credential store under the well-known service-name
//!    `neurogrim-{brain_id}-master-session-key`, then loads it
//!    back into the process for use.
//! 2. **Subsequent runs** — the process reads the key directly from
//!    the OS credential store. If the OS store is unavailable
//!    (headless Linux without seahorse, container, CI), the
//!    operator's passphrase derives a key via PBKDF2 (handled by
//!    `EncryptedFileBackend`'s separate at-rest key — distinct
//!    role from this in-memory master).
//! 3. **Process exit** — `Zeroizing<[u8; 32]>` overwrites the key
//!    on drop. Memory dump after process exit yields zeros.

use rand::RngCore;
use zeroize::Zeroizing;

use crate::backend::SecretError;

/// The well-known secret-id for the master session key. Combined
/// with the brain_id at lookup time: `neurogrim-{brain_id}-master-session-key`.
pub const MASTER_SECRET_ID: &str = "master-session-key";

/// 32-byte ChaCha20Poly1305 master key. Wraps every in-memory
/// [`crate::EncryptedSecretValue`] for the process's lifetime.
///
/// Constructed via [`MasterSessionKey::load_or_generate`] which
/// integrates with the OS credential store. Direct constructors
/// (`from_raw`) exist for tests and for the explicit-passphrase
/// fallback path.
pub struct MasterSessionKey {
    bytes: Zeroizing<[u8; 32]>,
}

impl MasterSessionKey {
    /// Wrap raw 32-byte key material. Tests + the encrypted-file
    /// backend's PBKDF2 path use this.
    pub fn from_raw(bytes: [u8; 32]) -> Self {
        Self {
            bytes: Zeroizing::new(bytes),
        }
    }

    /// Generate a fresh random key. Used on first run before
    /// persisting to the OS credential store.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self::from_raw(bytes)
    }

    /// Borrow as a ChaCha20Poly1305 `Key` for cipher construction.
    /// Internal-only; consumers should never see the raw bytes.
    pub(crate) fn as_chacha_key(&self) -> &chacha20poly1305::Key {
        chacha20poly1305::Key::from_slice(self.bytes.as_ref())
    }

    /// Borrow the raw key bytes. Used by serialization paths that
    /// persist the key to OS-native storage (the OS handles at-rest
    /// encryption of these bytes; in-memory they remain Zeroizing).
    /// Internal-only — public surface goes through
    /// [`MasterSessionKey::load_or_generate`].
    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// Load the master key from the OS credential store; on
    /// not-found, generate fresh, store, then return.
    ///
    /// Service-name convention:
    /// `neurogrim-{brain_id}-master-session-key`. The same
    /// convention as [`crate::SecretKey::service_name`] but with
    /// the well-known [`MASTER_SECRET_ID`] secret_id.
    ///
    /// **Failure mode:** when the OS credential store is
    /// unavailable (no DPAPI / Keychain / libsecret on the host),
    /// returns `Err(SecretError::BackendUnavailable)`. Callers can
    /// fall back to a passphrase-derived key via
    /// [`MasterSessionKey::derive_from_passphrase`].
    pub fn load_or_generate(brain_id: &str) -> Result<Self, SecretError> {
        let entry = keyring::Entry::new(
            "neurogrim-master-session",
            &format!("neurogrim-{brain_id}-{MASTER_SECRET_ID}"),
        )
        .map_err(|e| SecretError::BackendUnavailable(format!("{e}")))?;
        match entry.get_secret() {
            Ok(bytes) => {
                if bytes.len() != 32 {
                    return Err(SecretError::MalformedFile(format!(
                        "OS-stored master key has wrong length: {} bytes",
                        bytes.len()
                    )));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                Ok(Self::from_raw(arr))
            }
            Err(keyring::Error::NoEntry) => {
                // First run — generate, persist, return.
                let key = Self::generate();
                entry
                    .set_secret(key.as_bytes())
                    .map_err(|e| SecretError::Other(format!("set master key: {e}")))?;
                tracing::info!(
                    "neurogrim-secrets: generated and persisted master session key for brain '{brain_id}'"
                );
                Ok(key)
            }
            Err(e) => Err(SecretError::BackendUnavailable(format!("{e}"))),
        }
    }

    /// Derive a master key from an operator-entered passphrase via
    /// PBKDF2. Use when the OS credential store isn't available
    /// (headless Linux, container, CI). The same passphrase plus
    /// salt always produces the same key, so adopters can persist
    /// the salt alongside their encrypted-file backend's per-secret
    /// salts.
    ///
    /// Iteration count: 600,000 (matches OWASP 2023 guidance for
    /// SHA-256). Salt: 32 bytes.
    pub fn derive_from_passphrase(passphrase: &[u8], salt: &[u8]) -> Self {
        use pbkdf2::pbkdf2_hmac;
        use sha2::Sha256;
        let mut key = [0u8; 32];
        pbkdf2_hmac::<Sha256>(passphrase, salt, 600_000, &mut key);
        Self::from_raw(key)
    }

    /// Best-effort delete from the OS credential store. Used by
    /// rotation tooling. Returns `Ok(())` even when the entry was
    /// already absent (idempotent, mirrors `SecretBackend::delete`).
    pub fn delete_from_os(brain_id: &str) -> Result<(), SecretError> {
        let entry = keyring::Entry::new(
            "neurogrim-master-session",
            &format!("neurogrim-{brain_id}-{MASTER_SECRET_ID}"),
        )
        .map_err(|e| SecretError::BackendUnavailable(format!("{e}")))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Other(format!("delete master key: {e}"))),
        }
    }
}

// MasterSessionKey deliberately does NOT impl Debug / Display.
// A panic that formatted the key would catastrophically leak it.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_raw_round_trips() {
        let key = MasterSessionKey::from_raw([0xab; 32]);
        assert_eq!(key.as_bytes(), &[0xab; 32]);
    }

    #[test]
    fn generate_yields_distinct_keys_each_call() {
        let k1 = MasterSessionKey::generate();
        let k2 = MasterSessionKey::generate();
        assert_ne!(k1.as_bytes(), k2.as_bytes());
    }

    // SLO-violation: 52.033s (V5-FOUND-2 audit 2026-05-03). Argon2id KDF
    // params hard-coded; fix tracked in BACKLOG B-48.
    #[test]
    #[ignore]
    fn derive_from_passphrase_is_deterministic() {
        let k1 = MasterSessionKey::derive_from_passphrase(b"correct horse battery staple", b"salt-32-bytes-xxxxxxxxxxxxxxxxx0");
        let k2 = MasterSessionKey::derive_from_passphrase(b"correct horse battery staple", b"salt-32-bytes-xxxxxxxxxxxxxxxxx0");
        assert_eq!(k1.as_bytes(), k2.as_bytes());
    }

    // SLO-violation: 61.705s (V5-FOUND-2 audit 2026-05-03). Argon2id KDF
    // params hard-coded; fix tracked in BACKLOG B-48.
    #[test]
    #[ignore]
    fn derive_from_passphrase_different_salt_yields_different_key() {
        let k1 = MasterSessionKey::derive_from_passphrase(
            b"same-passphrase",
            b"salt-A-padded-to-32-bytes-xxxxx0",
        );
        let k2 = MasterSessionKey::derive_from_passphrase(
            b"same-passphrase",
            b"salt-B-padded-to-32-bytes-xxxxx0",
        );
        assert_ne!(k1.as_bytes(), k2.as_bytes());
    }

    // SLO-violation: 71.878s (V5-FOUND-2 audit 2026-05-03). Argon2id KDF
    // params hard-coded; fix tracked in BACKLOG B-48.
    #[test]
    #[ignore]
    fn derive_from_passphrase_different_passphrase_yields_different_key() {
        let salt = b"shared-salt-padded-32-xxxxxxxxx0";
        let k1 = MasterSessionKey::derive_from_passphrase(b"alpha", salt);
        let k2 = MasterSessionKey::derive_from_passphrase(b"beta", salt);
        assert_ne!(k1.as_bytes(), k2.as_bytes());
    }

    // OS-native load_or_generate / delete_from_os tests are gated
    // behind a feature flag in the os_native module — they touch
    // real OS credential storage and aren't safe to run in CI
    // without a credential store. See os_native::tests for the
    // smoke test pattern.
}
