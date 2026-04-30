//! v4.2 S14-S-4.5 v2 — HTTPS server binding for the dashboard's
//! secret-management endpoints.
//!
//! Pairs with the v1 cert lifecycle in `neurogrim-secrets::tls`
//! (operator runs `neurogrim secrets tls-cert generate` once;
//! cert + key land at `<project>/.claude/brain/tls/{cert,key}.pem`).
//! When those files exist on dashboard startup, this module binds
//! a second HTTPS listener on `<http_port>+1` serving the same
//! router. Plain HTTP stays available for non-secret traffic so
//! adopters who don't need HTTPS see no change.
//!
//! ## What's NOT here
//!
//! - **Path-level enforcement.** All routes are served on both
//!   listeners. The frontend chooses HTTPS for `/api/brains/:id/
//!   secrets/*` paths via its own client logic; the server
//!   doesn't reject HTTP requests to those paths. v3 tightening
//!   (deny HTTP for `secrets/*`) lands once the UI page (S-6) is
//!   in operator hands and we can verify nothing depends on the
//!   permissive default.
//!
//! - **Browser fingerprint pinning.** That's a frontend concern
//!   (localStorage), not a server-side check. Lands with the v3
//!   tightening above.
//!
//! - **Real-CA cert import.** Operators stuck with the dev cert
//!   today get the same SAN entries (`localhost` / `127.0.0.1` /
//!   brain_id), which works for loopback. `tls-cert import` for
//!   real-CA certs is in the deferred list.

use axum_server::tls_rustls::RustlsConfig;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Once;

/// Install the rustls `ring` crypto provider exactly once per
/// process. rustls 0.23 made provider selection explicit — without
/// this call the first TLS handshake panics with "Could not
/// automatically determine the process-level CryptoProvider".
///
/// Idempotent across calls and concurrency-safe via `std::sync::Once`.
/// Tests + production both go through the same path so behavior
/// matches.
fn ensure_crypto_provider() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // `install_default` returns Err if a provider is already
        // installed (e.g., a transitive dep beat us to it). Either
        // way we're fine — the provider exists by the time this
        // returns. We deliberately discard the Err.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Resolved cert + key paths at the convention location for a
/// brain's TLS files: `<project>/.claude/brain/tls/{cert,key}.pem`.
///
/// Mirrors the path layout in `commands::secrets::default_tls_dir`
/// + `tls::CertBundle::write_to_disk`.
pub fn tls_paths(project_root: &Path) -> (PathBuf, PathBuf) {
    let dir = project_root.join(".claude").join("brain").join("tls");
    (dir.join("cert.pem"), dir.join("key.pem"))
}

/// True iff both cert.pem and key.pem exist at the convention
/// location. The dashboard uses this to decide whether to bind
/// an HTTPS listener at all.
pub fn tls_files_present(project_root: &Path) -> bool {
    let (cert, key) = tls_paths(project_root);
    cert.is_file() && key.is_file()
}

/// SHA-256 fingerprint of the on-disk cert at the convention
/// location. Returns `None` when the cert file is missing or
/// can't be parsed. Used by the frontend's TOFU pinning UX
/// (S14-S-4.5 v3).
pub fn cert_fingerprint(project_root: &Path) -> Option<String> {
    let (cert, _) = tls_paths(project_root);
    if !cert.is_file() {
        return None;
    }
    neurogrim_secrets::tls::cert_der_from_pem_file(&cert)
        .ok()
        .map(|der| neurogrim_secrets::tls::cert_fingerprint_sha256(&der))
}

/// HTTPS port = HTTP port + 1.
///
/// Picked deliberately so operators don't need a second config
/// knob. Adopters who have port collisions can kill the
/// conflicting service or use `--port` to shift the HTTP port
/// (HTTPS follows automatically). v3 may add `--tls-port`.
pub fn https_port_for(http_port: u16) -> u16 {
    http_port.saturating_add(1)
}

/// Load a Rustls config from the cert + key files at
/// `<project>/.claude/brain/tls/`. Returns `Ok(None)` when the
/// files are missing (caller falls back to HTTP-only).
///
/// Side-effect: installs the rustls `ring` crypto provider once
/// per process (no-op on subsequent calls). Required for any TLS
/// op under rustls 0.23+.
pub async fn load_rustls_config(
    project_root: &Path,
) -> anyhow::Result<Option<RustlsConfig>> {
    if !tls_files_present(project_root) {
        return Ok(None);
    }
    ensure_crypto_provider();
    let (cert, key) = tls_paths(project_root);
    let config = RustlsConfig::from_pem_file(&cert, &key)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "load TLS config from {} + {}: {}",
                cert.display(),
                key.display(),
                e
            )
        })?;
    Ok(Some(config))
}

