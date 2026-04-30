//! v4.2 S14-S-4.5 — self-signed TLS cert generation for the
//! dashboard's secret-management endpoints.
//!
//! ## Design
//!
//! The dashboard's secret-management surface (`/api/brains/:id/
//! secrets/...`) carries plaintext secret values over the wire on
//! the operator's request. Loopback-only deployments make this
//! safe in practice (no other host on the network can reach the
//! port), but defense-in-depth + the prospect of multi-host
//! deployments motivate TLS.
//!
//! v1 of S-4.5 (this module) ships:
//!
//! - **Cert generation** — [`generate_self_signed_cert`] produces
//!   a fresh ECDSA P-256 cert + key pair via the `rcgen` crate.
//!   Pure-Rust, no OpenSSL dependency. SAN includes `127.0.0.1`,
//!   `::1`, `localhost`, and the brain_id (so federation peers
//!   can verify cross-brain HTTPS calls if/when those land).
//!
//! - **Fingerprint** — [`cert_fingerprint_sha256`] returns the
//!   SHA-256 fingerprint of the cert's DER bytes as
//!   `lowercase-hex` (no colons). The frontend pins this in
//!   localStorage on first visit so subsequent loads can verify
//!   the cert hasn't been swapped under it (TOFU pinning;
//!   "Trust on first use" is acceptable on loopback because the
//!   attacker would have to already control the operator's host
//!   for the swap to be achievable).
//!
//! - **Persistence** — `CertBundle::write_to_disk` saves cert PEM
//!   + key PEM as files under `<project>/.claude/brain/tls/`.
//!   The key file is written `0600` on Unix; on Windows the
//!   `.claude/` directory's ACLs are operator-managed (the
//!   default user-profile ACLs are sufficient for single-user
//!   adopters; production multi-user hosts should use
//!   `EncryptedFileBackend` once that wiring lands).
//!
//! ## Deferred to v2 (separate session)
//!
//! - **HTTPS server binding** in the dashboard via `axum-server`
//!   + `rustls`. The cert files exist after v1; the runtime
//!   wiring loads them and binds an HTTPS listener.
//! - **Frontend HTTPS redirect** for `/api/brains/:id/secrets/*`
//!   paths.
//! - **Browser fingerprint pinning** in localStorage.
//! - **Production cert import** (`tls-cert import` for
//!   operator-supplied certs from a real CA).
//! - **Storing the private key in `SecretBackend`** instead of a
//!   `0600` file. v1's file approach is correct for single-user
//!   adopters; multi-user hosts get the upgrade later.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TlsCertError {
    #[error("rcgen failed to generate cert: {0}")]
    Rcgen(#[from] rcgen::Error),
    #[error("io error during cert persistence: {0}")]
    Io(#[from] std::io::Error),
    #[error("cert PEM parse failed: {0}")]
    PemParse(String),
}

/// Result type alias for the module.
pub type Result<T> = std::result::Result<T, TlsCertError>;

/// A freshly-generated cert + private key pair, plus its DER form
/// (for fingerprint computation without re-parsing PEM).
///
/// Both PEM strings are owned. The private key is sensitive — do
/// NOT log or `Debug` print this struct in production paths.
/// (We intentionally do NOT derive Debug to make accidental
/// logging a compile error.)
#[derive(Clone)]
pub struct CertBundle {
    /// PEM-encoded X.509 certificate. Public — safe to log.
    pub cert_pem: String,
    /// PEM-encoded private key (PKCS#8). Sensitive — never log.
    pub key_pem: String,
    /// DER bytes of the certificate (for fingerprint).
    pub cert_der: Vec<u8>,
}

impl CertBundle {
    /// SHA-256 fingerprint of the certificate's DER bytes,
    /// lowercase hex without separators. Matches the form
    /// browsers display in their cert UIs (when colons are
    /// stripped) and is operator-comparable across hosts.
    pub fn fingerprint_sha256_hex(&self) -> String {
        cert_fingerprint_sha256(&self.cert_der)
    }

    /// Write cert + key to `<dir>/cert.pem` + `<dir>/key.pem`.
    /// Creates the directory if missing. On Unix, the key file
    /// is chmod 0600 immediately after write.
    ///
    /// Returns the two written paths so callers can surface them
    /// in operator-facing output.
    pub fn write_to_disk(&self, dir: &Path) -> Result<(PathBuf, PathBuf)> {
        std::fs::create_dir_all(dir)?;
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");
        std::fs::write(&cert_path, &self.cert_pem)?;
        std::fs::write(&key_path, &self.key_pem)?;
        // Defense in depth: tighten permissions on the key file.
        // On Windows this is a no-op (ACLs handle access); on
        // Unix we drop group + world bits so only the owner can
        // read.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&key_path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&key_path, perms)?;
        }
        Ok((cert_path, key_path))
    }
}

