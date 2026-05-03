//! v4.1 S13-B-3 — pluggable queue persistence backends.
//!
//! The bus has two storage shapes that share a single primitive
//! concept (append-only ordered messages) but earn-keep at different
//! workloads:
//!
//! 1. **JSONL** (default) — `cat`-inspectable, simple, fan-out only.
//!    No ack semantics; consumers track their own offset. Optimal
//!    for low-volume topics + the "everything inspectable as files"
//!    methodology principle.
//!
//! 2. **SQLite** (opt-in via `queue-config.yaml`) — transactional,
//!    supports `ack_required: true` consumer-group semantics.
//!    Earns its keep when adopters need exactly-once consumption
//!    coordinated across multiple readers (e.g., the approvals
//!    queue feeding work-stealing approval handlers).
//!
//! Both backends implement the same [`QueueBackend`] trait so the
//! dashboard's `bus.rs` and the MCP queue tools can dispatch
//! per-topic without knowing which backend is in play. The trait
//! lives in `neurogrim-core` so consumers across the workspace
//! can program against it.
//!
//! ## Feature gating
//!
//! `SqliteBackend` is gated behind the `sqlite` feature so callers
//! that don't need it (e.g., `neurogrim-mcp` running just the
//! Pattern-1 fan-out path) don't pay rusqlite's compile-time +
//! binary cost. The dashboard + CLI enable the feature.
//!
//! ## v1 scope (this stage)
//!
//! - Trait + 2 impls
//! - Per-consumer-group ack semantics for SQLite
//! - WAL-mode SQLite (mirrors `neurogrim-a2a`'s token-store)
//! - 8+ shared property-suite tests
//!
//! ## Deferred to v2 (follow-up session)
//!
//! - Wiring `bus.rs` to dispatch per-topic via `queue-config.yaml`
//!   (today's `bus.rs` always writes/reads JSONL directly)
//! - `neurogrim queue migrate <topic> jsonl sqlite` (and reverse) CLI
//! - `neurogrim queue inspect <topic>` reads from either backend
//! - MCP `queue_consume` tool understanding ack semantics

use crate::queue::{self, QueueMessage};
use anyhow::Result;
use std::path::PathBuf;

/// One stored message paired with its persistent offset. The
/// offset is monotonic and stable across restarts (JSONL: line
/// number; SQLite: ROWID).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    /// Persistent offset. Consumers may resume from `last_offset + 1`.
    pub offset: u64,
    pub message: QueueMessage,
}

/// Pluggable persistence backend for one bus topic.
///
/// All methods are `&self` (V5-MOD-3 Phase 2, 2026-05-02 — Fork A2):
/// backends that need interior mutability (e.g., `SqliteBackend`'s
/// `Connection`) wrap the state in `Mutex` internally. Promotion
/// to `&self` lets the registry hand out `Arc<dyn QueueBackend>`
/// directly; consumers don't pay the `Arc<Mutex<dyn>>` ceremony.
///
/// `Send + Sync` (V5-MOD-3 Fork A2) so backends can be shared across
/// the bus's tokio runtime without per-call lock-and-clone gymnastics.
/// Matches V5-MOD-1's `ScoringSource` and V5-MOD-2's `Sensor` for
/// SDK consistency.
///
/// Default impls of the ack-related methods return an error or
/// no-op so backends that don't support ack semantics (JsonlBackend)
/// don't need to override them.
pub trait QueueBackend: Send + Sync {
    /// Append a single message. Returns the assigned offset.
    fn append(&self, msg: &QueueMessage) -> Result<u64>;

    /// Read messages with offset >= `since_offset`, up to `limit`.
    /// Returns ascending-offset order.
    fn read_from(&self, since_offset: u64, limit: usize) -> Result<Vec<StoredMessage>>;

    /// Total parseable message count. Used by `queue stats`.
    fn len(&self) -> Result<u64>;

    /// True iff this backend supports `ack_required: true` semantics.
    /// JSONL returns false; SQLite returns true.
    fn supports_ack(&self) -> bool {
        false
    }

    /// Read messages not yet acked by `consumer_group`, up to `limit`.
    /// Default impl errors — only ack-capable backends override.
    fn read_unacked(
        &self,
        _consumer_group: &str,
        _limit: usize,
    ) -> Result<Vec<StoredMessage>> {
        anyhow::bail!(
            "queue backend: read_unacked() called on a backend that does \
             not support ack_required semantics — declare the topic with \
             `backend: sqlite` and `ack_required: true` in queue-config.yaml"
        )
    }

    /// Mark a message as acked by `consumer_group`.
    /// Default impl errors — only ack-capable backends override.
    fn ack(&self, _offset: u64, _consumer_group: &str) -> Result<()> {
        anyhow::bail!(
            "queue backend: ack() called on a backend that does not \
             support ack_required semantics"
        )
    }

    /// Highest acked offset for `consumer_group`, if any.
    /// Default impl returns None — non-ack backends have no ack state.
    fn last_acked(&self, _consumer_group: &str) -> Result<Option<u64>> {
        Ok(None)
    }
}

