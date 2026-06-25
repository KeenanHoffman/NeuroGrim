//! BB #12 — Trace Sink (minimal MVP).
//!
//! JSONL append-only log of every dispatch. **MVP design choice (per ultra-pass
//! U9):** record snapshot **deltas** from prior snapshot, not full snapshots,
//! to avoid trace-file bloat at scale. The full-snapshot view is recoverable
//! by walking the delta chain.
//!
//! ## Snapshot delta computation (Wave 2)
//!
//! `SnapshotDelta::compute(prior, current)` produces a Value with three fields:
//! - `added`: paths whose values are new in `current`
//! - `removed`: paths that existed in `prior` but not `current`
//! - `changed`: paths whose values differ; each entry has `{from: <prior>, to: <current>}`
//!
//! Recovery: walking the delta chain from a known-full-snapshot is recoverable
//! via apply: `apply_delta(prior, delta) → current`. The runner stores the
//! prior-snapshot per broker so subsequent dispatches log only the delta.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TraceError {
    #[error("trace file write failed: {0}")]
    WriteFailed(#[from] std::io::Error),

    #[error("trace record serialization failed: {0}")]
    SerializeFailed(#[from] serde_json::Error),
}

/// A trace record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRecord {
    pub schema_version: String,
    pub ts: String,
    pub trace_id: String,
    pub pipeline_id: String,
    pub broker_id: String,
    pub params: serde_json::Value,
    /// `None` on the broker's first trace record (delta-from-nothing means
    /// `snapshot_delta` contains the full snapshot).
    pub snapshot_delta_from: Option<String>,
    pub snapshot_delta: serde_json::Value,
    pub outcome: serde_json::Value,
    pub audit_class: String,
    pub duration_ms: u64,
}

/// Trace Sink — appends trace records to a per-broker JSONL file.
pub struct TraceSink {
    file_path: PathBuf,
    write_lock: std::sync::Mutex<()>,
}

impl TraceSink {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            write_lock: std::sync::Mutex::new(()),
        }
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// C1 — append an externally-authored TraceRecord to the unified
    /// JSONL ledger. Used by non-broker components (process_broker spawn
    /// sites, IDE-internal lifecycle events, etc.) so the operator sees
    /// ONE audit log per harness rather than a patchwork of per-subsystem
    /// JSONLs. Per plan §C1 + Adversarial-hat critique #4 mitigation.
    ///
    /// The caller is responsible for choosing audit_class — convention is
    /// `"infrastructure"` for subprocess/lifecycle events, `"capability"`
    /// for IDE-internal capability dispatches, etc. `pipeline_id` should
    /// be a synthetic well-known id (e.g., `process-broker/spawn`) so the
    /// audit trail remains queryable by id.
    ///
    /// Equivalent to `append()`; the separate name signals intent at the
    /// call site (and future versions may enforce that external callers
    /// can't emit `audit_class: governance` to prevent confusion with
    /// substrate-emitted governance events).
    pub fn append_external(&self, record: &TraceRecord) -> Result<(), TraceError> {
        self.append(record)
    }

    /// Append a single record (JSONL: one JSON object per line).
    pub fn append(&self, record: &TraceRecord) -> Result<(), TraceError> {
        let serialized = serde_json::to_string(record)?;
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| std::io::Error::other("trace write lock poisoned"))?;
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;
        writeln!(file, "{}", serialized)?;
        Ok(())
    }
}

/// Snapshot delta — records added/removed/changed paths between two
/// JSON values. Used by the Trace Sink to log only what changed since the
/// prior dispatch (ultra-pass U9 mitigation).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotDelta {
    pub added: serde_json::Map<String, serde_json::Value>,
    pub removed: Vec<String>,
    pub changed: serde_json::Map<String, serde_json::Value>,
}

