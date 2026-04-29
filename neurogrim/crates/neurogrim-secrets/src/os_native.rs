//! OS-native credential adapter (S14-S-2).
//!
//! Wraps the [`keyring`] crate (~10M downloads), which itself wraps:
//!
//! | Platform | Underlying API |
//! |---|---|
//! | Windows | DPAPI (Credential Manager) |
//! | macOS | Keychain |
//! | Native Linux (with seahorse) | libsecret over D-Bus |
//! | iOS | Keychain |
//!
//! ## Failure modes
//!
//! - **WSL without seahorse**: libsecret unavailable. `keyring`
//!   returns `Error::PlatformFailure`. Caller falls back to
//!   [`crate::EncryptedFileBackend`] with a `tracing::warn!`.
//! - **Container / CI**: same — no credential store. Falls back to
//!   the file backend.
//! - **Headless Linux**: same.
//!
//! ## Service-name convention
//!
//! `neurogrim-{brain_id}-{secret_id}` (see
//! [`crate::SecretKey::service_name`]). The `keyring` crate uses
//! `(service, user)` for its lookup; we split into `("neurogrim",
//! service_name)` so all entries share a single root namespace.

use crate::backend::{SecretBackend, SecretError, SecretKey, SecretMetadata};
use crate::master_key::MasterSessionKey;
use crate::value::{EncryptedSecretValue, SecretValue};

/// OS-native backend. Construct one per Brain (each brain has its
/// own master session key, stored under
/// `neurogrim-{brain_id}-master-session-key`).
pub struct OsNativeBackend {
    master: MasterSessionKey,
    /// Index of secret_ids per brain_id, persisted to a separate
    /// keyring entry so `list()` can enumerate without scanning the
    /// whole credential store. Some platforms (macOS Keychain
    /// notably) don't expose enumeration.
    /// Format: JSON `{"secrets": [{"key": SecretKey, "metadata": SecretMetadata}, ...]}`.
    /// Stored under service `neurogrim-os-native-index`,
    /// account-name = brain_id.
    index_account: String,
}

impl OsNativeBackend {
    /// Construct a backend bound to `brain_id`. Loads or generates
    /// the master session key from the OS credential store.
    pub fn open(brain_id: &str) -> Result<Self, SecretError> {
        let master = MasterSessionKey::load_or_generate(brain_id)?;
        Ok(Self {
            master,
            index_account: brain_id.to_string(),
        })
    }

    /// Test-only constructor that takes a pre-generated master key.
    /// Avoids touching the real OS credential store during unit
    /// tests; integration smoke uses `open()`.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn from_master(brain_id: &str, master: MasterSessionKey) -> Self {
        Self {
            master,
            index_account: brain_id.to_string(),
        }
    }

    fn index_entry(&self) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new("neurogrim-os-native-index", &self.index_account)
            .map_err(|e| SecretError::BackendUnavailable(format!("index entry: {e}")))
    }

    fn read_index(&self) -> Result<Vec<SecretMetadata>, SecretError> {
        let entry = self.index_entry()?;
        match entry.get_password() {
            Ok(json) => serde_json::from_str::<Vec<SecretMetadata>>(&json)
                .map_err(|e| SecretError::MalformedFile(format!("index parse: {e}"))),
            Err(keyring::Error::NoEntry) => Ok(Vec::new()),
            Err(e) => Err(SecretError::BackendUnavailable(format!("read index: {e}"))),
        }
    }

    fn write_index(&self, list: &[SecretMetadata]) -> Result<(), SecretError> {
        let entry = self.index_entry()?;
        let json = serde_json::to_string(list)
            .map_err(|e| SecretError::Other(format!("serialize index: {e}")))?;
        entry
            .set_password(&json)
            .map_err(|e| SecretError::BackendUnavailable(format!("write index: {e}")))
    }
}

impl SecretBackend for OsNativeBackend {
    fn get(&self, key: &SecretKey) -> Result<Option<EncryptedSecretValue>, SecretError> {
        let entry = keyring::Entry::new("neurogrim", &key.service_name())
            .map_err(|e| SecretError::BackendUnavailable(format!("{e}")))?;
        match entry.get_secret() {
            Ok(bytes) => {
                // The OS handles at-rest encryption; on retrieval we
                // get plaintext, which we immediately wrap in the
                // in-memory ciphertext form so the runtime layer
                // never holds raw bytes longer than the wrap call.
                let ev = EncryptedSecretValue::wrap_plaintext(&bytes, &self.master)?;
                Ok(Some(ev))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::BackendUnavailable(format!("get: {e}"))),
        }
    }

