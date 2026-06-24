//! BB #6 — Cold Store backend implementation (gap #11 closure).
//!
//! V0-RETROSPECTIVE §C3 / gap #11: the R-O-4 per-broker file-isolation
//! claim was unvalidated through Wave 5. This module lands the JSONL
//! backend + tests that exercise per-broker file isolation across
//! concurrent broker writes.
//!
//! ## Backend choices (per BB #6 row in BROKER-INTERNALS.md)
//!
//! - **JSONL append-only** (this module): high-tick-rate friendly; no lock
//!   contention; queries require scanning. Default for V0+ workloads.
//! - **SQLite** (deferred to S1-T): transactional integrity; per-broker file
//!   isolation still required (R-O-4); slower under high write throughput.
//!
//! ## Per-broker file isolation (R-O-4 enforcement)
//!
//! Every `ColdStore` is constructed with a `broker_id`; the storage path
//! includes that ID. Different brokers cannot share a storage file. This
//! is enforced at construction time, not at runtime — the type system
//! prevents the misuse pattern that motivated R-O-4.

use serde::{de::DeserializeOwned, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ColdStoreError {
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("serialization failed: {0}")]
    SerializationFailed(#[from] serde_json::Error),

    #[error("lock poisoned: {0}")]
    LockPoisoned(String),
}

/// Cold Store trait — backends implement this to provide durable append-only
/// storage per broker. MVP API is intentionally minimal (append + read all).
/// Wave 5.5+ may add query / index / compact / transaction methods.
pub trait ColdStore: Send + Sync {
    /// Append a record. Returns the offset assigned (monotonically increasing).
    fn append(&self, record: &str) -> Result<u64, ColdStoreError>;

    /// Read all records since `offset`. Returns (offset_of_next_unread, records).
    fn read_from(&self, offset: u64) -> Result<(u64, Vec<String>), ColdStoreError>;

    /// Current record count (for diagnostics).
    fn len(&self) -> Result<u64, ColdStoreError>;
}

/// JSONL append-only backend with per-broker file isolation.
///
/// Storage layout: each broker gets a file at `<base_dir>/<broker_id>.jsonl`.
/// Constructor enforces R-O-4: each instance has its own file path; no
/// instance can write to another broker's file.
pub struct JsonlColdStore {
    file_path: PathBuf,
    /// Append serialization (intra-broker writes are serialized; inter-broker
    /// writes are isolated by file-path uniqueness).
    write_lock: Mutex<()>,
}

impl JsonlColdStore {
    /// Create a new JSONL backend for the given broker. Per-broker file
    /// isolation is enforced by the file path containing the broker_id.
    pub fn new(base_dir: &Path, broker_id: &str) -> Result<Self, ColdStoreError> {
        std::fs::create_dir_all(base_dir)?;
        let file_path = base_dir.join(format!("{}.jsonl", broker_id));
        // Touch the file so it exists for subsequent reads
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?;
        Ok(Self {
            file_path,
            write_lock: Mutex::new(()),
        })
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// Helper: append a serializable record (encodes to JSON; appends line).
    pub fn append_value<T: Serialize>(&self, value: &T) -> Result<u64, ColdStoreError> {
        let serialized = serde_json::to_string(value)?;
        self.append(&serialized)
    }

    /// Helper: read + deserialize all records since `offset`.
    pub fn read_value_from<T: DeserializeOwned>(
        &self,
        offset: u64,
    ) -> Result<(u64, Vec<T>), ColdStoreError> {
        let (next, lines) = self.read_from(offset)?;
        let values: Vec<T> = lines
            .into_iter()
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str(&line))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((next, values))
    }
}

impl ColdStore for JsonlColdStore {
    fn append(&self, record: &str) -> Result<u64, ColdStoreError> {
        use std::io::Write;
        let _guard = self
            .write_lock
            .lock()
            .map_err(|e| ColdStoreError::LockPoisoned(e.to_string()))?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)?;
        writeln!(file, "{}", record)?;
        // Offset = current file size in bytes (simpler than tracking
        // record-count; consumers can also use record count via len()).
        Ok(file.metadata()?.len())
    }

    fn read_from(&self, offset: u64) -> Result<(u64, Vec<String>), ColdStoreError> {
        use std::io::{BufRead, BufReader, Seek, SeekFrom};
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .open(&self.file_path)?;
        let end = file.metadata()?.len();
        if offset >= end {
            return Ok((end, Vec::new()));
        }
        file.seek(SeekFrom::Start(offset))?;
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader
            .lines()
            .collect::<Result<Vec<_>, _>>()
            .map_err(ColdStoreError::IoError)?;
        Ok((end, lines))
    }

    fn len(&self) -> Result<u64, ColdStoreError> {
        let (_, lines) = self.read_from(0)?;
        Ok(lines.iter().filter(|l| !l.is_empty()).count() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[test]
    fn jsonl_per_broker_file_isolation_enforced_by_path() {
        let tmp = TempDir::new().unwrap();
        let store_a = JsonlColdStore::new(tmp.path(), "broker-a").unwrap();
        let store_b = JsonlColdStore::new(tmp.path(), "broker-b").unwrap();
        // R-O-4 enforcement: different file paths
        assert_ne!(store_a.file_path(), store_b.file_path());
        assert!(store_a.file_path().to_str().unwrap().contains("broker-a"));
        assert!(store_b.file_path().to_str().unwrap().contains("broker-b"));
    }

    #[test]
    fn jsonl_append_then_read_roundtrips() {
        let tmp = TempDir::new().unwrap();
        let store = JsonlColdStore::new(tmp.path(), "test").unwrap();
        store.append(r#"{"event": "first"}"#).unwrap();
        store.append(r#"{"event": "second"}"#).unwrap();
        let (next, records) = store.read_from(0).unwrap();
        let records: Vec<_> = records.into_iter().filter(|r| !r.is_empty()).collect();
        assert_eq!(records.len(), 2);
        assert!(records[0].contains("first"));
        assert!(records[1].contains("second"));
        assert!(next > 0);
    }

    #[test]
    fn jsonl_read_from_offset_returns_only_new_records() {
        let tmp = TempDir::new().unwrap();
        let store = JsonlColdStore::new(tmp.path(), "test").unwrap();
        store.append(r#"{"old": 1}"#).unwrap();
        let (offset_after_first, _) = store.read_from(0).unwrap();
        store.append(r#"{"new": 1}"#).unwrap();
        let (_, records) = store.read_from(offset_after_first).unwrap();
        let records: Vec<_> = records.into_iter().filter(|r| !r.is_empty()).collect();
        assert_eq!(records.len(), 1);
        assert!(records[0].contains("new"));
    }

    #[test]
    fn jsonl_concurrent_writes_to_same_broker_serialize_via_lock() {
        let tmp = TempDir::new().unwrap();
        let store = Arc::new(JsonlColdStore::new(tmp.path(), "concurrent").unwrap());
        let mut handles = Vec::new();
        for i in 0..50 {
            let s = store.clone();
            handles.push(std::thread::spawn(move || {
                s.append(&format!(r#"{{"i": {}}}"#, i)).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let count = store.len().unwrap();
        assert_eq!(count, 50);
    }

    #[test]
    fn jsonl_concurrent_writes_across_brokers_dont_interfere() {
        // R-O-4 validation: 10 concurrent broker writers, each with their own
        // file; verify no cross-contamination.
        let tmp = TempDir::new().unwrap();
        let mut handles = Vec::new();
        for b in 0..10 {
            let dir = tmp.path().to_path_buf();
            handles.push(std::thread::spawn(move || {
                let store = JsonlColdStore::new(&dir, &format!("broker-{}", b)).unwrap();
                for i in 0..20 {
                    store
                        .append(&format!(r#"{{"broker": {}, "i": {}}}"#, b, i))
                        .unwrap();
                }
                b
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // Verify each broker's file has exactly 20 records + no cross-contamination
        for b in 0..10 {
            let store = JsonlColdStore::new(tmp.path(), &format!("broker-{}", b)).unwrap();
            let (_, records) = store.read_from(0).unwrap();
            let records: Vec<_> = records.into_iter().filter(|r| !r.is_empty()).collect();
            assert_eq!(records.len(), 20, "broker-{} should have 20 records", b);
            // Every record in this broker's file should contain its own broker id
            let expected = format!(r#""broker": {}"#, b);
            for r in &records {
                assert!(
                    r.contains(&expected),
                    "broker-{} contains foreign record: {}",
                    b,
                    r
                );
            }
        }
    }

    #[test]
    fn jsonl_typed_helpers_work() {
        use serde::{Deserialize, Serialize};
        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct Record {
            id: u64,
            name: String,
        }

        let tmp = TempDir::new().unwrap();
        let store = JsonlColdStore::new(tmp.path(), "typed").unwrap();
        store
            .append_value(&Record {
                id: 1,
                name: "alpha".into(),
            })
            .unwrap();
        store
            .append_value(&Record {
                id: 2,
                name: "beta".into(),
            })
            .unwrap();
        let (_, recs): (_, Vec<Record>) = store.read_value_from(0).unwrap();
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].id, 1);
        assert_eq!(recs[1].name, "beta");
    }
}
