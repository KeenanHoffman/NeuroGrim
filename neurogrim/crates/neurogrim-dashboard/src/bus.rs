//! Agent coordination bus (S13-B-2 v1, S13-B-3 v2 wiring).
//!
//! Wraps `neurogrim_core::queue` + `queue_backend` behind HTTP +
//! per-topic SSE pubsub. The dashboard owns the live-fanout layer;
//! every route ultimately calls into the right [`QueueBackend`]
//! per topic.
//!
//! ## Per-topic backend dispatch (S13-B-3 v2)
//!
//! At construction, the bus optionally loads
//! `<project>/.claude/brain/queue-config.yaml` and resolves each
//! topic's backend on demand:
//!
//! - `backend: jsonl` (default) → `JsonlBackend` at
//!   `.claude/brain/queues/<topic>.jsonl` (preserves `tail -f`
//!   inspectability).
//! - `backend: sqlite` (opt-in) → `SqliteBackend` at
//!   `.claude/brain/queues/<topic>.sqlite` (transactional, supports
//!   `ack_required: true` consumer-group semantics).
//!
//! The first publish or read for a topic lazy-creates its backend.
//! SQLite handles persist for the dashboard's lifetime; JSONL
//! handles are reconstructed-on-demand (cheap — they're just a
//! path).
//!
//! ## Responsibilities
//!
//! - **Persistence**: per-topic backend writes the actual data.
//!   For JSONL, subdirs preserve slash structure
//!   (`_neurogrim/approvals.jsonl`); for SQLite, subdirs work the
//!   same way (`pc-state/alerts.sqlite`).
//! - **Fanout**: per-topic [`tokio::sync::broadcast`] channel,
//!   capacity 64. Bounded to prevent memory growth from idle
//!   subscribers.
//! - **Discovery**: scans the queues directory for `*.jsonl` AND
//!   `*.sqlite` files to enumerate topics.
//!
//! ## What this module does NOT do
//!
//! - `--allow-mutations` enforcement (route layer).
//! - Compaction / retention scheduling. Daily auto-compaction is a
//!   follow-up; manual `neurogrim queue compact` is the v1 lever.
//! - MCP-side ack semantics. The MCP `queue_consume` tool still
//!   uses offset-based reads in v2; consumer-group-aware tools
//!   are deferred.

use neurogrim_core::queue::{JsonlQueueReader, QueueMessage};
use neurogrim_core::queue_backend::{
    JsonlBackend, QueueBackend, SqliteBackend, StoredMessage,
};
use neurogrim_core::queue_config::{BackendKind, QueueConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, RwLock};

/// Bounded channel capacity per topic. If subscribers can't keep
/// up they get a `Lagged(N)` error from the broadcast stream — the
/// SSE handler drops the lagged message rather than crashing.
pub const TOPIC_CHANNEL_CAPACITY: usize = 64;

/// Per-topic dispatcher. JSONL handles are stateless (each call
/// reopens the file); SQLite handles hold a long-lived `Connection`
/// behind a `Mutex` because rusqlite's `Connection` is `!Sync`.
pub enum BackendHandle {
    /// Stateless JSONL handle. The path resolves to
    /// `.claude/brain/queues/<topic>.jsonl`.
    Jsonl(PathBuf),
    /// SQLite handle wrapping an open `Connection`. The path
    /// resolves to `.claude/brain/queues/<topic>.sqlite`.
    Sqlite(Arc<Mutex<SqliteBackend>>),
}

impl BackendHandle {
    /// Append a single message. Returns the assigned offset.
    pub fn append(&self, msg: &QueueMessage) -> anyhow::Result<u64> {
        match self {
            BackendHandle::Jsonl(path) => {
                let mut be = JsonlBackend::new(path.clone());
                be.append(msg)
            }
            BackendHandle::Sqlite(handle) => {
                let mut be = handle.lock().expect("sqlite mutex poisoned");
                be.append(msg)
            }
        }
    }

    /// Read messages from the given offset, up to `limit`.
    pub fn read_from(
        &self,
        since_offset: u64,
        limit: usize,
    ) -> anyhow::Result<Vec<StoredMessage>> {
        match self {
            BackendHandle::Jsonl(path) => {
                JsonlBackend::new(path.clone()).read_from(since_offset, limit)
            }
            BackendHandle::Sqlite(handle) => {
                let be = handle.lock().expect("sqlite mutex poisoned");
                be.read_from(since_offset, limit)
            }
        }
    }