// ── JsonlBackend ────────────────────────────────────────────────────────

/// Default JSONL-backed implementation. Wraps `crate::queue::append`
/// + `JsonlQueueReader` so existing callers can migrate to the trait
/// API without changing on-disk format.
///
/// Ack methods are unsupported (the JSONL log is fan-out, not
/// transactional); `supports_ack()` returns false and the trait's
/// default impls take effect.
pub struct JsonlBackend {
    path: PathBuf,
}

impl JsonlBackend {
    /// Construct a backend reading/writing the topic file at `path`.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Path to the underlying JSONL file. Useful for `compact()` /
    /// `archive()` callers that need direct file access.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl QueueBackend for JsonlBackend {
    fn append(&self, msg: &QueueMessage) -> Result<u64> {
        // The JSONL writer doesn't natively report an offset, so
        // we count current messages and return that as the assigned
        // offset of the row we just appended. This mirrors the
        // semantics of `read_from(offset, ...)` returning rows at
        // their line-number offset.
        let pre_count = self.len()?;
        queue::append(&self.path, msg)?;
        Ok(pre_count)
    }

    fn read_from(&self, since_offset: u64, limit: usize) -> Result<Vec<StoredMessage>> {
        let reader = queue::JsonlQueueReader::open(&self.path)?;
        let since = since_offset as usize;
        let stored: Vec<StoredMessage> = reader
            .iter_from(since)
            .take(limit)
            .enumerate()
            .map(|(i, m)| StoredMessage {
                offset: (since + i) as u64,
                message: m.clone(),
            })
            .collect();
        Ok(stored)
    }

    fn len(&self) -> Result<u64> {
        let reader = queue::JsonlQueueReader::open(&self.path)?;
        Ok(reader.len() as u64)
    }
}

// ── SqliteBackend (feature-gated) ──────────────────────────────────────

#[cfg(feature = "sqlite")]
mod sqlite_impl {
    use super::*;
    use rusqlite::{params, Connection, OptionalExtension};
    use std::path::Path;

    /// SQLite-backed queue with consumer-group ack semantics.
    ///
    /// **Schema (v1):**
    ///
    /// ```sql
    /// CREATE TABLE messages (
    ///   offset      INTEGER PRIMARY KEY AUTOINCREMENT,
    ///   id          TEXT NOT NULL,         -- UUID v4
    ///   topic       TEXT NOT NULL,
    ///   payload     TEXT NOT NULL,         -- JSON
    ///   produced_at TEXT NOT NULL,         -- RFC3339
    ///   priority    TEXT NOT NULL,
    ///   expires_at  TEXT
    /// );
    /// CREATE TABLE acks (
    ///   consumer_group TEXT NOT NULL,
    ///   offset         INTEGER NOT NULL REFERENCES messages(offset),
    ///   acked_at       TEXT NOT NULL,
    ///   PRIMARY KEY (consumer_group, offset)
    /// );
    /// CREATE TABLE schema_version (version INTEGER NOT NULL);
    /// ```
    ///
    /// **WAL mode** is enabled at open — required for concurrent
    /// readers + writer on the same DB file. Mirrors the discipline
    /// in `neurogrim-a2a/src/token_store.rs`.
    ///
    /// **Concurrency note (V5-MOD-3 Phase 2 update, 2026-05-02):** the
    /// `Connection` is wrapped in a `std::sync::Mutex` *internally*
    /// so the type is `Send + Sync`. Callers no longer need to hold
    /// an external `Arc<Mutex<SqliteBackend>>`; an `Arc<dyn QueueBackend>`
    /// dispatches directly. WAL mode + a per-call `lock()` keep the
    /// concurrency model identical to v4.x: at most one in-process
    /// caller in the critical section per topic, with multi-process
    /// readers unblocked by WAL.
    pub struct SqliteBackend {
        conn: std::sync::Mutex<Connection>,
    }

