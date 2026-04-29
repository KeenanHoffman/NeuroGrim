//! Secrets-readiness advisory sensor (v4.2 S14-S-8).
//!
//! Reads `.claude/secret-refs.yaml` (the human-authored manifest of
//! declared secrets) and checks each declared secret against the
//! local encrypted-file backend's on-disk presence. Emits findings
//! for declared-but-missing and rotation-overdue cases.
//!
//! ## v1 scope
//!
//! - **Encrypted-file backend only** — checks for `.enc` file
//!   presence under `<root>/.claude/brain/secrets/`. OS-native
//!   backend presence isn't surfaced here because cross-platform
//!   credential-store enumeration is platform-specific (the
//!   keyring crate's API doesn't expose enumeration on macOS
//!   Keychain). v2 can check OS-native via the index file written
//!   by `OsNativeBackend::set` (see crates/neurogrim-secrets).
//! - **Manifest schema is permissive** — the existing
//!   `secret_refs` sensor's parser is best-effort YAML; we mirror
//!   that posture. Missing fields produce neutral findings, not
//!   errors.
//!
//! ## CMDB shape
//!
//! Returns the standard CMDB envelope. `extras` carries:
//!
//! - `declared_count` (u32) — total secrets in manifest
//! - `present_count` (u32) — secrets found on disk
//! - `missing_count` (u32)
//! - `rotation_overdue_count` (u32)
//! - `secrets` (array) — per-secret summary (id, present, age_days,
//!   rotation_days, overdue) — **never the value**

use crate::cmdb::{build_cmdb, Finding};
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::path::Path;

