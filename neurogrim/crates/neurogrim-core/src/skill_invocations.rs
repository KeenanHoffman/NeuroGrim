//! v4.4 dogfooding — `_neurogrim/skill-invocations` SQLite bus topic
//! as a materialized view of the canonical `invocation-ledger.jsonl`.
//!
//! ## Why a hybrid pattern?
//!
//! The PostToolUse shell hook (`scripts/record-skill-invocation.sh`)
//! is the high-frequency writer — ~1-10 invocations per minute during
//! active sessions. Bash can append to JSONL trivially but cannot
//! write SQLite without spawning the `neurogrim` binary on every
//! invocation (~100ms cold-start cost — unacceptable).
//!
//! So the JSONL stays as the canonical source-of-truth, and SQLite
//! is treated as a derived index that the **readers** lazily catch
//! up on demand. This preserves:
//!
//! - **Shell-hook simplicity** (no change to `record-skill-invocation.sh`)
//! - **`cat`-inspectability** of the ledger (operator-visibility methodology)
//! - **Indexed bounded queries** (the dashboard + capability-hygiene
//!   sensor get O(window) reads instead of O(total ledger))
//! - **Bus-topic shape** (external subscribers can read via the
//!   existing `/api/brains/:id/queues/_neurogrim/skill-invocations`
//!   endpoint)
//!
//! ## Watermark: row count
//!
//! The ingest helper compares JSONL non-empty-line count vs SQLite
//! row count. If JSONL has more, it appends the diff. This is
//! correct under the invariant that JSONL is append-only: existing
//! lines never move or change. Malformed lines that fail JSON parsing
//! are silently re-attempted on every read (cheap; no data loss).
//!
//! If `backend.append()` fails mid-loop (transient SQLite error),
//! the loop aborts so SQLite stays consistent at its current row
//! count and the un-ingested lines retry on the next call.
//!
//! ## Cost
//!
//! Reading JSONL once per call: O(file_size). For typical projects
//! (~10k entries × ~200 bytes = 2 MB), this is <10ms. Acceptable for
//! dashboard request rates and per-score-run sensor invocations.
//! A v2 watermark optimization (mtime / size sidecar) is out of
//! scope for this migration.

use crate::queue::{QueueMessage, SKILL_INVOCATIONS_TOPIC};
use crate::queue_backend::{QueueBackend, SqliteBackend};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Path to the SQLite-backed bus topic for skill invocations.
pub fn topic_sqlite_path(project_root: &Path) -> PathBuf {
    project_root
        .join(".claude")
        .join("brain")
        .join("queues")
        .join("_neurogrim")
        .join("skill-invocations.sqlite")
}

/// Path to the canonical JSONL ledger that the shell hook writes to.
pub fn jsonl_path(project_root: &Path) -> PathBuf {
    project_root
        .join(".claude")
        .join("brain")
        .join("invocation-ledger.jsonl")
}

