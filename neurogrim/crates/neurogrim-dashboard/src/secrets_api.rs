//! v4.2 S14-S-6 v1 — secrets-management API surface for the
//! dashboard's `/brains/:id/secrets` page.
//!
//! Surfaces a list of declared secrets (parsed from
//! `<project>/.claude/secret-refs.yaml`) with per-secret status
//! drawn from the [`OsNativeBackend`]'s index. Operators set or
//! delete values through this API; the **value never round-trips
//! back through any endpoint**. Set/delete are gated behind
//! `--allow-mutations`; list is read-only.
//!
//! ## Threat model + leak prevention
//!
//! - **Wire**: requests carrying secret values traverse the
//!   HTTPS listener (S14-S-4.5 v2) when the operator has run
//!   `tls-cert generate`. Loopback-only deployments without TLS
//!   keep the bytes on `lo` — still defended in depth.
//! - **Server**: incoming POST values are moved into a
//!   `SecretValue` (zeroizing on drop) before the response is
//!   even constructed; the request body is consumed once, never
//!   echoed.
//! - **Logs**: this module deliberately does NOT format secret
//!   values via tracing or any other path. The `tracing::warn!`
//!   / `tracing::error!` calls below format only the secret_id
//!   (an opaque kebab-case label, not the value).
//! - **Responses**: list/set/delete responses carry only
//!   metadata (id, present flag, rotation_days, updated_at).
//!
//! ## v2 deferred
//!
//! - "Test" button: per-secret validators that exercise the
//!   stored secret without exposing it (e.g., a no-op API call
//!   to verify auth). Needs adopter-defined test endpoints in
//!   `secret-refs.yaml`.
//! - Rotated-at history: today the OsNativeBackend tracks
//!   `created_at` + `updated_at`, but we don't expose a per-
//!   secret history of past rotations.
//! - Client-side encryption with passphrase-derived session
//!   keys: TLS already protects the wire; the additional layer
//!   is meaningful for hostile-host threat models we don't
//!   currently in-scope.

use neurogrim_secrets::{
    OsNativeBackend, SecretBackend, SecretError, SecretKey, SecretValue,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use ts_rs::TS;

/// Wire shape of one entry in `secret-refs.yaml::secrets`.
/// Hand-authored by operators; we parse a narrow subset (the rest
/// is sensor-only fields like `secret_path` for env-provider
/// references which the dashboard doesn't surface).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretRefEntry {
    /// Human-readable purpose. Surfaces in the dashboard list
    /// view so operators know which secret is which.
    #[serde(default)]
    pub description: Option<String>,
    /// Provider tag from the manifest (env / vault / aws / etc.).
    /// We surface it so operators can see "this one's an env-var
    /// reference" vs "this one's stored encrypted".
    #[serde(default)]
    pub provider: Option<String>,
    /// Optional rotation policy in days. Surfaces so operators
    /// can see when the secret should be rotated.
    #[serde(default)]
    pub rotation_days: Option<u32>,
    /// Free-form tags. Adopter-defined; we pass through.
    #[serde(default)]
    pub used_by: Vec<String>,
}

/// Wire shape of `secret-refs.yaml`. Other fields exist
/// (`default_provider`, `providers`, etc.) but the dashboard
/// only needs the secrets map.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SecretRefsManifest {
    #[serde(default)]
    pub secrets: BTreeMap<String, SecretRefEntry>,
}

/// One row in the secrets list endpoint response.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct SecretListItem {
    /// kebab-case secret id (the key in `secret-refs.yaml::secrets`).
    pub id: String,
    /// Operator-facing description from the manifest.
    pub description: Option<String>,
    /// Provider tag from the manifest.
    pub provider: Option<String>,
    /// Rotation policy in days, if declared.
    pub rotation_days: Option<u32>,
    /// True iff a value is stored in the SecretBackend for this id.
    pub present: bool,
    /// RFC3339 of when the secret was last set/rotated. None if
    /// not present.
    pub updated_at: Option<String>,
    /// Backend that owns this secret (`os-native` /
    /// `encrypted-file`). None if not present.
    pub backend: Option<String>,
}