    /// Total message count.
    pub fn len(&self) -> anyhow::Result<u64> {
        match self {
            BackendHandle::Jsonl(path) => JsonlBackend::new(path.clone()).len(),
            BackendHandle::Sqlite(handle) => {
                let be = handle.lock().expect("sqlite mutex poisoned");
                be.len()
            }
        }
    }

    /// True iff this backend supports ack semantics.
    pub fn supports_ack(&self) -> bool {
        matches!(self, BackendHandle::Sqlite(_))
    }

    /// Read messages not yet acked by `consumer_group`. Errors for
    /// JSONL topics (which don't support ack semantics).
    pub fn read_unacked(
        &self,
        consumer_group: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<StoredMessage>> {
        match self {
            BackendHandle::Jsonl(_) => {
                anyhow::bail!(
                    "queue: read_unacked() called on a JSONL-backed topic; \
                     declare the topic with `backend: sqlite` and \
                     `ack_required: true` in queue-config.yaml"
                );
            }
            BackendHandle::Sqlite(handle) => {
                let be = handle.lock().expect("sqlite mutex poisoned");
                be.read_unacked(consumer_group, limit)
            }
        }
    }

    /// Mark a message as acked by `consumer_group`. Errors for
    /// JSONL topics.
    pub fn ack(&self, offset: u64, consumer_group: &str) -> anyhow::Result<()> {
        match self {
            BackendHandle::Jsonl(_) => {
                anyhow::bail!(
                    "queue: ack() called on a JSONL-backed topic; \
                     declare the topic with `backend: sqlite` and \
                     `ack_required: true` in queue-config.yaml"
                );
            }
            BackendHandle::Sqlite(handle) => {
                let mut be = handle.lock().expect("sqlite mutex poisoned");
                be.ack(offset, consumer_group)
            }
        }
    }

    /// Highest acked offset for `consumer_group`. None for JSONL
    /// topics.
    pub fn last_acked(&self, consumer_group: &str) -> anyhow::Result<Option<u64>> {
        match self {
            BackendHandle::Jsonl(_) => Ok(None),
            BackendHandle::Sqlite(handle) => {
                let be = handle.lock().expect("sqlite mutex poisoned");
                be.last_acked(consumer_group)
            }
        }
    }
}

/// Per-topic broadcast registry + backend cache. One sender per
/// topic; receivers are minted on demand by SSE subscribers.
/// Senders persist for the dashboard's lifetime.
#[derive(Clone)]
pub struct BusState {
    senders: Arc<RwLock<HashMap<String, broadcast::Sender<QueueMessage>>>>,
    /// v4.1 S13-B-3 v2: per-topic backend cache. SQLite handles
    /// persist (we don't want to reopen the connection for every
    /// publish); JSONL handles are reconstructed-on-demand so the
    /// cache holds just a marker.
    backends: Arc<RwLock<HashMap<String, Arc<BackendHandle>>>>,
    /// v4.1 S13-B-3 v2: queue-config.yaml resolved at startup.
    /// `None` when no config file exists — every topic defaults
    /// to JSONL.
    config: Arc<Option<QueueConfig>>,
}

impl BusState {
    /// Construct a bus with no per-topic config — every topic
    /// defaults to JSONL with the standard retention policy.
    pub fn new() -> Self {
        Self {
            senders: Arc::new(RwLock::new(HashMap::new())),
            backends: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(None),
        }
    }

    /// Construct a bus with the given queue-config. SQLite-backed
    /// topics will dispatch through `SqliteBackend` once their
    /// first publish/read lands.
    pub fn with_config(config: QueueConfig) -> Self {
        Self {
            senders: Arc::new(RwLock::new(HashMap::new())),
            backends: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(Some(config)),
        }
    }