/// SHA-256 fingerprint of an X.509 cert's DER bytes, formatted
/// as lowercase hex (no colons or other separators).
///
/// Public so callers that already hold DER bytes (e.g. cert
/// loaded from disk) don't need to round-trip through CertBundle.
pub fn cert_fingerprint_sha256(cert_der: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(cert_der);
    let digest = h.finalize();
    hex::encode(digest)
}

/// Generate a fresh self-signed cert valid for 5 years.
///
/// **SAN entries:**
/// - `127.0.0.1` (loopback IPv4)
/// - `::1` (loopback IPv6)
/// - `localhost`
/// - the supplied `brain_id` (kebab-case identifier; allows
///   federation peers to verify cross-brain HTTPS calls if/when
///   that wiring lands)
///
/// **Algorithm:** ECDSA P-256 with SHA-256 signature. Modern
/// curves are smaller than RSA (~256-byte cert vs ~2KB), faster,
/// and supported by every modern browser.
pub fn generate_self_signed_cert(brain_id: &str) -> Result<CertBundle> {
    use rcgen::{
        CertificateParams, DistinguishedName, DnType, KeyPair, SanType,
    };

    let mut params = CertificateParams::default();

    // Distinguished name: CN = brain_id. Adopters viewing the cert
    // in their browser see "neurogrim-myproject" or similar.
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, format!("neurogrim-{brain_id}"));
    dn.push(DnType::OrganizationName, "neurogrim");
    params.distinguished_name = dn;

    // SANs. Production browsers verify against SAN, not CN, so
    // these are the load-bearing entries.
    params.subject_alt_names = vec![
        SanType::IpAddress("127.0.0.1".parse().expect("valid IPv4")),
        SanType::IpAddress("::1".parse().expect("valid IPv6")),
        SanType::DnsName(
            "localhost"
                .try_into()
                .expect("'localhost' is a valid DNS name"),
        ),
        SanType::DnsName(
            brain_id
                .try_into()
                .map_err(|e| TlsCertError::PemParse(format!(
                    "brain_id {brain_id:?} is not a valid DNS name: {e}"
                )))?,
        ),
    ];

    // Validity: not_before = now, not_after = now + 5 years.
    // Self-signed certs don't have a CA-imposed expiry; we pick
    // 5 years so the cert outlives typical operator setups but
    // doesn't sit forever — `tls-cert rotate` will re-generate
    // when the operator chooses. rcgen pulls in `time` so we
    // use OffsetDateTime directly.
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now;
    params.not_after = now + time::Duration::days(365 * 5);

    // Generate ECDSA P-256 key pair + self-sign.
    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    let cert_pem = cert.pem();
    let cert_der = cert.der().to_vec();
    let key_pem = key_pair.serialize_pem();

    Ok(CertBundle {
        cert_pem,
        key_pem,
        cert_der,
    })
}

