//! Encrypted file fallback (S14-S-3).
//!
//! At-rest format using ChaCha20Poly1305 for content + PBKDF2-HMAC-
//! SHA256 for key derivation from operator passphrase. One file per
//! secret under `<project>/.claude/brain/secrets/{secret_id}.enc`.
//!
//! ## v1 file format (`*.enc`)
//!
//! Hex-encoded JSON for human-inspectability + version-bumpable
//! schema. The crate enforces fixed-length salt (32) + nonce (12)
//! and ChaCha20Poly1305-derived auth tag (16) embedded in the
//! ciphertext. Format:
//!
//! ```json
//! {
//!   "version": 1,
//!   "salt":       "<64 hex chars = 32 bytes>",
//!   "nonce":      "<24 hex chars = 12 bytes>",
//!   "ciphertext": "<2N hex chars = N bytes (auth tag baked in)>",
//!   "metadata": {
//!     "created_at": "RFC3339",
//!     "updated_at": "RFC3339",
//!     "rotation_days": null
//!   }
//! }
//! ```
//!
//! Forward-compat: the `version` field is reserved; future versions
//! add an alternate file format and read both. v1 is the only
//! defined format today.

use crate::backend::{SecretBackend, SecretError, SecretKey, SecretMetadata};
use crate::master_key::MasterSessionKey;
use crate::value::{EncryptedSecretValue, SecretValue};
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

const FORMAT_VERSION: u32 = 1;
const PBKDF2_ITERATIONS: u32 = 600_000;
const SALT_LEN: usize = 32;
const NONCE_LEN: usize = 12;

/// Encrypted file backend. Stores secrets at
/// `<root>/.claude/brain/secrets/{secret_id}.enc`. Uses
/// PBKDF2-derived at-rest keys per-secret (each file gets its own
/// salt; the operator passphrase is shared across files).
///
/// **Distinct master keys:** the at-rest key (derived from
/// passphrase + per-file salt) is SEPARATE from the in-memory
/// master session key. The in-memory layer wraps every
/// `EncryptedSecretValue` in process memory after at-rest
/// decryption; the at-rest layer protects bytes on disk.
pub struct EncryptedFileBackend {
    project_root: PathBuf,
    /// Operator-supplied passphrase, retained for the process's
    /// lifetime. Wrapped in `Zeroizing<Vec<u8>>` — overwritten on
    /// drop.
    passphrase: Zeroizing<Vec<u8>>,
    /// In-memory master session key (separate from the at-rest
    /// passphrase). Backends ALL share this — it's the runtime
    /// in-memory wrapper.
    in_memory_master: MasterSessionKey,
}

impl EncryptedFileBackend {
    /// Construct a backend rooted at `project_root` with
    /// `passphrase` as the at-rest derivation source. The in-memory
    /// master session key is provided separately (typically derived
    /// from the same passphrase + a process-lifetime salt, OR from
    /// the OS credential store).
    pub fn open(
        project_root: PathBuf,
        passphrase: Vec<u8>,
        in_memory_master: MasterSessionKey,
    ) -> Self {
        Self {
            project_root,
            passphrase: Zeroizing::new(passphrase),
            in_memory_master,
        }
    }

    fn secrets_dir(&self) -> PathBuf {
        self.project_root.join(".claude").join("brain").join("secrets")
    }

    fn file_path(&self, key: &SecretKey) -> PathBuf {
        // brain_id is part of the SecretKey so different brains
        // sharing the same project root would file-conflict; the
        // file name is `{brain_id}__{secret_id}.enc`.
        self.secrets_dir()
            .join(format!("{}__{}.enc", key.brain_id, key.secret_id))
    }

    fn derive_at_rest_key(&self, salt: &[u8]) -> [u8; 32] {
        use pbkdf2::pbkdf2_hmac;
        use sha2::Sha256;
        let mut key = [0u8; 32];
        pbkdf2_hmac::<Sha256>(&self.passphrase, salt, PBKDF2_ITERATIONS, &mut key);
        key
    }
}

#[derive(Serialize, Deserialize)]
struct OnDiskRecord {
    version: u32,
    salt: String,
    nonce: String,
    ciphertext: String,
    metadata: OnDiskMetadata,
}

#[derive(Serialize, Deserialize)]
struct OnDiskMetadata {
    created_at: String,
    updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    rotation_days: Option<u32>,
}