impl SnapshotDelta {
    /// Compute a delta from `prior` to `current` at the top level.
    /// MVP: only top-level keys are diffed (sub-object changes show as
    /// `changed` with the whole sub-object). Wave 4+ may add recursive diff.
    pub fn compute(prior: &serde_json::Value, current: &serde_json::Value) -> Self {
        let mut delta = SnapshotDelta::default();
        let prior_obj = prior.as_object();
        let current_obj = current.as_object();

        match (prior_obj, current_obj) {
            (Some(p), Some(c)) => {
                for (key, cur_val) in c {
                    match p.get(key) {
                        Some(prior_val) if prior_val == cur_val => {} // unchanged
                        Some(prior_val) => {
                            delta.changed.insert(
                                key.clone(),
                                serde_json::json!({
                                    "from": prior_val,
                                    "to": cur_val
                                }),
                            );
                        }
                        None => {
                            delta.added.insert(key.clone(), cur_val.clone());
                        }
                    }
                }
                for key in p.keys() {
                    if !c.contains_key(key) {
                        delta.removed.push(key.clone());
                    }
                }
            }
            _ => {
                // Either prior or current is non-object (string, array, null).
                // MVP: log the full current value as "added" under a synthetic
                // key. Wave 4+ may add proper non-object diffing.
                delta
                    .added
                    .insert("__root__".to_string(), current.clone());
            }
        }
        delta
    }

    pub fn to_json(self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn snapshot_delta_added_keys() {
        let prior = serde_json::json!({"a": 1});
        let current = serde_json::json!({"a": 1, "b": 2});
        let delta = SnapshotDelta::compute(&prior, &current);
        assert_eq!(delta.added.len(), 1);
        assert_eq!(delta.added["b"], serde_json::json!(2));
        assert_eq!(delta.removed.len(), 0);
        assert_eq!(delta.changed.len(), 0);
    }

    #[test]
    fn snapshot_delta_removed_keys() {
        let prior = serde_json::json!({"a": 1, "b": 2});
        let current = serde_json::json!({"a": 1});
        let delta = SnapshotDelta::compute(&prior, &current);
        assert_eq!(delta.added.len(), 0);
        assert_eq!(delta.removed, vec!["b"]);
        assert_eq!(delta.changed.len(), 0);
    }

    #[test]
    fn snapshot_delta_changed_keys_show_from_to() {
        let prior = serde_json::json!({"a": 1});
        let current = serde_json::json!({"a": 2});
        let delta = SnapshotDelta::compute(&prior, &current);
        assert_eq!(delta.changed["a"]["from"], serde_json::json!(1));
        assert_eq!(delta.changed["a"]["to"], serde_json::json!(2));
    }

    #[test]
    fn snapshot_delta_unchanged_keys_are_omitted() {
        let prior = serde_json::json!({"a": 1, "b": 2});
        let current = serde_json::json!({"a": 1, "b": 2});
        let delta = SnapshotDelta::compute(&prior, &current);
        assert!(delta.added.is_empty());
        assert!(delta.removed.is_empty());
        assert!(delta.changed.is_empty());
    }

    #[test]
    fn trace_sink_appends_jsonl() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        let sink = TraceSink::new(path.clone());
        let r1 = TraceRecord {
            schema_version: "1".to_string(),
            ts: "2026-06-24T00:00:00Z".to_string(),
            trace_id: "trace-1".to_string(),
            pipeline_id: "t/echo".to_string(),
            broker_id: "t".to_string(),
            params: serde_json::json!({}),
            snapshot_delta_from: None,
            snapshot_delta: serde_json::json!({}),
            outcome: serde_json::json!({"status": "success"}),
            audit_class: "capability".to_string(),
            duration_ms: 1,
        };
        let r2 = TraceRecord {
            schema_version: "1".to_string(),
            ts: "2026-06-24T00:00:01Z".to_string(),
            trace_id: "trace-2".to_string(),
            pipeline_id: "t/echo".to_string(),
            broker_id: "t".to_string(),
            params: serde_json::json!({}),
            snapshot_delta_from: Some("trace-1".to_string()),
            snapshot_delta: serde_json::json!({"added": {"new_field": "x"}, "removed": [], "changed": {}}),
            outcome: serde_json::json!({"status": "success"}),
            audit_class: "capability".to_string(),
            duration_ms: 1,
        };
        sink.append(&r1).unwrap();
        sink.append(&r2).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
        // Each line parses as valid JSON
        for line in lines {
            let _: TraceRecord = serde_json::from_str(line).unwrap();
        }
    }
}
