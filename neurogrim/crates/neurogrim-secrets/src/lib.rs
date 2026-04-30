//! # neurogrim-secrets
//!
//! Encrypted secrets crate for NeuroGrim — v4.2 S14.
//!
//! Implements the four-layer encryption model (epic refinement,
//! 2026-04-29):
//!
//! | Layer | Where | Implementation |
//! |---|---|---|
//! | Wire | TCP | TLS on secret endpoints (S14-S-4.5; future) |
//! | Process boundary | JSON in/out | dashboard zeroizes request buffers |
//! | **In-memory** | runtime values | this crate's `EncryptedSecretValue` |
//! | At rest | OS / disk | `OsNativeBackend` or `EncryptedFileBackend` |
//!
//! ## What this crate exports
//!
//! - **[`SecretBackend`]** — backend trait. v1 implementations:
//!   [`OsNativeBackend`] (DPAPI / Keychain / libsecret via the
//!   `keyring` crate) and [`EncryptedFileBackend`] (ChaCha20Poly1305
//!   + PBKDF2; documented format).
//! - **[`SecretValue`]** — caller-facing input wrapper holding
//!   plaintext bytes in `Zeroizing<Vec<u8>>`. Memory zeroed on drop.
//!   No `Debug` / `Display` impls — explicit redaction required.
//! - **[`EncryptedSecretValue`]** — the in-memory ciphertext form.
//!   What backends return on `get()`. Decrypted only inside an
//!   explicit [`EncryptedSecretValue::decrypt_for_use`] call which
//!   yields a short-lived `Zeroizing<Vec<u8>>`.
//! - **[`MasterSessionKey`]** — 32-byte ChaCha20Poly1305 key derived
//!   from the OS credential store at process startup; wraps every
//!   [`SecretValue`] in memory so that a process memory dump yields
//!   ciphertext, not plaintext.
//! - **[`SecretKey`]** — addressing for a stored secret (composed of
//!   brain id + secret id; service-name convention
//!   `neurogrim-{brain_id}-{secret_id}`).
//! - **[`SecretMetadata`]** — what `list()` returns: id, when set,
//!   when last rotated. **Never** the value.
//!
//! ## Invariants
//!
//! - **No plaintext in `Debug`/`Display`** — `SecretValue` and
//!   `EncryptedSecretValue` deliberately do NOT implement either.
//!   Logging a secret requires explicit redaction.
//! - **Zeroize on drop** — every type holding key material wraps
//!   bytes in `zeroize::Zeroizing`.
//! - **Short plaintext window** — `decrypt_for_use` returns owned
//!   `Zeroizing<Vec<u8>>`; callers should drop the returned value
//!   immediately after the upstream API call. Holding it across
//!   `await` points or in long-lived state is a code-review smell.

pub mod backend;
pub mod encrypted_file;
pub mod master_key;
pub mod os_native;
pub mod tls;
pub mod value;

pub use backend::{SecretBackend, SecretError, SecretKey, SecretMetadata};
pub use encrypted_file::EncryptedFileBackend;
pub use master_key::MasterSessionKey;
pub use os_native::OsNativeBackend;
pub use tls::{
    cert_fingerprint_sha256, generate_self_signed_cert, CertBundle,
    TlsCertError,
};
pub use value::{EncryptedSecretValue, SecretValue};