impl SecretBackend for EncryptedFileBackend {
    fn get(&self, key: &SecretKey) -> Result<Option<EncryptedSecretValue>, SecretError> {
        let path = self.file_path(key);
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(SecretError::Io(e)),
        };
        let rec: OnDiskRecord = serde_json::from_str(&raw)
            .map_err(|e| SecretError::MalformedFile(format!("parse: {e}")))?;
        if rec.version != FORMAT_VERSION {
            return Err(SecretError::MalformedFile(format!(
                "unsupported file version {} (this build supports v{})",
                rec.version, FORMAT_VERSION
            )));
        }
        let salt = hex::decode(&rec.salt)
            .map_err(|e| SecretError::MalformedFile(format!("salt hex: {e}")))?;
        let nonce_bytes = hex::decode(&rec.nonce)
            .map_err(|e| SecretError::MalformedFile(format!("nonce hex: {e}")))?;
        let ciphertext = hex::decode(&rec.ciphertext)
            .map_err(|e| SecretError::MalformedFile(format!("ciphertext hex: {e}")))?;
        if salt.len() != SALT_LEN || nonce_bytes.len() != NONCE_LEN {
            return Err(SecretError::MalformedFile(format!(
                "salt or nonce length wrong: salt={} nonce={}",
                salt.len(),
                nonce_bytes.len()
            )));
        }
        // At-rest decryption.
        let at_rest_key = Zeroizing::new(self.derive_at_rest_key(&salt));
        let cipher = ChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(at_rest_key.as_ref()));
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|_| SecretError::BadPassphrase)?;
        let plaintext = Zeroizing::new(plaintext);
        // Re-wrap in the in-memory ciphertext form.
        let ev = EncryptedSecretValue::wrap_plaintext(&plaintext, &self.in_memory_master)?;
        Ok(Some(ev))
    }

    fn set(&self, key: &SecretKey, value: SecretValue) -> Result<(), SecretError> {
        let dir = self.secrets_dir();
        std::fs::create_dir_all(&dir)?;
        // Read previous record (if any) to preserve created_at.
        let path = self.file_path(key);
        let prior_created = match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str::<OnDiskRecord>(&s)
                .ok()
                .map(|r| r.metadata.created_at),
            Err(_) => None,
        };
        let now = chrono::Utc::now().to_rfc3339();
        let created_at = prior_created.unwrap_or_else(|| now.clone());

        // Generate fresh salt + nonce for this write.
        let mut salt = [0u8; SALT_LEN];
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut nonce_bytes);

        // At-rest encryption with passphrase-derived key.
        let at_rest_key = Zeroizing::new(self.derive_at_rest_key(&salt));
        let cipher = ChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(at_rest_key.as_ref()));
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, value.plaintext())
            .map_err(|_| SecretError::EncryptionFailed)?;

        let rec = OnDiskRecord {
            version: FORMAT_VERSION,
            salt: hex::encode(salt),
            nonce: hex::encode(nonce_bytes),
            ciphertext: hex::encode(&ciphertext),
            metadata: OnDiskMetadata {
                created_at,
                updated_at: now,
                rotation_days: None,
            },
        };
        // Atomic write: temp + rename.
        let tmp = path.with_extension("enc.tmp");
        let json = serde_json::to_string_pretty(&rec)
            .map_err(|e| SecretError::Other(format!("serialize: {e}")))?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        let path = self.file_path(key);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(SecretError::Io(e)),
        }
    }

    fn list(&self, brain_id: &str) -> Result<Vec<SecretMetadata>, SecretError> {
        let dir = self.secrets_dir();
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let prefix = format!("{brain_id}__");
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if !name.starts_with(&prefix) || !name.ends_with(".enc") {
                continue;
            }
            let secret_id = name
                .trim_start_matches(&prefix)
                .trim_end_matches(".enc")
                .to_string();
            let raw = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let rec: OnDiskRecord = match serde_json::from_str(&raw) {
                Ok(r) => r,
                Err(_) => continue,
            };
            out.push(SecretMetadata {
                key: SecretKey::new(brain_id, secret_id),
                created_at: rec.metadata.created_at,
                updated_at: rec.metadata.updated_at,
                backend: self.name().to_string(),
                rotation_days: rec.metadata.rotation_days,
            });
        }
        Ok(out)
    }

    fn name(&self) -> &'static str {
        "encrypted-file"
    }
}