    fn set(&self, key: &SecretKey, value: SecretValue) -> Result<(), SecretError> {
        let entry = keyring::Entry::new("neurogrim", &key.service_name())
            .map_err(|e| SecretError::BackendUnavailable(format!("{e}")))?;
        // Hand plaintext directly to the OS keyring; it handles
        // at-rest encryption.
        entry
            .set_secret(value.plaintext())
            .map_err(|e| SecretError::BackendUnavailable(format!("set: {e}")))?;
        // Update the index.
        let mut list = self.read_index()?;
        let now = chrono::Utc::now().to_rfc3339();
        if let Some(existing) = list.iter_mut().find(|m| m.key == *key) {
            existing.updated_at = now;
        } else {
            list.push(SecretMetadata {
                key: key.clone(),
                created_at: now.clone(),
                updated_at: now,
                backend: self.name().to_string(),
                rotation_days: None,
            });
        }
        self.write_index(&list)?;
        Ok(())
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        let entry = keyring::Entry::new("neurogrim", &key.service_name())
            .map_err(|e| SecretError::BackendUnavailable(format!("{e}")))?;
        match entry.delete_credential() {
            Ok(()) => {}
            Err(keyring::Error::NoEntry) => {} // idempotent
            Err(e) => return Err(SecretError::BackendUnavailable(format!("delete: {e}"))),
        }
        let mut list = self.read_index()?;
        list.retain(|m| m.key != *key);
        self.write_index(&list)?;
        Ok(())
    }

    fn list(&self, brain_id: &str) -> Result<Vec<SecretMetadata>, SecretError> {
        let list = self.read_index()?;
        Ok(list
            .into_iter()
            .filter(|m| m.key.brain_id == brain_id)
            .collect())
    }

    fn name(&self) -> &'static str {
        "os-native"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // OS-native backend tests touch the real OS credential store.
    // They're disabled by default — set NEUROGRIM_TEST_OS_NATIVE=1
    // in your shell to enable them. The integration smoke at
    // commit-time runs under this flag on Windows; CI defaults to
    // off so the test suite stays hermetic.
    fn os_native_tests_enabled() -> bool {
        std::env::var("NEUROGRIM_TEST_OS_NATIVE").map(|v| v == "1").unwrap_or(false)
    }

    #[test]
    fn os_native_set_get_delete_round_trip() {
        if !os_native_tests_enabled() {
            return;
        }
        // Use a unique brain_id so stale state from previous runs
        // doesn't pollute. (We delete what we created at the end.)
        let brain_id = format!(
            "test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let backend = OsNativeBackend::open(&brain_id).expect("open backend");
        let key = SecretKey::new(&brain_id, "round-trip");
        let plaintext = b"sentinel-do-not-leak".to_vec();
        backend
            .set(&key, SecretValue::from_bytes(plaintext.clone()))
            .expect("set");
        let got = backend.get(&key).expect("get").expect("present");
        let decrypted = got.decrypt_for_use(&backend.master).unwrap();
        assert_eq!(decrypted.as_slice(), plaintext.as_slice());
        // List sees it.
        let list = backend.list(&brain_id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].key, key);
        // Delete; gone.
        backend.delete(&key).expect("delete");
        assert!(backend.get(&key).unwrap().is_none());
        // Cleanup the brain's master key + index.
        MasterSessionKey::delete_from_os(&brain_id).ok();
    }

    #[test]
    fn os_native_get_returns_none_for_missing_key() {
        if !os_native_tests_enabled() {
            return;
        }
        let brain_id = format!(
            "test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let backend = OsNativeBackend::open(&brain_id).expect("open");
        let key = SecretKey::new(&brain_id, "nonexistent");
        assert!(backend.get(&key).unwrap().is_none());
        MasterSessionKey::delete_from_os(&brain_id).ok();
    }

    #[test]
    fn os_native_delete_is_idempotent() {
        if !os_native_tests_enabled() {
            return;
        }
        let brain_id = format!(
            "test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let backend = OsNativeBackend::open(&brain_id).expect("open");
        let key = SecretKey::new(&brain_id, "double-delete");
        // delete with no prior set — must be Ok.
        backend.delete(&key).expect("first delete");
        backend.delete(&key).expect("second delete");
        MasterSessionKey::delete_from_os(&brain_id).ok();
    }
}