    impl SqliteBackend {
        /// Open or create the SQLite database at `path`. Creates the
        /// schema if needed. Enables WAL mode + foreign keys.
        pub fn open(path: &Path) -> Result<Self> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let conn = Connection::open(path)?;
            // WAL mode: reader concurrency + crash-safe.
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "foreign_keys", "ON")?;
            // Synchronous=NORMAL pairs well with WAL — durable
            // enough for a coordination bus, faster than FULL.
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            Self::ensure_schema(&conn)?;
            Ok(Self {
                conn: std::sync::Mutex::new(conn),
            })
        }

        fn ensure_schema(conn: &Connection) -> Result<()> {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS messages (
                  offset      INTEGER PRIMARY KEY AUTOINCREMENT,
                  id          TEXT NOT NULL,
                  topic       TEXT NOT NULL,
                  payload     TEXT NOT NULL,
                  produced_at TEXT NOT NULL,
                  priority    TEXT NOT NULL,
                  expires_at  TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_messages_topic ON messages(topic);

                CREATE TABLE IF NOT EXISTS acks (
                  consumer_group TEXT NOT NULL,
                  offset         INTEGER NOT NULL REFERENCES messages(offset),
                  acked_at       TEXT NOT NULL,
                  PRIMARY KEY (consumer_group, offset)
                );

                CREATE TABLE IF NOT EXISTS schema_version (
                  version INTEGER PRIMARY KEY
                );
                INSERT OR IGNORE INTO schema_version (version) VALUES (1);
                "#,
            )?;
            Ok(())
        }

        fn row_to_stored(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredMessage> {
            let offset: i64 = row.get("offset")?;
            let id_str: String = row.get("id")?;
            let topic: String = row.get("topic")?;
            let payload_str: String = row.get("payload")?;
            let produced_at_str: String = row.get("produced_at")?;
            let priority_str: String = row.get("priority")?;
            let expires_at_str: Option<String> = row.get("expires_at")?;

            let id = uuid::Uuid::parse_str(&id_str)
                .map_err(|e| rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text))?;
            let payload: serde_json::Value = serde_json::from_str(&payload_str)
                .map_err(|e| rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text))?;
            let produced_at = chrono::DateTime::parse_from_rfc3339(&produced_at_str)
                .map_err(|e| rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&chrono::Utc);
            let priority = match priority_str.as_str() {
                "low" => crate::queue::Priority::Low,
                "normal" => crate::queue::Priority::Normal,
                "high" => crate::queue::Priority::High,
                other => {
                    return Err(rusqlite::Error::InvalidColumnType(
                        0,
                        format!("unknown priority {other:?}"),
                        rusqlite::types::Type::Text,
                    ));
                }
            };
            let expires_at = expires_at_str
                .map(|s| chrono::DateTime::parse_from_rfc3339(&s))
                .transpose()
                .map_err(|e| rusqlite::Error::InvalidColumnType(0, e.to_string(), rusqlite::types::Type::Text))?
                .map(|dt| dt.with_timezone(&chrono::Utc));

            Ok(StoredMessage {
                offset: offset as u64,
                message: QueueMessage {
                    id,
                    topic,
                    payload,
                    produced_at,
                    priority,
                    expires_at,
                },
            })
        }
    }

    impl QueueBackend for SqliteBackend {
        fn append(&self, msg: &QueueMessage) -> Result<u64> {
            let payload_str = serde_json::to_string(&msg.payload)?;
            let priority_str = match msg.priority {
                crate::queue::Priority::Low => "low",
                crate::queue::Priority::Normal => "normal",
                crate::queue::Priority::High => "high",
            };
            let expires_at_str = msg.expires_at.map(|d| d.to_rfc3339());
            let conn = self
                .conn
                .lock()
                .map_err(|_| anyhow::anyhow!("sqlite mutex poisoned"))?;
            conn.execute(
                r#"INSERT INTO messages
                       (id, topic, payload, produced_at, priority, expires_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
                params![
                    msg.id.to_string(),
                    msg.topic,
                    payload_str,
                    msg.produced_at.to_rfc3339(),
                    priority_str,
                    expires_at_str,
                ],
            )?;
            // SQLite returns the autoincremented ROWID (which equals
            // our `offset` PK) via `last_insert_rowid()`.
            Ok(conn.last_insert_rowid() as u64)
        }

        fn read_from(
            &self,
            since_offset: u64,
            limit: usize,
        ) -> Result<Vec<StoredMessage>> {
            let conn = self
                .conn
                .lock()
                .map_err(|_| anyhow::anyhow!("sqlite mutex poisoned"))?;
            let mut stmt = conn.prepare(
                r#"SELECT offset, id, topic, payload, produced_at, priority, expires_at
                   FROM messages
                   WHERE offset >= ?1
                   ORDER BY offset ASC
                   LIMIT ?2"#,
            )?;
            let iter = stmt.query_map(
                params![since_offset as i64, limit as i64],
                Self::row_to_stored,
            )?;
            let mut out = Vec::new();
            for r in iter {
                out.push(r?);
            }
            Ok(out)
        }

        fn len(&self) -> Result<u64> {
            let conn = self
                .conn
                .lock()
                .map_err(|_| anyhow::anyhow!("sqlite mutex poisoned"))?;
            let n: i64 =
                conn.query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))?;
            Ok(n as u64)
        }

        fn supports_ack(&self) -> bool {
            true
        }

        fn read_unacked(
            &self,
            consumer_group: &str,
            limit: usize,
        ) -> Result<Vec<StoredMessage>> {
            let conn = self
                .conn
                .lock()
                .map_err(|_| anyhow::anyhow!("sqlite mutex poisoned"))?;
            let mut stmt = conn.prepare(
                r#"SELECT m.offset, m.id, m.topic, m.payload, m.produced_at,
                          m.priority, m.expires_at
                   FROM messages m
                   WHERE NOT EXISTS (
                     SELECT 1 FROM acks a
                     WHERE a.consumer_group = ?1
                       AND a.offset = m.offset
                   )
                   ORDER BY m.offset ASC
                   LIMIT ?2"#,
            )?;
            let iter = stmt.query_map(
                params![consumer_group, limit as i64],
                Self::row_to_stored,
            )?;
            let mut out = Vec::new();
            for r in iter {
                out.push(r?);
            }
            Ok(out)
        }

        fn ack(&self, offset: u64, consumer_group: &str) -> Result<()> {
            let conn = self
                .conn
                .lock()
                .map_err(|_| anyhow::anyhow!("sqlite mutex poisoned"))?;
            // Verify the offset exists; if not, the caller is acking
            // a non-existent message — return a clear error rather
            // than silently inserting an orphan ack row.
            let exists: bool = conn
                .query_row(
                    "SELECT 1 FROM messages WHERE offset = ?1",
                    params![offset as i64],
                    |_| Ok(true),
                )
                .optional()?
                .unwrap_or(false);
            if !exists {
                anyhow::bail!(
                    "queue backend (sqlite): cannot ack offset {} — \
                     no such message",
                    offset
                );
            }
            // INSERT OR IGNORE — second ack from the same consumer
            // group is a no-op (idempotent).
            conn.execute(
                r#"INSERT OR IGNORE INTO acks (consumer_group, offset, acked_at)
                   VALUES (?1, ?2, ?3)"#,
                params![consumer_group, offset as i64, chrono::Utc::now().to_rfc3339()],
            )?;
            Ok(())
        }

        fn last_acked(&self, consumer_group: &str) -> Result<Option<u64>> {
            let conn = self
                .conn
                .lock()
                .map_err(|_| anyhow::anyhow!("sqlite mutex poisoned"))?;
            let row: Option<i64> = conn
                .query_row(
                    r#"SELECT MAX(offset) FROM acks WHERE consumer_group = ?1"#,
                    params![consumer_group],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            Ok(row.map(|n| n as u64))
        }
    }

    // V5-MOD-3 Phase 2 (2026-05-02) — `unsafe impl Send for SqliteBackend`
    // was needed pre-V5-MOD-3 because the struct held a raw `Connection`
    // (which is conditionally Send only on certain SQLite builds). With
    // `conn: Mutex<Connection>` after Fork A2, `Send + Sync` are
    // auto-derived; the unsafe impl is no longer needed and was removed.
}

