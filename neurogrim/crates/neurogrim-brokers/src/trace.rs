//! BB #12 — Trace Sink (minimal MVP).
//!
//! JSONL append-only log of every dispatch. **MVP design choice (per ultra-pass
//! U9):** record snapshot **deltas** from prior snapshot, not full snapshots,
//! to avoid trace-file bloat at scale. The full-snapshot view is recoverable
//! by walking the delta chain.
//!
//! ## Wave 0 scaffold; Wave 2 implements.
//!
//! Trace record schema (Wave 2 finalizes):
//!
//! ```ignore
//! {
//!   "schema_version": "1",
//!   "ts": "<ISO 8601 UTC>",
//!   "trace_id": "<UUID>",
//!   "pipeline_id": "<broker_id>/<pipeline_name>",
//!   "params": {...},
//!   "snapshot_delta_from": "<prior trace_id | null>",
//!   "snapshot_delta": {...},
//!   "outcome": { "status": "success | refused | failed", ... },
//!   "audit_class": "capability | governance | meta-observation",
//!   "duration_ms": 42
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TraceError {
    #[error("trace file write failed: {0}")]
    WriteFailed(#[from] std::io::Error),

    #[error("trace record serialization failed: {0}")]
    SerializeFailed(#[from] serde_json::Error),
}

/// A trace record. Wave 2 finalizes the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRecord {
    pub schema_version: String,
    pub ts: String,
    pub trace_id: String,
    pub pipeline_id: String,
    pub params: serde_json::Value,
    pub snapshot_delta_from: Option<String>,
    pub snapshot_delta: serde_json::Value,
    pub outcome: serde_json::Value,
    pub audit_class: String,
    pub duration_ms: u64,
}

/// Trace Sink — appends trace records to a per-broker JSONL file.
/// Wave 2 implements.
pub struct TraceSink {
    _file_path: PathBuf,
}

impl TraceSink {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            _file_path: file_path,
        }
    }

    pub fn append(&self, _record: &TraceRecord) -> Result<(), TraceError> {
        Err(TraceError::WriteFailed(std::io::Error::other(
            "trace::append not yet implemented (Wave 2)",
        )))
    }
}