    /// Helper: load `queue-config.yaml` from the conventional path
    /// + return a configured BusState. On any read/parse error,
    /// returns `Self::new()` and logs a warning — the bus still
    /// works (topics fall back to JSONL) but the operator's
    /// intended SQLite topics won't be honored until they fix the
    /// config.
    pub fn from_project_root(project_root: &Path) -> Self {
        let path = project_root
            .join(".claude")
            .join("brain")
            .join("queue-config.yaml");
        match QueueConfig::from_path(&path) {
            Ok(Some(cfg)) => Self::with_config(cfg),
            Ok(None) => Self::new(),
            Err(e) => {
                tracing::warn!(
                    "bus: queue-config.yaml at {} failed to parse: {} — \
                     all topics will fall back to JSONL",
                    path.display(),
                    e
                );
                Self::new()
            }
        }
    }

    /// Get-or-create the broadcast sender for a topic.
    pub async fn sender_for(&self, topic: &str) -> broadcast::Sender<QueueMessage> {
        {
            let map = self.senders.read().await;
            if let Some(s) = map.get(topic) {
                return s.clone();
            }
        }
        let mut map = self.senders.write().await;
        if let Some(s) = map.get(topic) {
            return s.clone();
        }
        let (tx, _rx) = broadcast::channel(TOPIC_CHANNEL_CAPACITY);
        map.insert(topic.to_string(), tx.clone());
        tx
    }

    /// Subscribe a fresh receiver for a topic.
    pub async fn subscribe(&self, topic: &str) -> broadcast::Receiver<QueueMessage> {
        self.sender_for(topic).await.subscribe()
    }

    /// Resolve (or lazy-create) the per-topic backend handle.
    /// Routes use this to get a handle that knows which backend
    /// type to dispatch to for read/write operations.
    pub async fn backend_for(
        &self,
        project_root: &Path,
        topic: &str,
    ) -> anyhow::Result<Arc<BackendHandle>> {
        // Fast path: already cached.
        {
            let map = self.backends.read().await;
            if let Some(h) = map.get(topic) {
                return Ok(h.clone());
            }
        }
        // Slow path: take exclusive lock + double-check.
        let mut map = self.backends.write().await;
        if let Some(h) = map.get(topic) {
            return Ok(h.clone());
        }
        let kind = match self.config.as_ref() {
            Some(cfg) => cfg.lookup(topic).backend,
            None => BackendKind::Jsonl,
        };
        let handle = match kind {
            BackendKind::Jsonl => {
                Arc::new(BackendHandle::Jsonl(jsonl_topic_path(project_root, topic)))
            }
            BackendKind::Sqlite => {
                let path = sqlite_topic_path(project_root, topic);
                let be = SqliteBackend::open(&path)?;
                Arc::new(BackendHandle::Sqlite(Arc::new(Mutex::new(be))))
            }
        };
        map.insert(topic.to_string(), handle.clone());
        Ok(handle)
    }

    /// Publish a message: dispatch to the topic's backend +
    /// fan out to live subscribers.
    ///
    /// Disk-write failures are propagated; fanout-send failures
    /// are silently swallowed (a `send` errors only when there
    /// are zero subscribers — the normal case for an idle topic).
    pub async fn publish(
        &self,
        project_root: &Path,
        msg: QueueMessage,
    ) -> anyhow::Result<QueueMessage> {
        let handle = self.backend_for(project_root, &msg.topic).await?;
        // Backend ops are sync — release the read lock before
        // acquiring any per-topic Mutex. The Arc<BackendHandle>
        // already gives us isolated per-topic synchronization.
        handle.append(&msg)?;
        let sender = self.sender_for(&msg.topic).await;
        let _ = sender.send(msg.clone());
        Ok(msg)
    }
}

impl Default for BusState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Path helpers ────────────────────────────────────────────────────────

/// Resolve the on-disk path for a topic. Topic segments separated
/// by `/` become directory levels; the leaf gets a `.jsonl`
/// extension. Preserves the inspectability invariant — adopters
/// can `tail -f .claude/brain/queues/_neurogrim/approvals.jsonl`
/// directly.
///
/// Backward-compat alias for `jsonl_topic_path`. New code should
/// prefer the explicit helpers; this name is kept because
/// `routes.rs` and tests depend on it.
pub fn topic_path(project_root: &Path, topic: &str) -> PathBuf {
    jsonl_topic_path(project_root, topic)
}

/// JSONL backing path: `<project>/.claude/brain/queues/<topic>.jsonl`.
pub fn jsonl_topic_path(project_root: &Path, topic: &str) -> PathBuf {
    let mut p = project_root
        .join(".claude")
        .join("brain")
        .join("queues");
    for segment in topic.split('/') {
        if !segment.is_empty() {
            p.push(segment);
        }
    }
    p.set_extension("jsonl");
    p
}

/// SQLite backing path: `<project>/.claude/brain/queues/<topic>.sqlite`.
pub fn sqlite_topic_path(project_root: &Path, topic: &str) -> PathBuf {
    let mut p = project_root
        .join(".claude")
        .join("brain")
        .join("queues");
    for segment in topic.split('/') {
        if !segment.is_empty() {
            p.push(segment);
        }
    }
    p.set_extension("sqlite");
    p
}

// ── Topic stats ─────────────────────────────────────────────────────────

/// Stats for a single topic. Cheap to compute. Used by
/// `GET /api/brains/:id/queues` and `neurogrim queue stats`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TopicStats {
    pub topic: String,
    pub message_count: usize,
    /// Bytes on disk. Approximates retention pressure.
    pub size_bytes: u64,
    /// `produced_at` of the oldest message, RFC3339. None when empty.
    pub oldest: Option<String>,
    /// `produced_at` of the newest message, RFC3339. None when empty.
    pub newest: Option<String>,
    /// v4.1 S13-B-3 v2: which backend stores this topic.
    pub backend: String,
}