#[cfg(feature = "sqlite")]
pub use sqlite_impl::SqliteBackend;

// ────────────────────────────────────────────────────────────────
// V5-MOD-3 Phase 1 (2026-05-02) — `QueueBackendFactory` +
// `QueueBackendRegistry` for the pluggable backend dispatch.
// ────────────────────────────────────────────────────────────────
//
// Replaces the closed-set `BackendKind` match in
// `neurogrim-dashboard/src/bus.rs::BackendHandle::*` (V5-MOD-3
// Phase 3 conversion target). Mirrors V5-MOD-1's `ScoringSource`
// + V5-MOD-2's `Sensor` factory + registry pattern: hand-rolled
// `HashMap<&str, Box<dyn Factory>>`, last-write-wins on duplicate
// register, `register_all` for the canonical built-in list.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Factory: produces a [`QueueBackend`] impl for one bus topic.
///
/// Each bus topic has its own backend instance (one `.jsonl` file
/// per topic, one `.sqlite` file per topic), so `build()` takes
/// the topic file path. The factory is responsible for any
/// per-topic state initialization (open SQLite connection,
/// validate file format, etc.).
///
/// `Send + Sync` so the registry can be shared across the bus's
/// tokio runtime without `Arc<Mutex>` ceremony — registration is
/// startup-time-only.
pub trait QueueBackendFactory: Send + Sync {
    /// Stable wire-name. Matches the `backend` string in
    /// `queue-config.yaml::topics::<topic>::backend`.
    fn name(&self) -> &'static str;

    /// True iff backends produced by this factory support
    /// `ack_required: true` semantics. `queue_config::validate()`
    /// (V5-MOD-3 Phase 3 update) checks this when a topic declares
    /// `ack_required: true` — declaring ack on a non-ack backend is
    /// a startup-time configuration error.
    ///
    /// Default: `false`. Backends that support ack semantics
    /// (e.g., `SqliteBackend`) override to return `true`.
    fn supports_ack(&self) -> bool {
        false
    }

    /// Construct a backend instance bound to `topic_path`. The
    /// path's existence is not assumed — the factory is responsible
    /// for creating the file/directory if needed. Returns
    /// `Arc<dyn QueueBackend>` so the bus's per-topic cache holds
    /// a thread-safe shared handle.
    fn build(&self, topic_path: &Path) -> Result<Arc<dyn QueueBackend>>;
}