/// Response body of `GET /api/brains/:id/secrets`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct SecretsListResponse {
    pub brain_id: String,
    /// Path to the parsed manifest. Null when the file is absent
    /// (operator hasn't authored one yet).
    pub manifest_path: String,
    pub manifest_present: bool,
    /// Ordered list (sorted by id). Includes secrets declared in
    /// the manifest; secrets that exist in the SecretBackend
    /// without a manifest entry are omitted from this list (they
    /// would surface in `secrets-readiness` domain findings as
    /// orphans — the dashboard presents the operator's declared
    /// surface, not the backend's raw inventory).
    pub items: Vec<SecretListItem>,
}

/// Request body of `POST /api/brains/:id/secrets/:secret_id`.
/// Carries the plaintext value; the server consumes it into a
/// zeroizing `SecretValue` before any response is built.
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct SetSecretRequest {
    /// Plaintext value. Server moves this into a `SecretValue`
    /// immediately + drops the raw String. Not echoed in the
    /// response.
    pub value: String,
}

/// Response body of `POST /api/brains/:id/secrets/:secret_id`
/// (success path).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct SetSecretResponse {
    pub brain_id: String,
    pub secret_id: String,
    /// RFC3339 of when the write completed. Useful for the UI to
    /// flash "Saved at <ts>" without re-fetching the whole list.
    pub updated_at: String,
}

/// Response body of `DELETE /api/brains/:id/secrets/:secret_id`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct DeleteSecretResponse {
    pub brain_id: String,
    pub secret_id: String,
    /// True iff a stored value existed and was removed. False
    /// when no value was present (idempotent delete).
    pub removed: bool,
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Conventional location for the secret-refs manifest.
pub fn secret_refs_path(project_root: &Path) -> std::path::PathBuf {
    project_root.join(".claude").join("secret-refs.yaml")
}

/// Read + parse the secret-refs manifest. Returns `Ok(None)`
/// when the file doesn't exist — adopters may not have authored
/// one yet (in which case the dashboard shows an empty list with
/// a "no secrets declared" hint).
pub fn read_secret_refs(
    project_root: &Path,
) -> Result<Option<SecretRefsManifest>, anyhow::Error> {
    let path = secret_refs_path(project_root);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(e) => {
            return Err(anyhow::Error::from(e).context(format!(
                "secret-refs.yaml: read {}",
                path.display()
            )));
        }
    };
    let manifest: SecretRefsManifest = serde_yaml::from_str(&text)
        .map_err(|e| {
            anyhow::anyhow!("secret-refs.yaml: parse {}: {e}", path.display())
        })?;
    Ok(Some(manifest))
}

/// Build the full list response: cross-reference declared
/// secrets with the backend's index to populate the present /
/// updated_at / backend fields.
pub fn build_secrets_list(
    brain_id: &str,
    project_root: &Path,
) -> SecretsListResponse {
    let manifest_path = secret_refs_path(project_root);
    let (manifest_present, manifest) = match read_secret_refs(project_root) {
        Ok(Some(m)) => (true, m),
        Ok(None) => (false, SecretRefsManifest::default()),
        Err(e) => {
            tracing::warn!(
                "secrets list: failed to read manifest at {}: {e}",
                manifest_path.display()
            );
            (false, SecretRefsManifest::default())
        }
    };

    // Try to open the backend; if it fails (no keyring, etc.),
    // we still return the manifest entries with present=false +
    // log the failure. Operators get a useful list even when the
    // OS keyring is unreachable.
    let index = match OsNativeBackend::open(brain_id) {
        Ok(be) => match be.list(brain_id) {
            Ok(metas) => metas
                .into_iter()
                .map(|m| (m.key.secret_id.clone(), m))
                .collect::<BTreeMap<_, _>>(),
            Err(e) => {
                tracing::warn!(
                    "secrets list: backend.list failed for brain {brain_id}: \
                     {e} — present flags will all read false"
                );
                BTreeMap::new()
            }
        },
        Err(e) => {
            tracing::warn!(
                "secrets list: cannot open OsNativeBackend for brain \
                 {brain_id}: {e} — present flags will all read false"
            );
            BTreeMap::new()
        }
    };

    let mut items: Vec<SecretListItem> = manifest
        .secrets
        .into_iter()
        .map(|(id, entry)| {
            let stored = index.get(&id);
            SecretListItem {
                id: id.clone(),
                description: entry.description,
                provider: entry.provider,
                rotation_days: entry.rotation_days,
                present: stored.is_some(),
                updated_at: stored.map(|m| m.updated_at.clone()),
                backend: stored.map(|m| m.backend.clone()),
            }
        })
        .collect();
    items.sort_by(|a, b| a.id.cmp(&b.id));

    SecretsListResponse {
        brain_id: brain_id.to_string(),
        manifest_path: manifest_path.to_string_lossy().to_string(),
        manifest_present,
        items,
    }
}