impl TopicStats {
    /// Compute stats by reading the JSONL file directly.
    /// Backward-compat with v1 callers; new code should prefer
    /// [`Self::for_topic`] which respects queue-config.
    pub fn from_path(topic: &str, path: &Path) -> Self {
        let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let reader = JsonlQueueReader::open(path).ok();
        let messages = reader.map(|r| r.into_messages()).unwrap_or_default();
        let oldest = messages.first().map(|m| m.produced_at.to_rfc3339());
        let newest = messages.last().map(|m| m.produced_at.to_rfc3339());
        Self {
            topic: topic.to_string(),
            message_count: messages.len(),
            size_bytes,
            oldest,
            newest,
            backend: "jsonl".to_string(),
        }
    }

    /// Compute stats via the topic's resolved backend handle.
    /// Honors per-topic backend configuration.
    pub fn for_topic(handle: &BackendHandle, project_root: &Path, topic: &str) -> Self {
        let backend = match handle {
            BackendHandle::Jsonl(_) => "jsonl",
            BackendHandle::Sqlite(_) => "sqlite",
        };
        let path = match handle {
            BackendHandle::Jsonl(p) => p.clone(),
            BackendHandle::Sqlite(_) => sqlite_topic_path(project_root, topic),
        };
        let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        // For both backends, fetch a sample to extract oldest/newest.
        // We pull only the first + last message rather than the whole
        // ledger.
        let total = handle.len().unwrap_or(0);
        let head = handle.read_from(0, 1).unwrap_or_default();
        let tail = if total > 0 {
            handle
                .read_from(total.saturating_sub(1), 1)
                .unwrap_or_default()
        } else {
            vec![]
        };
        let oldest = head.first().map(|sm| sm.message.produced_at.to_rfc3339());
        let newest = tail.first().map(|sm| sm.message.produced_at.to_rfc3339());
        Self {
            topic: topic.to_string(),
            message_count: total as usize,
            size_bytes,
            oldest,
            newest,
            backend: backend.to_string(),
        }
    }
}

// ── Topic enumeration ───────────────────────────────────────────────────

/// Walk `<project>/.claude/brain/queues/` and enumerate every
/// topic with either a `.jsonl` OR `.sqlite` file on disk.
///
/// In v4.1 S13-B-3 v2, both extensions count as "topic exists" so
/// adopters who migrated to SQLite still see their topics in the
/// list/stats endpoints. Topics with both extensions present
/// (mid-migration state) are listed once.
pub fn list_topics(project_root: &Path) -> Vec<String> {
    let queues_root = project_root
        .join(".claude")
        .join("brain")
        .join("queues");
    if !queues_root.is_dir() {
        return Vec::new();
    }
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    walk_topics(&queues_root, &queues_root, &mut out);
    out.into_iter().collect()
}