/// Hand-rolled registry mapping backend wire-names to factories.
///
/// **Why hand-rolled** (V5-MOD-1 / V5-MOD-2 Subagent 2 finding,
/// reapplied to V5-MOD-3): the workspace has no existing
/// static-registration substrate (`inventory` / `linkme` / `ctor` —
/// none present). The `dependency-discipline` skill enforces a
/// 4-point pre-flight on new deps; this `HashMap`-backed registry
/// is the same ~40 lines with zero supply-chain review burden.
///
/// # Built-in registration
///
/// [`built_in_factories`] returns the JSONL + SQLite factories.
/// Consuming binaries call `registry.register_all(built_in_factories())`
/// at startup; third-party crates register their own via
/// [`Self::register`] alongside.
pub struct QueueBackendRegistry {
    factories: HashMap<&'static str, Box<dyn QueueBackendFactory>>,
}

impl QueueBackendRegistry {
    /// Empty registry. Caller registers factories explicitly.
    pub fn new() -> Self {
        QueueBackendRegistry {
            factories: HashMap::new(),
        }
    }

    /// Register a factory by its `name()`. Last-write-wins on
    /// duplicate name — consumers can override built-ins for
    /// testing.
    pub fn register(&mut self, factory: Box<dyn QueueBackendFactory>) {
        let name = factory.name();
        self.factories.insert(name, factory);
    }

    /// Convenience: register multiple factories from an iterator.
    /// V5-MOD-3 Phase 3 wire-up:
    /// `registry.register_all(built_in_factories())`.
    pub fn register_all(
        &mut self,
        factories: impl IntoIterator<Item = Box<dyn QueueBackendFactory>>,
    ) {
        for factory in factories {
            self.register(factory);
        }
    }

    /// Look up a factory by wire-name.
    pub fn get(&self, name: &str) -> Option<&dyn QueueBackendFactory> {
        self.factories.get(name).map(|f| f.as_ref())
    }

    /// Convenience: look up a factory and immediately build a
    /// backend for `topic_path`. Returns `None` if no factory is
    /// registered for the given name; otherwise propagates any
    /// `build()` error.
    pub fn build(
        &self,
        name: &str,
        topic_path: &Path,
    ) -> Option<Result<Arc<dyn QueueBackend>>> {
        self.get(name).map(|f| f.build(topic_path))
    }

    /// True if a factory is registered for the given name.
    pub fn has(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }

    /// Iterate over registered wire-names.
    pub fn registered_names(&self) -> impl Iterator<Item = &&'static str> {
        self.factories.keys()
    }

    /// Number of registered factories.
    pub fn len(&self) -> usize {
        self.factories.len()
    }

    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }
}

impl Default for QueueBackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Built-in factories ──────────────────────────────────────────

/// Factory for [`JsonlBackend`]. Always-on (no feature gate);
/// JSONL is the default backend for any topic without an explicit
/// `queue-config.yaml` override.
pub struct JsonlBackendFactory;

impl QueueBackendFactory for JsonlBackendFactory {
    fn name(&self) -> &'static str {
        "jsonl"
    }
    // supports_ack defaults to false — JSONL is fan-out only.
    fn build(&self, topic_path: &Path) -> Result<Arc<dyn QueueBackend>> {
        Ok(Arc::new(JsonlBackend::new(topic_path.to_path_buf())))
    }
}

/// Factory for [`SqliteBackend`]. Gated by the `sqlite` feature
/// (matches the `SqliteBackend` itself). Required for any topic
/// declaring `ack_required: true`.
#[cfg(feature = "sqlite")]
pub struct SqliteBackendFactory;

#[cfg(feature = "sqlite")]
impl QueueBackendFactory for SqliteBackendFactory {
    fn name(&self) -> &'static str {
        "sqlite"
    }
    fn supports_ack(&self) -> bool {
        true
    }
    fn build(&self, topic_path: &Path) -> Result<Arc<dyn QueueBackend>> {
        Ok(Arc::new(SqliteBackend::open(topic_path)?))
    }
}