/// Open the SQLite-backed `_neurogrim/skill-invocations` bus topic,
/// catching it up with any new lines from the canonical
/// `invocation-ledger.jsonl` first.
///
/// Returns the opened backend ready for queries (`.read_from`,
/// `.len`, etc.). Idempotent — calling repeatedly with no new JSONL
/// activity is a no-op (just opens the SQLite + counts rows).
///
/// On first call for a project (no SQLite file yet), the parent
/// directory is created and the entire JSONL is ingested.
///
/// JSONL missing → returns the (empty) SQLite backend without error.
pub fn ingest_and_open(project_root: &Path) -> Result<SqliteBackend> {
    let sqlite = topic_sqlite_path(project_root);
    if let Some(parent) = sqlite.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut backend = SqliteBackend::open(&sqlite)?;

    let jsonl = jsonl_path(project_root);
    let text = match std::fs::read_to_string(&jsonl) {
        Ok(t) => t,
        Err(_) => return Ok(backend), // no JSONL yet → nothing to sync
    };

    let already_ingested = backend.len()? as usize;
    let mut seen_non_empty = 0usize;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        seen_non_empty += 1;
        if seen_non_empty <= already_ingested {
            // Already in SQLite (or was a malformed line we skipped
            // last time and will re-skip below). Skip.
            continue;
        }
        let parsed: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue, // malformed → tolerate, skip
        };
        let msg = QueueMessage::new(SKILL_INVOCATIONS_TOPIC, parsed);
        if let Err(e) = backend.append(&msg) {
            // Transient DB error — abort to keep SQLite consistent.
            // Next call retries from the current SQLite row count.
            tracing::warn!(
                "skill-invocations ingest: append failed at non-empty line {seen_non_empty}: {e}"
            );
            break;
        }
    }
    Ok(backend)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_jsonl(root: &Path, lines: &[&str]) {
        let path = jsonl_path(root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    }

    fn entry(ts: &str, name: &str) -> String {
        format!(
            r#"{{"schema_version":"2","ts":"{ts}","type":"skill","subtype":"hard","name":"{name}","session_id":"s1","invocation_id":"i1"}}"#
        )
    }

    #[test]
    fn missing_jsonl_returns_empty_backend() {
        let tmp = TempDir::new().unwrap();
        let backend = ingest_and_open(tmp.path()).unwrap();
        assert_eq!(backend.len().unwrap(), 0);
    }

    #[test]
    fn first_ingest_copies_all_lines() {
        let tmp = TempDir::new().unwrap();
        write_jsonl(
            tmp.path(),
            &[
                &entry("2026-04-29T10:00:00Z", "alpha"),
                &entry("2026-04-29T10:01:00Z", "beta"),
                &entry("2026-04-29T10:02:00Z", "gamma"),
            ],
        );
        let backend = ingest_and_open(tmp.path()).unwrap();
        assert_eq!(backend.len().unwrap(), 3);
    }

    #[test]
    fn second_ingest_only_appends_new_lines() {
        let tmp = TempDir::new().unwrap();
        write_jsonl(tmp.path(), &[&entry("2026-04-29T10:00:00Z", "alpha")]);
        {
            let b = ingest_and_open(tmp.path()).unwrap();
            assert_eq!(b.len().unwrap(), 1);
        }
        // Append two more lines to the JSONL
        write_jsonl(
            tmp.path(),
            &[
                &entry("2026-04-29T10:00:00Z", "alpha"),
                &entry("2026-04-29T10:01:00Z", "beta"),
                &entry("2026-04-29T10:02:00Z", "gamma"),
            ],
        );
        let backend = ingest_and_open(tmp.path()).unwrap();
        assert_eq!(backend.len().unwrap(), 3);
    }

    #[test]
    fn idempotent_with_no_new_data() {
        let tmp = TempDir::new().unwrap();
        write_jsonl(
            tmp.path(),
            &[
                &entry("2026-04-29T10:00:00Z", "alpha"),
                &entry("2026-04-29T10:01:00Z", "beta"),
            ],
        );
        let n1 = ingest_and_open(tmp.path()).unwrap().len().unwrap();
        let n2 = ingest_and_open(tmp.path()).unwrap().len().unwrap();
        let n3 = ingest_and_open(tmp.path()).unwrap().len().unwrap();
        assert_eq!(n1, 2);
        assert_eq!(n2, 2);
        assert_eq!(n3, 2);
    }

    #[test]
    fn malformed_lines_are_skipped_not_counted() {
        let tmp = TempDir::new().unwrap();
        write_jsonl(
            tmp.path(),
            &[
                &entry("2026-04-29T10:00:00Z", "alpha"),
                "not json",
                &entry("2026-04-29T10:01:00Z", "beta"),
            ],
        );
        let backend = ingest_and_open(tmp.path()).unwrap();
        assert_eq!(backend.len().unwrap(), 2);
    }

    #[test]
    fn empty_lines_are_ignored() {
        let tmp = TempDir::new().unwrap();
        write_jsonl(
            tmp.path(),
            &[
                &entry("2026-04-29T10:00:00Z", "alpha"),
                "",
                "   ",
                &entry("2026-04-29T10:01:00Z", "beta"),
            ],
        );
        let backend = ingest_and_open(tmp.path()).unwrap();
        assert_eq!(backend.len().unwrap(), 2);
    }
}