/// CMDB-envelope analysis. Mirrors the convention of every other
/// sensor: `analyze_<name>(project_root)` returns a `Value`
/// containing `score`, `findings`, `extras`, `meta`.
pub async fn analyze_secrets_readiness(project_root: &str) -> Value {
    let root = Path::new(project_root);
    let mut findings: Vec<Finding> = Vec::new();
    let mut extras: Vec<(&str, Value)> = Vec::new();

    let manifest_path = root.join(".claude").join("secret-refs.yaml");
    let manifest_str = match tokio::fs::read_to_string(&manifest_path).await {
        Ok(s) => s,
        Err(_) => {
            findings.push(Finding {
                name: "secrets_readiness:manifest_missing".into(),
                status: "missing".into(),
                points: 0,
                detail: Some(
                    "no .claude/secret-refs.yaml — sensor returns no findings; \
                     advisory weight 0.0 means this doesn't affect the unified score"
                        .into(),
                ),
            });
            extras.push(("declared_count", json!(0u32)));
            extras.push(("present_count", json!(0u32)));
            extras.push(("missing_count", json!(0u32)));
            extras.push(("rotation_overdue_count", json!(0u32)));
            extras.push(("secrets", json!([])));
            return build_cmdb("secrets-readiness", 100, findings, Some(extras), None);
        }
    };

    let manifest: Value = match serde_yaml::from_str(&manifest_str) {
        Ok(v) => v,
        Err(e) => {
            findings.push(Finding {
                name: "secrets_readiness:manifest_parse_error".into(),
                status: "error".into(),
                points: 0,
                detail: Some(format!(
                    ".claude/secret-refs.yaml parse error: {e}"
                )),
            });
            extras.push(("declared_count", json!(0u32)));
            extras.push(("present_count", json!(0u32)));
            extras.push(("missing_count", json!(0u32)));
            extras.push(("rotation_overdue_count", json!(0u32)));
            extras.push(("secrets", json!([])));
            return build_cmdb("secrets-readiness", 0, findings, Some(extras), None);
        }
    };

    // Brain id from optional manifest field; default to "default".
    let brain_id = manifest
        .get("brain_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    let secrets_dir = root.join(".claude").join("brain").join("secrets");

    let declared = manifest
        .get("secrets")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let declared_count = declared.len() as u32;

    let now = Utc::now();
    let mut present_count = 0u32;
    let mut missing_count = 0u32;
    let mut rotation_overdue_count = 0u32;
    let mut per_secret = Vec::new();

    for entry in &declared {
        let secret_id = entry
            .get("name")
            .or_else(|| entry.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("(unnamed)");
        let rotation_days = entry
            .get("rotation_days")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);
        let file_path =
            secrets_dir.join(format!("{brain_id}__{secret_id}.enc"));
        let present = file_path.is_file();

        // Read created_at from on-disk file if present + parseable.
        let mut age_days: Option<i64> = None;
        let mut overdue = false;
        if present {
            present_count += 1;
            if let Ok(raw) = std::fs::read_to_string(&file_path) {
                if let Ok(rec) = serde_json::from_str::<Value>(&raw) {
                    if let Some(meta) = rec.get("metadata") {
                        if let Some(updated_at) =
                            meta.get("updated_at").and_then(|v| v.as_str())
                        {
                            if let Ok(ts) = DateTime::parse_from_rfc3339(updated_at)
                            {
                                let age = now.signed_duration_since(ts.with_timezone(&Utc));
                                age_days = Some(age.num_days());
                                if let Some(rd) = rotation_days {
                                    if age >= Duration::days(rd as i64) {
                                        overdue = true;
                                        rotation_overdue_count += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            missing_count += 1;
            findings.push(Finding {
                name: format!("secrets_readiness:missing:{secret_id}"),
                status: "missing".into(),
                points: 0,
                detail: Some(format!(
                    "secret '{secret_id}' declared in secret-refs.yaml but no encrypted-file \
                     entry found at {}",
                    file_path.display()
                )),
            });
        }

        if overdue {
            findings.push(Finding {
                name: format!("secrets_readiness:rotation_overdue:{secret_id}"),
                status: "warning".into(),
                points: 0,
                detail: Some(format!(
                    "secret '{secret_id}' is {} days old; rotation_days={} declared in manifest",
                    age_days.unwrap_or(0),
                    rotation_days.unwrap_or(0)
                )),
            });
        }

        per_secret.push(json!({
            "secret_id": secret_id,
            "present": present,
            "age_days": age_days,
            "rotation_days": rotation_days,
            "overdue": overdue,
        }));
    }

    extras.push(("declared_count", json!(declared_count)));
    extras.push(("present_count", json!(present_count)));
    extras.push(("missing_count", json!(missing_count)));
    extras.push(("rotation_overdue_count", json!(rotation_overdue_count)));
    extras.push(("secrets", json!(per_secret)));

    // Score: 100 when no missing + no overdue; -10 per missing, -5
    // per overdue, floor 0. Advisory (weight 0.0); the score
    // surfaces in the dashboard but doesn't drive the unified
    // unless the operator promotes the domain.
    let raw = 100i32 - (missing_count as i32 * 10) - (rotation_overdue_count as i32 * 5);
    let score = raw.clamp(0, 100) as u8;

    build_cmdb("secrets-readiness", score, findings, Some(extras), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_manifest(dir: &TempDir, yaml: &str) {
        let path = dir.path().join(".claude").join("secret-refs.yaml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, yaml).unwrap();
    }

    fn write_enc_file(dir: &TempDir, brain: &str, secret: &str, updated_at: &str) {
        let secrets_dir = dir.path().join(".claude/brain/secrets");
        std::fs::create_dir_all(&secrets_dir).unwrap();
        let path = secrets_dir.join(format!("{brain}__{secret}.enc"));
        let body = serde_json::json!({
            "version": 1,
            "salt": "00",
            "nonce": "00",
            "ciphertext": "00",
            "metadata": {
                "created_at": updated_at,
                "updated_at": updated_at,
            }
        });
        std::fs::write(&path, body.to_string()).unwrap();
    }

    #[tokio::test]
    async fn no_manifest_returns_advisory_finding() {
        let dir = TempDir::new().unwrap();
        let v = analyze_secrets_readiness(&dir.path().to_string_lossy()).await;
        assert_eq!(v["declared_count"], 0);
        let findings = v["findings"].as_array().unwrap();
        assert!(
            findings.iter().any(|f| f["name"] == "secrets_readiness:manifest_missing"),
            "expected manifest_missing finding"
        );
    }

    #[tokio::test]
    async fn malformed_manifest_returns_parse_error() {
        let dir = TempDir::new().unwrap();
        write_manifest(&dir, "not: : : valid: yaml: ");
        let v = analyze_secrets_readiness(&dir.path().to_string_lossy()).await;
        let findings = v["findings"].as_array().unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f["name"] == "secrets_readiness:manifest_parse_error"),
            "expected manifest_parse_error"
        );
    }

    #[tokio::test]
    async fn declared_present_no_findings() {
        let dir = TempDir::new().unwrap();
        write_manifest(
            &dir,
            "brain_id: alpha\nsecrets:\n  - name: anthropic\n",
        );
        let now = Utc::now().to_rfc3339();
        write_enc_file(&dir, "alpha", "anthropic", &now);
        let v = analyze_secrets_readiness(&dir.path().to_string_lossy()).await;
        assert_eq!(v["declared_count"], 1);
        assert_eq!(v["present_count"], 1);
        assert_eq!(v["missing_count"], 0);
        // No missing/overdue findings.
        let findings = v["findings"].as_array().unwrap();
        assert!(findings.iter().all(|f| {
            !f["name"]
                .as_str()
                .unwrap()
                .starts_with("secrets_readiness:missing")
        }));
    }

    #[tokio::test]
    async fn declared_missing_emits_finding() {
        let dir = TempDir::new().unwrap();
        write_manifest(
            &dir,
            "brain_id: alpha\nsecrets:\n  - name: ghost\n",
        );
        let v = analyze_secrets_readiness(&dir.path().to_string_lossy()).await;
        assert_eq!(v["declared_count"], 1);
        assert_eq!(v["missing_count"], 1);
        let findings = v["findings"].as_array().unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f["name"] == "secrets_readiness:missing:ghost"),
            "expected missing:ghost finding"
        );
        // Score docked: 100 - 10 = 90.
        assert_eq!(v["score"], 90);
    }

    #[tokio::test]
    async fn rotation_overdue_emits_finding_when_past_threshold() {
        let dir = TempDir::new().unwrap();
        write_manifest(
            &dir,
            "brain_id: alpha\nsecrets:\n  - name: stale\n    rotation_days: 7\n",
        );
        // updated_at is 30 days ago — well past 7-day rotation.
        let old = (Utc::now() - Duration::days(30)).to_rfc3339();
        write_enc_file(&dir, "alpha", "stale", &old);
        let v = analyze_secrets_readiness(&dir.path().to_string_lossy()).await;
        assert_eq!(v["rotation_overdue_count"], 1);
        let findings = v["findings"].as_array().unwrap();
        assert!(findings
            .iter()
            .any(|f| f["name"] == "secrets_readiness:rotation_overdue:stale"));
        // Score: 100 - 0 missing - 1 overdue * 5 = 95.
        assert_eq!(v["score"], 95);
    }

    #[tokio::test]
    async fn rotation_within_threshold_emits_no_finding() {
        let dir = TempDir::new().unwrap();
        write_manifest(
            &dir,
            "brain_id: alpha\nsecrets:\n  - name: fresh\n    rotation_days: 30\n",
        );
        let recent = (Utc::now() - Duration::days(5)).to_rfc3339();
        write_enc_file(&dir, "alpha", "fresh", &recent);
        let v = analyze_secrets_readiness(&dir.path().to_string_lossy()).await;
        assert_eq!(v["rotation_overdue_count"], 0);
        let findings = v["findings"].as_array().unwrap();
        assert!(!findings
            .iter()
            .any(|f| f["name"] == "secrets_readiness:rotation_overdue:fresh"));
    }

    #[tokio::test]
    async fn unnamed_secret_falls_back_to_id_field() {
        let dir = TempDir::new().unwrap();
        // Some adopters use `id:` instead of `name:`. Both work.
        write_manifest(
            &dir,
            "brain_id: alpha\nsecrets:\n  - id: by-id-key\n",
        );
        let v = analyze_secrets_readiness(&dir.path().to_string_lossy()).await;
        assert_eq!(v["declared_count"], 1);
        assert_eq!(v["missing_count"], 1);
        // The finding should mention "by-id-key", confirming the
        // fallback fired.
        let findings = v["findings"].as_array().unwrap();
        assert!(findings
            .iter()
            .any(|f| f["name"] == "secrets_readiness:missing:by-id-key"));
    }

    #[tokio::test]
    async fn multiple_declared_secrets_aggregate_correctly() {
        let dir = TempDir::new().unwrap();
        write_manifest(
            &dir,
            "brain_id: alpha\nsecrets:\n  - name: a\n  - name: b\n  - name: c\n",
        );
        // Only `b` exists.
        let now = Utc::now().to_rfc3339();
        write_enc_file(&dir, "alpha", "b", &now);
        let v = analyze_secrets_readiness(&dir.path().to_string_lossy()).await;
        assert_eq!(v["declared_count"], 3);
        assert_eq!(v["present_count"], 1);
        assert_eq!(v["missing_count"], 2);
        // Score: 100 - 20 = 80.
        assert_eq!(v["score"], 80);
    }

    #[tokio::test]
    async fn cmdb_extras_includes_per_secret_summary() {
        let dir = TempDir::new().unwrap();
        write_manifest(
            &dir,
            "brain_id: alpha\nsecrets:\n  - name: x\n",
        );
        let v = analyze_secrets_readiness(&dir.path().to_string_lossy()).await;
        let secrets = v["secrets"].as_array().unwrap();
        assert_eq!(secrets.len(), 1);
        let entry = &secrets[0];
        assert_eq!(entry["secret_id"], "x");
        assert_eq!(entry["present"], false);
        assert_eq!(entry["overdue"], false);
        // Sensor never surfaces secret values — only metadata.
        // Use the field names as a regression guard against future
        // additions that might inadvertently expose content.
        assert!(entry.get("plaintext").is_none());
        assert!(entry.get("value").is_none());
    }
}