/// Canonical list of built-in queue backend factories.
///
/// V5-MOD-3 Phase 3 wire-up (in cli + dashboard startup):
/// ```ignore
/// use neurogrim_core::queue_backend::{QueueBackendRegistry, built_in_factories};
/// let mut registry = QueueBackendRegistry::new();
/// registry.register_all(built_in_factories());
/// ```
///
/// Under `--features sqlite` (default in cli + dashboard) returns
/// 2 factories (jsonl + sqlite); without the feature, returns 1
/// (jsonl only).
pub fn built_in_factories() -> Vec<Box<dyn QueueBackendFactory>> {
    #[allow(unused_mut)]
    let mut factories: Vec<Box<dyn QueueBackendFactory>> =
        vec![Box::new(JsonlBackendFactory)];
    #[cfg(feature = "sqlite")]
    factories.push(Box::new(SqliteBackendFactory));
    factories
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::{Priority, QueueMessage};
    use serde_json::json;
    use tempfile::TempDir;

    // ── Shared property suite ──
    //
    // These tests exercise both backends through the same trait.
    // Each test takes a constructor closure so it can run against
    // JSONL + SQLite from a single body. The pattern lifts the
    // "8+ tests across both backends with same property-suite"
    // requirement from the S13-B-3 spec into one place.

    fn run_appended_messages_round_trip(make: fn(&TempDir) -> Box<dyn QueueBackend>) {
        let dir = TempDir::new().expect("tempdir");
        let be = make(&dir);
        assert_eq!(be.len().unwrap(), 0);
        let m1 = QueueMessage::new("test/topic", json!({"n": 1}));
        let m2 = QueueMessage::new("test/topic", json!({"n": 2}));
        let off1 = be.append(&m1).unwrap();
        let off2 = be.append(&m2).unwrap();
        assert!(off2 > off1);
        let all = be.read_from(0, 10).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].message.payload, m1.payload);
        assert_eq!(all[1].message.payload, m2.payload);
        assert_eq!(be.len().unwrap(), 2);
    }

    fn run_read_from_offset(make: fn(&TempDir) -> Box<dyn QueueBackend>) {
        let dir = TempDir::new().expect("tempdir");
        let be = make(&dir);
        for i in 0..5 {
            let m = QueueMessage::new("test/topic", json!({"i": i}));
            be.append(&m).unwrap();
        }
        let off2 = be.read_from(0, 10).unwrap()[2].offset;
        let from_2 = be.read_from(off2, 10).unwrap();
        assert_eq!(from_2.len(), 3);
        assert_eq!(from_2[0].message.payload, json!({"i": 2}));
    }

    fn run_read_with_limit(make: fn(&TempDir) -> Box<dyn QueueBackend>) {
        let dir = TempDir::new().expect("tempdir");
        let be = make(&dir);
        for i in 0..10 {
            let m = QueueMessage::new("test/topic", json!({"i": i}));
            be.append(&m).unwrap();
        }
        let first_three = be.read_from(0, 3).unwrap();
        assert_eq!(first_three.len(), 3);
    }

    fn run_priority_round_trip(make: fn(&TempDir) -> Box<dyn QueueBackend>) {
        let dir = TempDir::new().expect("tempdir");
        let be = make(&dir);
        let m = QueueMessage::new("test/topic", json!({})).with_priority(Priority::High);
        be.append(&m).unwrap();
        let read = be.read_from(0, 10).unwrap();
        assert_eq!(read[0].message.priority, Priority::High);
    }

    fn run_expires_at_round_trip(make: fn(&TempDir) -> Box<dyn QueueBackend>) {
        let dir = TempDir::new().expect("tempdir");
        let be = make(&dir);
        let mut m = QueueMessage::new("test/topic", json!({}));
        let exp = chrono::Utc::now() + chrono::Duration::hours(1);
        m.expires_at = Some(exp);
        be.append(&m).unwrap();
        let read = be.read_from(0, 10).unwrap();
        // RFC3339 round-trip can differ in subsecond precision; compare
        // truncated to seconds.
        let read_exp = read[0].message.expires_at.unwrap();
        assert_eq!(read_exp.timestamp(), exp.timestamp());
    }

    fn run_empty_returns_zero_len(make: fn(&TempDir) -> Box<dyn QueueBackend>) {
        let dir = TempDir::new().expect("tempdir");
        let be = make(&dir);
        assert_eq!(be.len().unwrap(), 0);
        assert!(be.read_from(0, 10).unwrap().is_empty());
    }

    fn run_uuid_preserved(make: fn(&TempDir) -> Box<dyn QueueBackend>) {
        let dir = TempDir::new().expect("tempdir");
        let be = make(&dir);
        let m = QueueMessage::new("test/topic", json!({"x": 1}));
        let original_id = m.id;
        be.append(&m).unwrap();
        let read = be.read_from(0, 10).unwrap();
        assert_eq!(read[0].message.id, original_id);
    }

    // ── JsonlBackend constructor + suite ──

    fn make_jsonl(dir: &TempDir) -> Box<dyn QueueBackend> {
        Box::new(JsonlBackend::new(dir.path().join("test.jsonl")))
    }

    #[test]
    fn jsonl_appended_messages_round_trip() {
        run_appended_messages_round_trip(make_jsonl);
    }

    #[test]
    fn jsonl_read_from_offset() {
        run_read_from_offset(make_jsonl);
    }

    #[test]
    fn jsonl_read_with_limit() {
        run_read_with_limit(make_jsonl);
    }

    #[test]
    fn jsonl_priority_round_trip() {
        run_priority_round_trip(make_jsonl);
    }

    #[test]
    fn jsonl_expires_at_round_trip() {
        run_expires_at_round_trip(make_jsonl);
    }

    #[test]
    fn jsonl_empty_returns_zero_len() {
        run_empty_returns_zero_len(make_jsonl);
    }

    #[test]
    fn jsonl_uuid_preserved() {
        run_uuid_preserved(make_jsonl);
    }

    #[test]
    fn jsonl_does_not_support_ack() {
        let dir = TempDir::new().expect("tempdir");
        let be = make_jsonl(&dir);
        assert!(!be.supports_ack());
        let m = QueueMessage::new("test/topic", json!({}));
        be.append(&m).unwrap();
        // ack-related calls error out cleanly.
        assert!(be.ack(0, "consumer-1").is_err());
        assert!(be.read_unacked("consumer-1", 10).is_err());
        // last_acked returns None instead of erroring (it's a query,
        // not a state-mutation, so a "no ack state exists" answer
        // is well-defined).
        assert_eq!(be.last_acked("consumer-1").unwrap(), None);
    }

    // ── SqliteBackend constructor + suite ──

    #[cfg(feature = "sqlite")]
    fn make_sqlite(dir: &TempDir) -> Box<dyn QueueBackend> {
        Box::new(SqliteBackend::open(&dir.path().join("test.sqlite")).unwrap())
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_appended_messages_round_trip() {
        run_appended_messages_round_trip(make_sqlite);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_read_from_offset() {
        run_read_from_offset(make_sqlite);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_read_with_limit() {
        run_read_with_limit(make_sqlite);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_priority_round_trip() {
        run_priority_round_trip(make_sqlite);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_expires_at_round_trip() {
        run_expires_at_round_trip(make_sqlite);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_empty_returns_zero_len() {
        run_empty_returns_zero_len(make_sqlite);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_uuid_preserved() {
        run_uuid_preserved(make_sqlite);
    }

    // ── SqliteBackend ack-specific tests ──

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_supports_ack() {
        let dir = TempDir::new().expect("tempdir");
        let be = make_sqlite(&dir);
        assert!(be.supports_ack());
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_ack_marks_message_consumed_for_a_group() {
        let dir = TempDir::new().expect("tempdir");
        let be = make_sqlite(&dir);
        let m1 = QueueMessage::new("test/topic", json!({"n": 1}));
        let m2 = QueueMessage::new("test/topic", json!({"n": 2}));
        let off1 = be.append(&m1).unwrap();
        let off2 = be.append(&m2).unwrap();
        // Initially both unacked for "group-A".
        let unacked = be.read_unacked("group-A", 10).unwrap();
        assert_eq!(unacked.len(), 2);
        // Ack the first.
        be.ack(off1, "group-A").unwrap();
        let unacked = be.read_unacked("group-A", 10).unwrap();
        assert_eq!(unacked.len(), 1);
        assert_eq!(unacked[0].offset, off2);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_ack_isolated_per_consumer_group() {
        // Two different consumer groups each see the full stream
        // until they ack — fan-out semantics on top of per-group
        // exactly-once.
        let dir = TempDir::new().expect("tempdir");
        let be = make_sqlite(&dir);
        let m = QueueMessage::new("test/topic", json!({"n": 1}));
        let off = be.append(&m).unwrap();
        be.ack(off, "group-A").unwrap();
        // group-A sees nothing more.
        assert!(be.read_unacked("group-A", 10).unwrap().is_empty());
        // group-B still sees the message.
        let group_b = be.read_unacked("group-B", 10).unwrap();
        assert_eq!(group_b.len(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_ack_idempotent() {
        let dir = TempDir::new().expect("tempdir");
        let be = make_sqlite(&dir);
        let m = QueueMessage::new("test/topic", json!({"n": 1}));
        let off = be.append(&m).unwrap();
        be.ack(off, "group-A").unwrap();
        // Second ack is a no-op (no error).
        be.ack(off, "group-A").unwrap();
        assert_eq!(be.last_acked("group-A").unwrap(), Some(off));
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_ack_unknown_offset_errors() {
        let dir = TempDir::new().expect("tempdir");
        let be = make_sqlite(&dir);
        let result = be.ack(99_999, "group-A");
        assert!(result.is_err(), "ack for non-existent offset should fail");
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_last_acked_tracks_max() {
        let dir = TempDir::new().expect("tempdir");
        let be = make_sqlite(&dir);
        let mut offsets = vec![];
        for i in 0..5 {
            let m = QueueMessage::new("test/topic", json!({"i": i}));
            offsets.push(be.append(&m).unwrap());
        }
        be.ack(offsets[2], "group-A").unwrap();
        be.ack(offsets[4], "group-A").unwrap();
        be.ack(offsets[1], "group-A").unwrap();
        // last_acked is MAX, not LAST-acked.
        assert_eq!(be.last_acked("group-A").unwrap(), Some(offsets[4]));
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_persists_across_reopen() {
        // Durability check: write through one connection, close,
        // reopen, observe.
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("durable.sqlite");
        {
            let be = SqliteBackend::open(&path).unwrap();
            let m = QueueMessage::new("test/topic", json!({"durable": true}));
            be.append(&m).unwrap();
        }
        let be = SqliteBackend::open(&path).unwrap();
        assert_eq!(be.len().unwrap(), 1);
        let read = be.read_from(0, 10).unwrap();
        assert_eq!(read[0].message.payload, json!({"durable": true}));
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_acks_persist_across_reopen() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("durable-acks.sqlite");
        let off;
        {
            let be = SqliteBackend::open(&path).unwrap();
            let m = QueueMessage::new("test/topic", json!({"n": 1}));
            off = be.append(&m).unwrap();
            be.ack(off, "group-A").unwrap();
        }
        let be = SqliteBackend::open(&path).unwrap();
        assert!(be.read_unacked("group-A", 10).unwrap().is_empty());
        assert_eq!(be.last_acked("group-A").unwrap(), Some(off));
    }

    // ────────────────────────────────────────────────────────────────
    // V5-MOD-3 Phase 1 tests — QueueBackendFactory + Registry
    // ────────────────────────────────────────────────────────────────

    /// Compile-only object-safety guards. Fail to compile if a
    /// future change breaks `Box<dyn>` / `Arc<dyn>` dispatch.
    #[allow(dead_code)]
    fn _object_safety_check_backend(_: Box<dyn QueueBackend>) {}
    #[allow(dead_code)]
    fn _object_safety_check_factory(_: Box<dyn QueueBackendFactory>) {}
    #[allow(dead_code)]
    fn _arc_dyn_backend_works(_: Arc<dyn QueueBackend>) {}

    #[test]
    fn jsonl_factory_builds_a_working_backend() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");
        let factory = JsonlBackendFactory;
        assert_eq!(factory.name(), "jsonl");
        assert!(!factory.supports_ack());
        let be = factory.build(&path).unwrap();
        let m = QueueMessage::new("test/topic", json!({"n": 1}));
        let off = be.append(&m).unwrap();
        assert_eq!(off, 0);
        assert_eq!(be.len().unwrap(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_factory_builds_a_working_backend() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.sqlite");
        let factory = SqliteBackendFactory;
        assert_eq!(factory.name(), "sqlite");
        assert!(factory.supports_ack());
        let be = factory.build(&path).unwrap();
        let m = QueueMessage::new("test/topic", json!({"n": 1}));
        let off = be.append(&m).unwrap();
        assert_eq!(off, 1); // SQLite ROWID starts at 1
        assert_eq!(be.len().unwrap(), 1);
    }

    #[test]
    fn empty_registry_has_no_factories() {
        let reg = QueueBackendRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        assert!(!reg.has("jsonl"));
        assert!(reg.get("anything").is_none());
        assert!(reg.build("anything", Path::new("/tmp")).is_none());
    }

    #[test]
    fn register_then_lookup_and_build() {
        let dir = TempDir::new().unwrap();
        let mut reg = QueueBackendRegistry::new();
        reg.register(Box::new(JsonlBackendFactory));
        assert_eq!(reg.len(), 1);
        assert!(reg.has("jsonl"));
        assert!(reg.get("jsonl").is_some());
        let be = reg
            .build("jsonl", &dir.path().join("topic.jsonl"))
            .expect("factory registered")
            .expect("build succeeds");
        let _: Arc<dyn QueueBackend> = be;
    }

    #[test]
    fn unknown_name_returns_none() {
        let mut reg = QueueBackendRegistry::new();
        reg.register(Box::new(JsonlBackendFactory));
        assert!(reg.get("does-not-exist").is_none());
        assert!(reg.build("does-not-exist", Path::new("/tmp")).is_none());
    }

    #[test]
    fn register_all_populates_from_iterator() {
        let mut reg = QueueBackendRegistry::new();
        reg.register_all(built_in_factories());
        // With sqlite feature: 2 factories. Without: 1.
        #[cfg(feature = "sqlite")]
        assert_eq!(reg.len(), 2);
        #[cfg(not(feature = "sqlite"))]
        assert_eq!(reg.len(), 1);
        assert!(reg.has("jsonl"));
        #[cfg(feature = "sqlite")]
        assert!(reg.has("sqlite"));
    }

    /// Built-in factories' `supports_ack()` matches the
    /// per-backend ack capabilities.
    #[test]
    fn built_in_factory_ack_capabilities_match_backends() {
        let factories = built_in_factories();
        for f in &factories {
            match f.name() {
                "jsonl" => assert!(!f.supports_ack(), "jsonl is fan-out, no ack"),
                "sqlite" => assert!(f.supports_ack(), "sqlite supports ack"),
                other => panic!("unexpected built-in: {other:?}"),
            }
        }
    }

    /// Last-write-wins on duplicate registration. Lets test fixtures
    /// override built-ins (e.g., wrap `JsonlBackendFactory` with an
    /// instrumented variant).
    struct AlternateJsonlFactory;
    impl QueueBackendFactory for AlternateJsonlFactory {
        fn name(&self) -> &'static str {
            "jsonl"
        }
        fn build(&self, topic_path: &Path) -> Result<Arc<dyn QueueBackend>> {
            Ok(Arc::new(JsonlBackend::new(topic_path.to_path_buf())))
        }
    }

    #[test]
    fn last_write_wins_on_duplicate_registration() {
        let mut reg = QueueBackendRegistry::new();
        reg.register(Box::new(JsonlBackendFactory));
        reg.register(Box::new(AlternateJsonlFactory));
        // Still 1 entry; later registration replaced earlier.
        assert_eq!(reg.len(), 1);
        assert!(reg.has("jsonl"));
    }
}