fn walk_topics(
    root: &Path,
    dir: &Path,
    out: &mut std::collections::BTreeSet<String>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_topics(root, &path, out);
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str());
        if !matches!(ext, Some("jsonl") | Some("sqlite")) {
            continue;
        }
        if let Ok(rel) = path.strip_prefix(root) {
            let with_ext = rel.to_string_lossy().replace('\\', "/");
            // Strip whichever extension is present.
            let topic = with_ext
                .trim_end_matches(".jsonl")
                .trim_end_matches(".sqlite")
                .to_string();
            if !topic.is_empty() {
                out.insert(topic);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neurogrim_core::queue_config::{QueueConfig, TopicConfigYaml};
    use serde_json::json;
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    #[test]
    fn topic_path_for_reserved_namespace() {
        let root = Path::new("/proj");
        let p = topic_path(root, "_neurogrim/approvals");
        assert_eq!(
            p,
            Path::new("/proj/.claude/brain/queues/_neurogrim/approvals.jsonl")
        );
    }

    #[test]
    fn topic_path_for_adopter_namespace() {
        let root = Path::new("/proj");
        let p = topic_path(root, "pc-state/alerts");
        assert_eq!(
            p,
            Path::new("/proj/.claude/brain/queues/pc-state/alerts.jsonl")
        );
    }

    #[test]
    fn topic_path_for_single_segment() {
        let root = Path::new("/proj");
        let p = topic_path(root, "scratch");
        assert_eq!(p, Path::new("/proj/.claude/brain/queues/scratch.jsonl"));
    }

    #[test]
    fn sqlite_topic_path_uses_sqlite_extension() {
        let root = Path::new("/proj");
        let p = sqlite_topic_path(root, "pc-state/alerts");
        assert_eq!(
            p,
            Path::new("/proj/.claude/brain/queues/pc-state/alerts.sqlite")
        );
    }

    #[tokio::test]
    async fn sender_for_creates_lazily_and_dedupes() {
        let bus = BusState::new();
        let s1 = bus.sender_for("ng/test").await;
        let s2 = bus.sender_for("ng/test").await;
        let mut rx = s2.subscribe();
        let msg = QueueMessage::new("ng/test", json!({"hi": 1}));
        s1.send(msg.clone()).unwrap();
        let got = rx.recv().await.unwrap();
        assert_eq!(got.topic, "ng/test");
    }

    #[tokio::test]
    async fn publish_writes_disk_and_fans_out() {
        let dir = TempDir::new().unwrap();
        let bus = BusState::new();
        let mut rx = bus.subscribe("ng/test").await;
        let msg = QueueMessage::new("ng/test", json!({"k": "v"}));
        let written = bus.publish(dir.path(), msg.clone()).await.unwrap();
        assert_eq!(written.id, msg.id);
        let path = topic_path(dir.path(), "ng/test");
        let r = JsonlQueueReader::open(&path).unwrap();
        assert_eq!(r.len(), 1);
        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, msg.id);
    }

    #[tokio::test]
    async fn publish_with_no_subscribers_does_not_error() {
        let dir = TempDir::new().unwrap();
        let bus = BusState::new();
        let msg = QueueMessage::new("ng/test", json!({}));
        let res = bus.publish(dir.path(), msg).await;
        assert!(res.is_ok());
    }

    #[test]
    fn list_topics_walks_subdirs() {
        let dir = TempDir::new().unwrap();
        let queues = dir.path().join(".claude/brain/queues");
        std::fs::create_dir_all(queues.join("_neurogrim")).unwrap();
        std::fs::create_dir_all(queues.join("pc-state")).unwrap();
        std::fs::write(queues.join("_neurogrim/approvals.jsonl"), "{}").unwrap();
        std::fs::write(queues.join("pc-state/alerts.jsonl"), "{}").unwrap();
        std::fs::write(queues.join("scratch.jsonl"), "{}").unwrap();
        std::fs::write(queues.join("_neurogrim/notes.txt"), "ignore me").unwrap();
        let topics = list_topics(dir.path());
        assert_eq!(
            topics,
            vec![
                "_neurogrim/approvals".to_string(),
                "pc-state/alerts".to_string(),
                "scratch".to_string(),
            ]
        );
    }

    #[test]
    fn list_topics_includes_sqlite_files() {
        // S13-B-3 v2: SQLite-backed topics show up in the list too,
        // so adopters mid-migration don't see ghosts.
        let dir = TempDir::new().unwrap();
        let queues = dir.path().join(".claude/brain/queues");
        std::fs::create_dir_all(queues.join("pc-state")).unwrap();
        std::fs::write(queues.join("pc-state/alerts.sqlite"), b"").unwrap();
        std::fs::write(queues.join("scratch.jsonl"), "").unwrap();
        let topics = list_topics(dir.path());
        assert_eq!(
            topics,
            vec!["pc-state/alerts".to_string(), "scratch".to_string()]
        );
    }

    #[test]
    fn list_topics_dedupes_when_both_extensions_present() {
        // Mid-migration state: both files present. List once.
        let dir = TempDir::new().unwrap();
        let queues = dir.path().join(".claude/brain/queues");
        std::fs::create_dir_all(&queues).unwrap();
        std::fs::write(queues.join("scratch.jsonl"), "").unwrap();
        std::fs::write(queues.join("scratch.sqlite"), b"").unwrap();
        let topics = list_topics(dir.path());
        assert_eq!(topics, vec!["scratch".to_string()]);
    }

    #[test]
    fn list_topics_returns_empty_when_no_queues_dir() {
        let dir = TempDir::new().unwrap();
        assert!(list_topics(dir.path()).is_empty());
    }

    #[test]
    fn topic_stats_for_empty_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("scratch.jsonl");
        std::fs::write(&path, "").unwrap();
        let stats = TopicStats::from_path("scratch", &path);
        assert_eq!(stats.message_count, 0);
        assert_eq!(stats.oldest, None);
        assert_eq!(stats.newest, None);
        assert_eq!(stats.size_bytes, 0);
        assert_eq!(stats.backend, "jsonl");
    }

    #[test]
    fn topic_stats_for_populated_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("scratch.jsonl");
        for i in 0..3 {
            neurogrim_core::queue::append(
                &path,
                &QueueMessage::new("scratch", json!({"i": i})),
            )
            .unwrap();
        }
        let stats = TopicStats::from_path("scratch", &path);
        assert_eq!(stats.message_count, 3);
        assert!(stats.oldest.is_some());
        assert!(stats.newest.is_some());
        assert!(stats.size_bytes > 0);
        assert_eq!(stats.backend, "jsonl");
    }

    #[tokio::test]
    async fn subscribers_to_distinct_topics_are_isolated() {
        let bus = BusState::new();
        let mut rx_a = bus.subscribe("ng/a").await;
        let mut rx_b = bus.subscribe("ng/b").await;
        let dir = TempDir::new().unwrap();
        bus.publish(
            dir.path(),
            QueueMessage::new("ng/a", json!({"who": "a"})),
        )
        .await
        .unwrap();
        let got_a = rx_a.recv().await.unwrap();
        assert_eq!(got_a.topic, "ng/a");
        assert!(rx_b.try_recv().is_err());
    }

    #[tokio::test]
    async fn round_trip_publish_and_read_via_core_iter() {
        let dir = TempDir::new().unwrap();
        let bus = BusState::new();
        for i in 0..5 {
            bus.publish(
                dir.path(),
                QueueMessage::new("ng/t", json!({"i": i})),
            )
            .await
            .unwrap();
        }
        let path = topic_path(dir.path(), "ng/t");
        let reader = JsonlQueueReader::open(&path).unwrap();
        assert_eq!(reader.len(), 5);
        assert_eq!(reader.iter_from(3).count(), 2);
    }

    // ── S13-B-3 v2: backend dispatcher tests ──

    #[tokio::test]
    async fn backend_for_returns_jsonl_by_default() {
        let dir = TempDir::new().unwrap();
        let bus = BusState::new();
        let handle = bus.backend_for(dir.path(), "scratch").await.unwrap();
        assert!(matches!(handle.as_ref(), BackendHandle::Jsonl(_)));
    }

    #[tokio::test]
    async fn backend_for_returns_sqlite_when_config_says_so() {
        let dir = TempDir::new().unwrap();
        let mut topics = BTreeMap::new();
        topics.insert(
            "pc-state/alerts".to_string(),
            TopicConfigYaml {
                backend: Some(BackendKind::Sqlite),
                ..Default::default()
            },
        );
        let cfg = QueueConfig {
            schema_version: "1".to_string(),
            topics,
        };
        let bus = BusState::with_config(cfg);
        let handle = bus.backend_for(dir.path(), "pc-state/alerts").await.unwrap();
        assert!(matches!(handle.as_ref(), BackendHandle::Sqlite(_)));
        assert!(handle.supports_ack());
    }

    #[tokio::test]
    async fn backend_for_caches_so_repeated_calls_share_handle() {
        let dir = TempDir::new().unwrap();
        let bus = BusState::new();
        let h1 = bus.backend_for(dir.path(), "scratch").await.unwrap();
        let h2 = bus.backend_for(dir.path(), "scratch").await.unwrap();
        assert!(Arc::ptr_eq(&h1, &h2));
    }

    #[tokio::test]
    async fn publish_dispatches_to_sqlite_when_configured() {
        let dir = TempDir::new().unwrap();
        let mut topics = BTreeMap::new();
        topics.insert(
            "pc-state/alerts".to_string(),
            TopicConfigYaml {
                backend: Some(BackendKind::Sqlite),
                ..Default::default()
            },
        );
        let cfg = QueueConfig {
            schema_version: "1".to_string(),
            topics,
        };
        let bus = BusState::with_config(cfg);
        let msg = QueueMessage::new("pc-state/alerts", json!({"severity": "warn"}));
        bus.publish(dir.path(), msg.clone()).await.unwrap();

        // Disk artifact: SQLite file, not JSONL.
        let sqlite_path = sqlite_topic_path(dir.path(), "pc-state/alerts");
        assert!(sqlite_path.exists(), "sqlite file should be created");
        let jsonl_path = jsonl_topic_path(dir.path(), "pc-state/alerts");
        assert!(!jsonl_path.exists(), "jsonl file should NOT be created");

        // Read back via the handle.
        let handle = bus.backend_for(dir.path(), "pc-state/alerts").await.unwrap();
        let read = handle.read_from(0, 10).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].message.payload, json!({"severity": "warn"}));
    }

    #[tokio::test]
    async fn from_project_root_loads_existing_config() {
        let dir = TempDir::new().unwrap();
        let cfg_path = dir.path().join(".claude/brain/queue-config.yaml");
        std::fs::create_dir_all(cfg_path.parent().unwrap()).unwrap();
        std::fs::write(
            &cfg_path,
            r#"schema_version: "1"
topics:
  pc-state/alerts:
    backend: sqlite
    ack_required: true
"#,
        )
        .unwrap();
        let bus = BusState::from_project_root(dir.path());
        let handle = bus.backend_for(dir.path(), "pc-state/alerts").await.unwrap();
        assert!(handle.supports_ack());
    }

    #[tokio::test]
    async fn from_project_root_falls_back_when_config_missing() {
        let dir = TempDir::new().unwrap();
        let bus = BusState::from_project_root(dir.path());
        let handle = bus.backend_for(dir.path(), "scratch").await.unwrap();
        assert!(matches!(handle.as_ref(), BackendHandle::Jsonl(_)));
    }

    #[tokio::test]
    async fn jsonl_handle_rejects_ack_calls() {
        let dir = TempDir::new().unwrap();
        let bus = BusState::new();
        let handle = bus.backend_for(dir.path(), "scratch").await.unwrap();
        assert!(!handle.supports_ack());
        assert!(handle.ack(0, "g").is_err());
        assert!(handle.read_unacked("g", 10).is_err());
        assert_eq!(handle.last_acked("g").unwrap(), None);
    }

    #[tokio::test]
    async fn sqlite_handle_supports_ack_round_trip() {
        let dir = TempDir::new().unwrap();
        let mut topics = BTreeMap::new();
        topics.insert(
            "pc-state/alerts".to_string(),
            TopicConfigYaml {
                backend: Some(BackendKind::Sqlite),
                ack_required: Some(true),
                ..Default::default()
            },
        );
        let cfg = QueueConfig {
            schema_version: "1".to_string(),
            topics,
        };
        let bus = BusState::with_config(cfg);
        let msg = QueueMessage::new("pc-state/alerts", json!({"n": 1}));
        bus.publish(dir.path(), msg).await.unwrap();
        let handle = bus.backend_for(dir.path(), "pc-state/alerts").await.unwrap();
        let unacked = handle.read_unacked("group-A", 10).unwrap();
        assert_eq!(unacked.len(), 1);
        let off = unacked[0].offset;
        handle.ack(off, "group-A").unwrap();
        assert!(handle.read_unacked("group-A", 10).unwrap().is_empty());
    }
}