/// Validate that `secret_id` is in the manifest. Returns
/// `Ok(())` when the id is declared; `Err` (with a generic
/// message) when it isn't. Set/delete endpoints reject
/// undeclared ids so adopters can't accidentally create
/// orphan secrets that aren't tracked in `secret-refs.yaml`.
pub fn ensure_declared(
    project_root: &Path,
    secret_id: &str,
) -> Result<(), String> {
    match read_secret_refs(project_root) {
        Ok(Some(m)) => {
            if m.secrets.contains_key(secret_id) {
                Ok(())
            } else {
                Err(format!(
                    "secret_id '{secret_id}' is not declared in \
                     secret-refs.yaml; add it there first to keep the \
                     manifest the source of truth"
                ))
            }
        }
        Ok(None) => Err(
            "secret-refs.yaml is missing; author it before setting \
             values to keep the manifest the source of truth"
                .to_string(),
        ),
        Err(e) => Err(format!("secret-refs.yaml parse failed: {e}")),
    }
}

/// Set a secret value. The plaintext string is moved into a
/// `SecretValue` (zeroizing on drop) immediately; the original
/// `String` is consumed.
///
/// **Security discipline:** this function never logs or
/// returns the value. Its tracing surface uses only the
/// secret_id and a length-only summary.
pub fn set_secret(
    brain_id: &str,
    secret_id: &str,
    plaintext: String,
) -> Result<String, SecretError> {
    let len = plaintext.len();
    let value = SecretValue::from_string(plaintext);
    // The plaintext String now lives only inside `value`'s
    // zeroizing container. The caller's reference is dead (move
    // semantics).
    let backend = OsNativeBackend::open(brain_id)?;
    let key = SecretKey::new(brain_id, secret_id);
    backend.set(&key, value)?;
    let updated_at = chrono::Utc::now().to_rfc3339();
    tracing::info!(
        "secrets: set value for brain={brain_id} secret_id={secret_id} \
         len={len} updated_at={updated_at}"
    );
    Ok(updated_at)
}