/// Resolve the HTTPS bind address from the HTTP one. Same IP,
/// HTTP port + 1.
pub fn https_addr_for(http_addr: &SocketAddr) -> SocketAddr {
    SocketAddr::new(http_addr.ip(), https_port_for(http_addr.port()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use tempfile::TempDir;

    #[test]
    fn tls_paths_uses_convention_location() {
        let root = Path::new("/proj");
        let (cert, key) = tls_paths(root);
        assert_eq!(cert, Path::new("/proj/.claude/brain/tls/cert.pem"));
        assert_eq!(key, Path::new("/proj/.claude/brain/tls/key.pem"));
    }

    #[test]
    fn tls_files_present_returns_false_when_dir_missing() {
        let dir = TempDir::new().unwrap();
        assert!(!tls_files_present(dir.path()));
    }

    #[test]
    fn tls_files_present_returns_false_when_only_cert_exists() {
        let dir = TempDir::new().unwrap();
        let (cert, _) = tls_paths(dir.path());
        std::fs::create_dir_all(cert.parent().unwrap()).unwrap();
        std::fs::write(&cert, "fake cert").unwrap();
        assert!(!tls_files_present(dir.path()));
    }

    #[test]
    fn tls_files_present_returns_false_when_only_key_exists() {
        let dir = TempDir::new().unwrap();
        let (_, key) = tls_paths(dir.path());
        std::fs::create_dir_all(key.parent().unwrap()).unwrap();
        std::fs::write(&key, "fake key").unwrap();
        assert!(!tls_files_present(dir.path()));
    }

    #[test]
    fn tls_files_present_returns_true_when_both_exist() {
        let dir = TempDir::new().unwrap();
        let (cert, key) = tls_paths(dir.path());
        std::fs::create_dir_all(cert.parent().unwrap()).unwrap();
        std::fs::write(&cert, "fake cert").unwrap();
        std::fs::write(&key, "fake key").unwrap();
        assert!(tls_files_present(dir.path()));
    }

    #[test]
    fn https_port_is_http_plus_one() {
        assert_eq!(https_port_for(8420), 8421);
        assert_eq!(https_port_for(0), 1);
    }

    #[test]
    fn https_port_saturates_at_max() {
        // Doesn't panic at u16::MAX; saturating arithmetic.
        assert_eq!(https_port_for(u16::MAX), u16::MAX);
    }

    #[test]
    fn https_addr_preserves_ip() {
        let http = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8420);
        let https = https_addr_for(&http);
        assert_eq!(https.ip(), http.ip());
        assert_eq!(https.port(), 8421);
    }

    #[tokio::test]
    async fn load_rustls_config_returns_none_when_files_absent() {
        let dir = TempDir::new().unwrap();
        let cfg = load_rustls_config(dir.path()).await.unwrap();
        assert!(cfg.is_none());
    }

    #[tokio::test]
    async fn load_rustls_config_loads_real_generated_cert() {
        // Generate a real cert + key via neurogrim-secrets, write
        // them to the convention location, then verify the
        // dashboard's loader picks them up. End-to-end check:
        // S-4.5 v1 (cert generation) → S-4.5 v2 (cert loading).
        let dir = TempDir::new().unwrap();
        let bundle =
            neurogrim_secrets::tls::generate_self_signed_cert("test").unwrap();
        let tls_dir = dir.path().join(".claude/brain/tls");
        bundle.write_to_disk(&tls_dir).unwrap();
        let cfg = load_rustls_config(dir.path()).await.unwrap();
        assert!(cfg.is_some(), "expected RustlsConfig to load");
    }

    #[tokio::test]
    async fn load_rustls_config_errors_on_garbage_files() {
        let dir = TempDir::new().unwrap();
        let (cert, key) = tls_paths(dir.path());
        std::fs::create_dir_all(cert.parent().unwrap()).unwrap();
        std::fs::write(&cert, "this is not a PEM file").unwrap();
        std::fs::write(&key, "neither is this").unwrap();
        let result = load_rustls_config(dir.path()).await;
        assert!(result.is_err(), "expected garbage files to surface error");
    }
}