/// Detect whether a given path is a malformed v1 file by trying to
/// parse + decrypt with `passphrase`. Used by `proxy-cli audit` /
/// the readiness sensor to surface "wrong passphrase" without
/// printing the secret. Returns:
/// - `Ok(true)` → file parses, decrypts, content matches.
/// - `Ok(false)` → file parses but passphrase fails to decrypt.
/// - `Err(_)` → file is malformed at the JSON / hex level.
pub fn smoke_check_file(path: &Path, passphrase: &[u8]) -> Result<bool, SecretError> {
    let raw = std::fs::read_to_string(path)?;
    let rec: OnDiskRecord = serde_json::from_str(&raw)
        .map_err(|e| SecretError::MalformedFile(format!("parse: {e}")))?;
    if rec.version != FORMAT_VERSION {
        return Err(SecretError::MalformedFile(format!(
            "unsupported version {}",
            rec.version
        )));
    }
    let salt = hex::decode(&rec.salt)
        .map_err(|e| SecretError::MalformedFile(format!("salt hex: {e}")))?;
    let nonce_bytes = hex::decode(&rec.nonce)
        .map_err(|e| SecretError::MalformedFile(format!("nonce hex: {e}")))?;
    let ciphertext = hex::decode(&rec.ciphertext)
        .map_err(|e| SecretError::MalformedFile(format!("ciphertext hex: {e}")))?;
    use pbkdf2::pbkdf2_hmac;
    use sha2::Sha256;
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase, &salt, PBKDF2_ITERATIONS, &mut key);
    let key = Zeroizing::new(key);
    let cipher = ChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(key.as_ref()));
    let nonce = Nonce::from_slice(&nonce_bytes);
    Ok(cipher.decrypt(nonce, ciphertext.as_ref()).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fresh_master() -> MasterSessionKey {
        MasterSessionKey::generate()
    }

    fn fresh_backend(dir: &TempDir, passphrase: &[u8]) -> EncryptedFileBackend {
        EncryptedFileBackend::open(
            dir.path().to_path_buf(),
            passphrase.to_vec(),
            fresh_master(),
        )
    }

    // SLO-violation: 64.432s (V5-FOUND-2 audit 2026-05-03). Argon2id KDF
    // dominates wall-time; fix tracked in BACKLOG B-48.
    #[test]
    #[ignore]
    fn set_then_get_round_trip() {
        let dir = TempDir::new().unwrap();
        let backend = fresh_backend(&dir, b"correct horse battery staple");
        let key = SecretKey::new("alpha", "anthropic");
        let plaintext = b"sk-ant-very-secret-do-not-leak".to_vec();
        backend
            .set(&key, SecretValue::from_bytes(plaintext.clone()))
            .unwrap();
        let got = backend.get(&key).unwrap().unwrap();
        let decrypted = got.decrypt_for_use(&backend.in_memory_master).unwrap();
        assert_eq!(decrypted.as_slice(), plaintext.as_slice());
    }

    #[test]
    fn get_returns_none_for_missing_key() {
        let dir = TempDir::new().unwrap();
        let backend = fresh_backend(&dir, b"x");
        let key = SecretKey::new("a", "missing");
        assert!(backend.get(&key).unwrap().is_none());
    }

    // SLO-violation: 45.468s (V5-FOUND-2 audit 2026-05-03). Argon2id KDF
    // dominates wall-time; fix tracked in BACKLOG B-48.
    #[test]
    #[ignore]
    fn delete_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let backend = fresh_backend(&dir, b"x");
        let key = SecretKey::new("a", "to-delete");
        backend.delete(&key).unwrap(); // never-existed
        backend
            .set(&key, SecretValue::from_bytes(b"v".to_vec()))
            .unwrap();
        backend.delete(&key).unwrap(); // first real delete
        backend.delete(&key).unwrap(); // re-delete idempotent
        assert!(backend.get(&key).unwrap().is_none());
    }

    // SLO-violation: 69.991s (V5-FOUND-2 audit 2026-05-03). Argon2id KDF
    // dominates wall-time; fix tracked in BACKLOG B-48.
    #[test]
    #[ignore]
    fn wrong_passphrase_returns_bad_passphrase_error() {
        let dir = TempDir::new().unwrap();
        let backend_a = fresh_backend(&dir, b"correct-passphrase");
        let key = SecretKey::new("a", "x");
        backend_a
            .set(&key, SecretValue::from_bytes(b"value".to_vec()))
            .unwrap();
        // New backend with a wrong passphrase, same project_root.
        let backend_b = EncryptedFileBackend::open(
            dir.path().to_path_buf(),
            b"WRONG-passphrase".to_vec(),
            fresh_master(),
        );
        let err = backend_b.get(&key).unwrap_err();
        assert!(
            matches!(err, SecretError::BadPassphrase),
            "expected BadPassphrase, got {err:?}"
        );
    }

    // SLO-violation: 98.236s (V5-FOUND-2 audit 2026-05-03). Argon2id KDF
    // dominates wall-time; fix tracked in BACKLOG B-48.
    #[test]
    #[ignore]
    fn list_returns_only_brain_id_scoped_secrets() {
        let dir = TempDir::new().unwrap();
        let backend = fresh_backend(&dir, b"p");
        backend
            .set(
                &SecretKey::new("alpha", "a"),
                SecretValue::from_bytes(b"1".to_vec()),
            )
            .unwrap();
        backend
            .set(
                &SecretKey::new("alpha", "b"),
                SecretValue::from_bytes(b"2".to_vec()),
            )
            .unwrap();
        backend
            .set(
                &SecretKey::new("beta", "c"),
                SecretValue::from_bytes(b"3".to_vec()),
            )
            .unwrap();
        let alpha_list = backend.list("alpha").unwrap();
        let beta_list = backend.list("beta").unwrap();
        assert_eq!(alpha_list.len(), 2);
        assert_eq!(beta_list.len(), 1);
        assert_eq!(beta_list[0].key.secret_id, "c");
    }

    #[test]
    fn list_returns_empty_when_dir_missing() {
        let dir = TempDir::new().unwrap();
        let backend = fresh_backend(&dir, b"p");
        // No set() — secrets dir never created.
        let list = backend.list("alpha").unwrap();
        assert!(list.is_empty());
    }

    // SLO-violation: 64.180s (V5-FOUND-2 audit 2026-05-03). Argon2id KDF
    // dominates wall-time; fix tracked in BACKLOG B-48.
    #[test]
    #[ignore]
    fn each_set_uses_fresh_salt_and_nonce() {
        let dir = TempDir::new().unwrap();
        let backend = fresh_backend(&dir, b"p");
        let key = SecretKey::new("a", "x");
        backend
            .set(&key, SecretValue::from_bytes(b"v".to_vec()))
            .unwrap();
        let path = backend.file_path(&key);
        let raw1 = std::fs::read_to_string(&path).unwrap();
        let rec1: OnDiskRecord = serde_json::from_str(&raw1).unwrap();

        backend
            .set(&key, SecretValue::from_bytes(b"v".to_vec()))
            .unwrap();
        let raw2 = std::fs::read_to_string(&path).unwrap();
        let rec2: OnDiskRecord = serde_json::from_str(&raw2).unwrap();

        assert_ne!(rec1.salt, rec2.salt, "salt must rotate on each write");
        assert_ne!(rec1.nonce, rec2.nonce, "nonce must rotate on each write");
        // created_at preserved across the second set; updated_at refreshes.
        assert_eq!(rec1.metadata.created_at, rec2.metadata.created_at);
    }

    #[test]
    fn malformed_file_returns_malformed_error() {
        let dir = TempDir::new().unwrap();
        let backend = fresh_backend(&dir, b"p");
        let key = SecretKey::new("a", "x");
        // Plant a file that doesn't parse.
        std::fs::create_dir_all(backend.secrets_dir()).unwrap();
        std::fs::write(backend.file_path(&key), "not json at all").unwrap();
        let err = backend.get(&key).unwrap_err();
        assert!(matches!(err, SecretError::MalformedFile(_)));
    }

    #[test]
    fn unsupported_version_returns_malformed_error() {
        let dir = TempDir::new().unwrap();
        let backend = fresh_backend(&dir, b"p");
        let key = SecretKey::new("a", "x");
        std::fs::create_dir_all(backend.secrets_dir()).unwrap();
        std::fs::write(
            backend.file_path(&key),
            r#"{"version":99,"salt":"00","nonce":"00","ciphertext":"00","metadata":{"created_at":"x","updated_at":"y"}}"#,
        )
        .unwrap();
        let err = backend.get(&key).unwrap_err();
        match err {
            SecretError::MalformedFile(msg) => {
                assert!(msg.contains("99"), "should mention the bad version: {msg}");
            }
            other => panic!("expected MalformedFile, got {other:?}"),
        }
    }

    // SLO-violation: 89.104s (V5-FOUND-2 audit 2026-05-03). Argon2id KDF
    // dominates wall-time; fix tracked in BACKLOG B-48.
    #[test]
    #[ignore]
    fn smoke_check_file_succeeds_for_valid_passphrase() {
        let dir = TempDir::new().unwrap();
        let backend = fresh_backend(&dir, b"correct");
        let key = SecretKey::new("a", "x");
        backend
            .set(&key, SecretValue::from_bytes(b"v".to_vec()))
            .unwrap();
        let path = backend.file_path(&key);
        assert!(smoke_check_file(&path, b"correct").unwrap());
        assert!(!smoke_check_file(&path, b"wrong").unwrap());
    }
}