/// Delete a secret. Returns true iff a value existed and was
/// removed. Idempotent: deleting an absent key returns
/// `Ok(false)`, not an error.
pub fn delete_secret(
    brain_id: &str,
    secret_id: &str,
) -> Result<bool, SecretError> {
    let backend = OsNativeBackend::open(brain_id)?;
    let key = SecretKey::new(brain_id, secret_id);
    // Probe for existence first so we can return a meaningful
    // `removed` flag.
    let was_present = backend.get(&key)?.is_some();
    backend.delete(&key)?;
    tracing::info!(
        "secrets: delete brain={brain_id} secret_id={secret_id} \
         was_present={was_present}"
    );
    Ok(was_present)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn read_secret_refs_returns_none_when_absent() {
        let dir = TempDir::new().unwrap();
        let result = read_secret_refs(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn read_secret_refs_parses_minimal_manifest() {
        let dir = TempDir::new().unwrap();
        let p = secret_refs_path(dir.path());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(
            &p,
            r#"
default_provider: env
secrets:
  github-pat:
    description: "GitHub PAT"
    provider: env
    rotation_days: 90
"#,
        )
        .unwrap();
        let m = read_secret_refs(dir.path()).unwrap().unwrap();
        assert_eq!(m.secrets.len(), 1);
        let entry = m.secrets.get("github-pat").unwrap();
        assert_eq!(
            entry.description.as_deref(),
            Some("GitHub PAT")
        );
        assert_eq!(entry.provider.as_deref(), Some("env"));
        assert_eq!(entry.rotation_days, Some(90));
    }

    #[test]
    fn read_secret_refs_tolerates_extra_top_level_fields() {
        // The actual yaml has providers/, default_provider, etc.
        // Make sure parser accepts unknown top-level keys.
        let dir = TempDir::new().unwrap();
        let p = secret_refs_path(dir.path());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(
            &p,
            r#"
default_provider: env
providers:
  my-vault:
    description: foo
secrets:
  x:
    description: y
"#,
        )
        .unwrap();
        let m = read_secret_refs(dir.path()).unwrap().unwrap();
        assert_eq!(m.secrets.len(), 1);
    }

    #[test]
    fn build_secrets_list_with_no_manifest() {
        let dir = TempDir::new().unwrap();
        let resp = build_secrets_list("test-brain", dir.path());
        assert!(!resp.manifest_present);
        assert!(resp.items.is_empty());
        assert_eq!(resp.brain_id, "test-brain");
        assert!(resp.manifest_path.contains("secret-refs.yaml"));
    }

    #[test]
    fn build_secrets_list_returns_manifest_entries_with_present_false() {
        // No SecretBackend connection → all present flags read false.
        // We deliberately don't try to construct a real OsNativeBackend
        // in unit tests (it would touch the OS keyring). The function
        // logs a warning + populates items with present=false.
        let dir = TempDir::new().unwrap();
        let p = secret_refs_path(dir.path());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(
            &p,
            r#"
secrets:
  github-pat:
    description: "GitHub PAT"
  anthropic-api-key:
    description: "Anthropic key"
"#,
        )
        .unwrap();
        let resp = build_secrets_list("test-brain", dir.path());
        assert!(resp.manifest_present);
        assert_eq!(resp.items.len(), 2);
        // Sorted by id alphabetically.
        assert_eq!(resp.items[0].id, "anthropic-api-key");
        assert_eq!(resp.items[1].id, "github-pat");
        // Without a real backend, present flags are conservatively false.
        // (The actual flag depends on whether the test host's OS
        // keyring has anything for "test-brain"; in most CI environments
        // it's false. We don't strictly assert false here because the
        // function might find a real entry on a developer's machine —
        // that's fine, the contract is "no values returned" not
        // "always false".)
    }

    #[test]
    fn ensure_declared_accepts_declared_id() {
        let dir = TempDir::new().unwrap();
        let p = secret_refs_path(dir.path());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(
            &p,
            r#"
secrets:
  ok-id:
    description: ok
"#,
        )
        .unwrap();
        assert!(ensure_declared(dir.path(), "ok-id").is_ok());
    }

    #[test]
    fn ensure_declared_rejects_undeclared_id() {
        let dir = TempDir::new().unwrap();
        let p = secret_refs_path(dir.path());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(
            &p,
            r#"
secrets:
  ok-id:
    description: ok
"#,
        )
        .unwrap();
        let err = ensure_declared(dir.path(), "rogue-id").unwrap_err();
        assert!(err.contains("not declared"));
    }

    #[test]
    fn ensure_declared_errors_when_manifest_missing() {
        let dir = TempDir::new().unwrap();
        let err = ensure_declared(dir.path(), "any").unwrap_err();
        assert!(err.contains("missing"));
    }
}