/// Read an already-saved cert PEM file and return its DER bytes
/// (for fingerprint comparison or HTTPS-server loading).
///
/// Looks for the BEGIN/END CERTIFICATE markers and base64-decodes
/// the body. Stays narrow — adopters who need full PEM parsing
/// (chained certs, encrypted keys) should use a dedicated crate.
pub fn cert_der_from_pem_file(path: &Path) -> Result<Vec<u8>> {
    let pem = std::fs::read_to_string(path)?;
    let begin = "-----BEGIN CERTIFICATE-----";
    let end = "-----END CERTIFICATE-----";
    let begin_idx = pem.find(begin).ok_or_else(|| {
        TlsCertError::PemParse(format!(
            "{}: missing BEGIN CERTIFICATE marker",
            path.display()
        ))
    })?;
    let end_idx = pem.find(end).ok_or_else(|| {
        TlsCertError::PemParse(format!(
            "{}: missing END CERTIFICATE marker",
            path.display()
        ))
    })?;
    let body_start = begin_idx + begin.len();
    if end_idx <= body_start {
        return Err(TlsCertError::PemParse(format!(
            "{}: BEGIN appears after END",
            path.display()
        )));
    }
    let body = &pem[body_start..end_idx];
    // Strip whitespace/newlines from the base64 body.
    let cleaned: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    STANDARD
        .decode(cleaned)
        .map_err(|e| TlsCertError::PemParse(format!("base64 decode: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn generate_returns_valid_pem() {
        let bundle = generate_self_signed_cert("test-brain").unwrap();
        assert!(bundle.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(bundle.cert_pem.contains("END CERTIFICATE"));
        assert!(bundle.key_pem.contains("BEGIN PRIVATE KEY"));
        assert!(bundle.key_pem.contains("END PRIVATE KEY"));
        assert!(!bundle.cert_der.is_empty());
    }

    #[test]
    fn generate_two_certs_in_a_row_yield_distinct_keys() {
        let a = generate_self_signed_cert("test").unwrap();
        let b = generate_self_signed_cert("test").unwrap();
        assert_ne!(a.key_pem, b.key_pem);
        assert_ne!(a.cert_der, b.cert_der);
    }

    #[test]
    fn fingerprint_is_64_hex_chars() {
        let bundle = generate_self_signed_cert("test").unwrap();
        let fp = bundle.fingerprint_sha256_hex();
        assert_eq!(fp.len(), 64, "SHA-256 hex = 64 chars");
        assert!(
            fp.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
            "lowercase hex only"
        );
    }

    #[test]
    fn fingerprint_is_deterministic_for_same_cert() {
        let bundle = generate_self_signed_cert("test").unwrap();
        let fp1 = bundle.fingerprint_sha256_hex();
        let fp2 = cert_fingerprint_sha256(&bundle.cert_der);
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn fingerprint_differs_per_generated_cert() {
        let a = generate_self_signed_cert("test").unwrap();
        let b = generate_self_signed_cert("test").unwrap();
        assert_ne!(a.fingerprint_sha256_hex(), b.fingerprint_sha256_hex());
    }

    #[test]
    fn write_to_disk_creates_both_files() {
        let dir = TempDir::new().unwrap();
        let bundle = generate_self_signed_cert("test").unwrap();
        let (cert_path, key_path) = bundle.write_to_disk(dir.path()).unwrap();
        assert!(cert_path.exists());
        assert!(key_path.exists());
        let cert_text = std::fs::read_to_string(&cert_path).unwrap();
        let key_text = std::fs::read_to_string(&key_path).unwrap();
        assert_eq!(cert_text, bundle.cert_pem);
        assert_eq!(key_text, bundle.key_pem);
    }

    #[test]
    fn write_to_disk_creates_parent_dir() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("a/b/c/tls");
        let bundle = generate_self_signed_cert("test").unwrap();
        let (cert_path, _) = bundle.write_to_disk(&nested).unwrap();
        assert!(cert_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn write_to_disk_chmods_key_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let bundle = generate_self_signed_cert("test").unwrap();
        let (_, key_path) = bundle.write_to_disk(dir.path()).unwrap();
        let mode = std::fs::metadata(&key_path).unwrap().permissions().mode();
        // Owner read/write only; no group / world bits.
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn cert_der_from_pem_file_round_trips() {
        let dir = TempDir::new().unwrap();
        let bundle = generate_self_signed_cert("test").unwrap();
        let (cert_path, _) = bundle.write_to_disk(dir.path()).unwrap();
        let der = cert_der_from_pem_file(&cert_path).unwrap();
        assert_eq!(der, bundle.cert_der);
        assert_eq!(
            cert_fingerprint_sha256(&der),
            bundle.fingerprint_sha256_hex()
        );
    }

    #[test]
    fn cert_der_from_pem_file_rejects_garbage() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("not-pem.txt");
        std::fs::write(&path, "this is not a PEM file").unwrap();
        let err = cert_der_from_pem_file(&path).unwrap_err();
        assert!(matches!(err, TlsCertError::PemParse(_)));
    }

    #[test]
    fn cert_der_from_pem_file_rejects_truncated_pem() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("partial.pem");
        // Has BEGIN but no END.
        std::fs::write(
            &path,
            "-----BEGIN CERTIFICATE-----\nABCDEF\n",
        )
        .unwrap();
        let err = cert_der_from_pem_file(&path).unwrap_err();
        assert!(matches!(err, TlsCertError::PemParse(_)));
    }
}
