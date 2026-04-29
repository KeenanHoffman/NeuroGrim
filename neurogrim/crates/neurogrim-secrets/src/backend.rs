//! `SecretBackend` trait + addressing types (S14-S-1).

use serde::{Deserialize, Serialize};

/// Address of a stored secret. Service-name convention is
/// `neurogrim-{brain_id}-{secret_id}` for OS-native backends; the
/// brain_id distinguishes secrets across multi-Brain machines, the
/// secret_id distinguishes them within a single Brain.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecretKey {
    pub brain_id: String,
    pub secret_id: String,
}

impl SecretKey {
    pub fn new(brain_id: impl Into<String>, secret_id: impl Into<String>) -> Self {
        Self {
            brain_id: brain_id.into(),
            secret_id: secret_id.into(),
        }
    }

    /// OS-native service-name convention: `neurogrim-{brain_id}-{secret_id}`.
    /// This is what gets passed to DPAPI / Keychain / libsecret as
    /// the canonical lookup string. Document for adopters; future
    /// rotation tools assume this shape.
    pub fn service_name(&self) -> String {
        format!("neurogrim-{}-{}", self.brain_id, self.secret_id)
    }
}

/// Metadata about a stored secret. What `list()` returns. **Never
/// includes the value.**
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretMetadata {
    pub key: SecretKey,
    /// When the secret was first written, RFC3339 UTC.
    pub created_at: String,
    /// When the secret was last set / rotated, RFC3339 UTC.
    pub updated_at: String,
    /// Backend that owns this secret (`os-native` | `encrypted-file`).
    pub backend: String,
    /// Optional rotation policy from `secret-refs.yaml`. None when
    /// not declared.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub rotation_days: Option<u32>,
}

/// Errors any backend can produce. The Display impl is deliberately
/// generic — callers should NOT format these into operator-facing
/// messages without redaction (the inner Source might mention
/// secret-id paths or backend internals).
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("secret not found")]
    NotFound,
    #[error("backend not available: {0}")]
    BackendUnavailable(String),
    #[error("encryption failed")]
    EncryptionFailed,
    #[error("decryption failed (wrong key, corrupted ciphertext, or wrong passphrase)")]
    DecryptionFailed,
    #[error("bad passphrase")]
    BadPassphrase,
    #[error("malformed secret file: {0}")]
    MalformedFile(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("backend error: {0}")]
    Other(String),
}

/// Storage backend for encrypted secrets. Implementations: OS-native
/// (DPAPI / Keychain / libsecret) and encrypted-file (Cha-Cha20Poly1305
/// + PBKDF2).
///
/// All methods are sync. Backends are I/O-bounded but not network-
/// bounded; sync surface keeps the call sites simple.
pub trait SecretBackend: Send + Sync {
    /// Fetch a secret by key. Returns `None` (wrapped in `Ok`) when
    /// the key is absent — distinguished from I/O / decryption
    /// errors which return `Err`.
    fn get(&self, key: &SecretKey) -> Result<Option<crate::EncryptedSecretValue>, SecretError>;

    /// Store a secret. Plaintext is consumed (zeroized on drop of
    /// the input `SecretValue`).
    fn set(&self, key: &SecretKey, value: crate::SecretValue) -> Result<(), SecretError>;

    /// Delete a secret. Idempotent: deleting an already-absent key
    /// returns `Ok(())`.
    fn delete(&self, key: &SecretKey) -> Result<(), SecretError>;

    /// List all secrets the backend knows about. Metadata only;
    /// values are NEVER returned by this method.
    fn list(&self, brain_id: &str) -> Result<Vec<SecretMetadata>, SecretError>;

    /// Backend name (`os-native` | `encrypted-file`). Used in
    /// `SecretMetadata::backend` and in operator-facing diagnostics.
    fn name(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_name_matches_documented_convention() {
        let k = SecretKey::new("neurogrim", "claude_proxy_upstream_key");
        assert_eq!(
            k.service_name(),
            "neurogrim-neurogrim-claude_proxy_upstream_key"
        );
    }

    #[test]
    fn secret_metadata_round_trips_via_serde() {
        let m = SecretMetadata {
            key: SecretKey::new("alpha", "anthropic"),
            created_at: "2026-04-29T18:00:00Z".to_string(),
            updated_at: "2026-04-29T18:30:00Z".to_string(),
            backend: "os-native".to_string(),
            rotation_days: Some(90),
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: SecretMetadata = serde_json::from_str(&s).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn rotation_days_is_optional_in_serde() {
        let json = r#"{"key":{"brain_id":"a","secret_id":"b"},"created_at":"x","updated_at":"y","backend":"os-native"}"#;
        let m: SecretMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(m.rotation_days, None);
    }

    #[test]
    fn secret_error_messages_dont_leak_secret_content() {
        // Even when callers wrap a Display-formatted SecretError
        // into operator-facing output, the messages must NOT contain
        // any secret content. Verify the Display impls only mention
        // the failure category, not the underlying bytes.
        let e = SecretError::DecryptionFailed;
        assert!(!format!("{e}").to_lowercase().contains("plaintext"));
        let e = SecretError::BadPassphrase;
        assert_eq!(format!("{e}"), "bad passphrase");
    }
}
