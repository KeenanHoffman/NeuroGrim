//! Reader for `.claude/supply-chain-accepted-advisories.toml`.
//!
//! # File shape
//!
//! Operator-curated list of advisory IDs that have been reviewed and
//! consciously accepted as "not-a-risk-for-us-right-now." Example:
//!
//! ```toml
//! # .claude/supply-chain-accepted-advisories.toml
//!
//! [[accepted]]
//! id = "RUSTSEC-2024-0436"
//! package = "paste"                                    # optional
//! note = "Unmaintained-notice; transitive via rmcp; proc-macro-only; accepted 2026-04-24."
//! # expires_at = "2026-10-24"                          # optional; ISO-8601 date
//!
//! [[accepted]]
//! id = "RUSTSEC-2024-0400"
//! note = "..."
//! ```
//!
//! # Hygiene — non-empty `note` is required
//!
//! Acceptance without a documented rationale is the failure mode
//! this file is designed to prevent: "oh just accept it" silently
//! swallowing supply-chain signals. Entries with an empty/missing
//! `note` are SKIPPED with a tracing warning, so the advisory
//! continues to deduct from the score until the operator writes
//! down why it's accepted.
//!
//! A richer 2-phase append-only ledger
//! (`supply-chain-decision-ledger.jsonl`) lands in E-SC-6. Until
//! then, this TOML file is the v1 triage surface.
//!
//! # Graceful degradation
//!
//! - File missing → `Ok(empty)` (acceptance is opt-in).
//! - File unparseable → `Ok(empty)` + warning. Treating all
//!   advisories as unaccepted is the conservative posture.
//! - Entry with empty `note` → skipped + warning.
//! - Entry with past `expires_at` → skipped + warning (acceptance
//!   has lapsed; operator must re-review).

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

/// Path (relative to project_root) where the operator-accepted
/// advisories file lives.
const ACCEPTED_REL_PATH: &str = ".claude/supply-chain-accepted-advisories.toml";

/// Read the optional `.claude/supply-chain-accepted-advisories.toml`
/// and return the set of accepted advisory IDs (hygiene-filtered).
///
/// Only IDs whose entry has a non-empty `note` AND a non-expired
/// `expires_at` (or no `expires_at`) are returned.
///
/// Searches for the file in two places:
/// 1. `<project_root>/.claude/supply-chain-accepted-advisories.toml`
///    (standard; matches where `.claude/` lives for most users)
/// 2. `<project_root>/../.claude/...` (fallback for NeuroGrim's
///    unusual layout where the cargo workspace is in a subdir but
///    the Brain config is at repo root — same strategy as
///    `rustsec.rs::locate_advisory_db`).
pub fn read(project_root: &Path) -> Result<HashSet<String>> {
    let primary = project_root.join(ACCEPTED_REL_PATH);
    let fallback = project_root
        .parent()
        .map(|p| p.join(ACCEPTED_REL_PATH));

    let path = if primary.exists() {
        primary
    } else if let Some(f) = fallback.filter(|p| p.exists()) {
        f
    } else {
        return Ok(HashSet::new());
    };

    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                "could not read {}: {:#}; treating all advisories as unaccepted",
                path.display(),
                e
            );
            return Ok(HashSet::new());
        }
    };

    parse_accepted_toml(&raw, Utc::now().date_naive())
}

/// Parse the TOML content and return validated accepted IDs.
///
/// Separated from `read` so tests can inject a clock (via
/// `now_date`) and input text deterministically.
fn parse_accepted_toml(raw: &str, now_date: NaiveDate) -> Result<HashSet<String>> {
    let parsed: AcceptedFile = match toml::from_str(raw) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(
                "supply-chain-accepted-advisories.toml parse failed: {:#}; \
                 treating all advisories as unaccepted",
                e
            );
            return Ok(HashSet::new());
        }
    };

    let mut out = HashSet::new();
    for entry in parsed.accepted {
        if entry.id.trim().is_empty() {
            tracing::warn!(
                "supply-chain-accepted-advisories.toml: entry with empty `id` — skipping"
            );
            continue;
        }
        let note = entry.note.as_deref().map(str::trim).unwrap_or("");
        if note.is_empty() {
            tracing::warn!(
                "supply-chain-accepted-advisories.toml: entry `{}` has no `note` — \
                 skipping (operator rationale is required; see module docs)",
                entry.id
            );
            continue;
        }
        if let Some(exp) = entry.expires_at.as_deref() {
            match parse_expires_at(exp) {
                Ok(exp_date) if exp_date < now_date => {
                    tracing::warn!(
                        "supply-chain-accepted-advisories.toml: entry `{}` expired on {} \
                         — skipping (operator must re-review)",
                        entry.id,
                        exp_date
                    );
                    continue;
                }
                Ok(_) => {} // still valid
                Err(e) => {
                    tracing::warn!(
                        "supply-chain-accepted-advisories.toml: entry `{}` has \
                         unparseable expires_at `{}` ({:#}); treating as \
                         non-expiring",
                        entry.id,
                        exp,
                        e
                    );
                }
            }
        }
        out.insert(entry.id);
    }
    Ok(out)
}

/// Parse an `expires_at` string. Accepts either `YYYY-MM-DD` or a
/// full RFC 3339 timestamp (we only consume the date portion).
fn parse_expires_at(raw: &str) -> Result<NaiveDate> {
    let trimmed = raw.trim();
    // Try simple date first.
    if let Ok(d) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Ok(d);
    }
    // Then try full RFC 3339 (with time + timezone).
    let dt = DateTime::parse_from_rfc3339(trimmed)?;
    Ok(dt.date_naive())
}

#[derive(Debug, Deserialize, Default)]
struct AcceptedFile {
    #[serde(default)]
    accepted: Vec<AcceptedEntry>,
}

#[derive(Debug, Deserialize)]
struct AcceptedEntry {
    id: String,
    #[serde(default)]
    #[allow(dead_code)] // reserved for v2 (ledger integration); useful for humans now
    package: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn now() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 4, 24).unwrap()
    }

    #[test]
    fn empty_file_returns_empty_set() {
        let set = parse_accepted_toml("", now()).unwrap();
        assert!(set.is_empty());
    }

    #[test]
    fn one_valid_entry_is_accepted() {
        let raw = r#"
            [[accepted]]
            id = "RUSTSEC-2024-0436"
            package = "paste"
            note = "Unmaintained-notice; transitive via rmcp."
        "#;
        let set = parse_accepted_toml(raw, now()).unwrap();
        assert_eq!(set.len(), 1);
        assert!(set.contains("RUSTSEC-2024-0436"));
    }

    #[test]
    fn entry_without_note_is_skipped() {
        // No `note` key at all.
        let raw = r#"
            [[accepted]]
            id = "RUSTSEC-2024-0436"
        "#;
        let set = parse_accepted_toml(raw, now()).unwrap();
        assert!(set.is_empty(), "missing note must skip the entry");
    }

    #[test]
    fn entry_with_empty_note_is_skipped() {
        let raw = r#"
            [[accepted]]
            id = "RUSTSEC-2024-0436"
            note = "   "
        "#;
        let set = parse_accepted_toml(raw, now()).unwrap();
        assert!(set.is_empty(), "whitespace-only note must skip the entry");
    }

    #[test]
    fn entry_with_empty_id_is_skipped() {
        let raw = r#"
            [[accepted]]
            id = ""
            note = "something"
        "#;
        let set = parse_accepted_toml(raw, now()).unwrap();
        assert!(set.is_empty());
    }

    #[test]
    fn future_expires_at_is_accepted() {
        let raw = r#"
            [[accepted]]
            id = "RUSTSEC-X"
            note = "under review"
            expires_at = "2026-10-24"
        "#;
        let set = parse_accepted_toml(raw, now()).unwrap();
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn past_expires_at_is_skipped() {
        let raw = r#"
            [[accepted]]
            id = "RUSTSEC-X"
            note = "stale acceptance"
            expires_at = "2026-01-01"
        "#;
        let set = parse_accepted_toml(raw, now()).unwrap();
        assert!(set.is_empty(), "past expiration must drop the acceptance");
    }

    #[test]
    fn today_expires_at_is_accepted() {
        // An acceptance that expires "today" is still valid today.
        let raw = r#"
            [[accepted]]
            id = "RUSTSEC-X"
            note = "expires today"
            expires_at = "2026-04-24"
        "#;
        let set = parse_accepted_toml(raw, now()).unwrap();
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn rfc3339_expires_at_is_supported() {
        let raw = r#"
            [[accepted]]
            id = "RUSTSEC-X"
            note = "RFC3339 expiry"
            expires_at = "2026-10-24T12:00:00Z"
        "#;
        let set = parse_accepted_toml(raw, now()).unwrap();
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn unparseable_expires_at_treated_as_non_expiring() {
        // Defensive: don't silently drop an acceptance just because
        // the expiration date is malformed. Log a warning + keep the
        // acceptance. Operator can fix the date on next triage.
        let raw = r#"
            [[accepted]]
            id = "RUSTSEC-X"
            note = "garbage expiry"
            expires_at = "not-a-date"
        "#;
        let set = parse_accepted_toml(raw, now()).unwrap();
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn malformed_toml_returns_empty_not_error() {
        // Acceptance file being busted must not fail the scan — we
        // fall through to "all advisories unaccepted" which is the
        // conservative default.
        let raw = "this is = = = broken toml";
        let set = parse_accepted_toml(raw, now()).unwrap();
        assert!(set.is_empty());
    }

    #[test]
    fn multiple_entries_deduped_by_id() {
        let raw = r#"
            [[accepted]]
            id = "RUSTSEC-2024-0436"
            note = "first mention"

            [[accepted]]
            id = "RUSTSEC-2024-0436"
            note = "duplicate of above"

            [[accepted]]
            id = "RUSTSEC-X"
            note = "distinct"
        "#;
        let set = parse_accepted_toml(raw, now()).unwrap();
        assert_eq!(set.len(), 2);
        assert!(set.contains("RUSTSEC-2024-0436"));
        assert!(set.contains("RUSTSEC-X"));
    }

    // --- read() end-to-end ---

    #[test]
    fn read_returns_empty_when_file_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let set = read(tmp.path()).unwrap();
        assert!(set.is_empty());
    }

    #[test]
    fn read_happy_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let claude = tmp.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join("supply-chain-accepted-advisories.toml"),
            r#"
                [[accepted]]
                id = "RUSTSEC-2024-0436"
                note = "Unmaintained-notice; transitive via rmcp."
            "#,
        )
        .unwrap();
        let set = read(tmp.path()).unwrap();
        assert_eq!(set.len(), 1);
        assert!(set.contains("RUSTSEC-2024-0436"));
    }
}
