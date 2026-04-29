//! Agent coordination bus (S13-B-2, v4.1).
//!
//! Wraps the [`neurogrim_core::queue`] primitive behind HTTP +
//! per-topic SSE pubsub. The dashboard owns the live-fanout layer;
//! every route ultimately calls into the core queue's `append` /
//! `JsonlQueueReader::open` for persistence.
//!
//! ## Responsibilities
//!
//! - **Persistence**: writes go to
//!   `<project>/.claude/brain/queues/<topic>.jsonl` (subdirs for
//!   slashes in the topic — preserves `cat` inspectability).
//! - **Fanout**: per-topic [`tokio::sync::broadcast`] channel, lazy-
//!   initialized on first publish or subscribe. Bounded at 64 to
//!   prevent memory growth from idle subscribers (matches the
//!   v3.4 events.rs cap).
//! - **Discovery**: scans the queues directory for `*.jsonl` files
//!   to enumerate topics — there's no separate registry.
//!
//! ## What this module does NOT do
//!
//! - SQLite backend. S13-B-3 will add a [`QueueBackend`] trait that
//!   delegates to either JSONL (this module) or SQLite (next story).
//! - `--allow-mutations` enforcement. The route layer (in
//!   `routes.rs::brain_queue_publish`) gates POST writes; this module
//!   just performs them.
//! - Compaction / retention. Daily auto-compaction is a follow-up
//!   (S13-B-7's expanded scope). The CLI `queue compact` is the
//!   manual lever in v1.

use neurogrim_core::queue::{JsonlQueueReader, QueueMessage};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Bounded channel capacity per topic. Same magic number as the
/// global events channel in v3.4. If subscribers can't keep up,
/// they get a `Lagged(N)` error from the broadcast stream — the
/// SSE handler handles this by dropping the lagged message rather
/// than crashing.
pub const TOPIC_CHANNEL_CAPACITY: usize = 64;

/// Per-topic broadcast registry. One sender per topic; receivers
/// are minted on demand by SSE subscribers. Senders persist for
/// the lifetime of the dashboard process even if no subscribers
/// remain — re-creating them on each subscribe would race with
/// concurrent publishers.
#[derive(Clone)]
pub struct BusState {
    senders: Arc<RwLock<HashMap<String, broadcast::Sender<QueueMessage>>>>,
}

impl BusState {
    pub fn new() -> Self {
        Self {
            senders: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get-or-create the broadcast sender for a topic. Cloned so the
    /// caller can `subscribe()` without holding our internal lock.
    pub async fn sender_for(&self, topic: &str) -> broadcast::Sender<QueueMessage> {
        // Read-lock first for the common (already-exists) case.
        {
            let map = self.senders.read().await;
            if let Some(s) = map.get(topic) {
                return s.clone();
            }
        }
        // Slow path: take the write lock. Double-check under the
        // exclusive lock in case a racer just inserted.
        let mut map = self.senders.write().await;
        if let Some(s) = map.get(topic) {
            return s.clone();
        }
        let (tx, _rx) = broadcast::channel(TOPIC_CHANNEL_CAPACITY);
        map.insert(topic.to_string(), tx.clone());
        tx
    }

    /// Subscribe a fresh receiver for a topic. Convenience over
    /// `sender_for(topic).await.subscribe()`.
    pub async fn subscribe(&self, topic: &str) -> broadcast::Receiver<QueueMessage> {
        self.sender_for(topic).await.subscribe()
    }

    /// Publish a message: write to disk + fan out to live
    /// subscribers. Returns the message that was written (with the
    /// id + produced_at filled in if the caller didn't set them).
    ///
    /// Disk-write failures are propagated; fanout-send failures are
    /// silently swallowed (a `send` errors only when there are
    /// zero subscribers, which is the normal case for an idle topic).
    pub async fn publish(
        &self,
        project_root: &Path,
        msg: QueueMessage,
    ) -> anyhow::Result<QueueMessage> {
        let path = topic_path(project_root, &msg.topic);
        neurogrim_core::queue::append(&path, &msg)?;
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

/// Resolve the on-disk path for a topic's JSONL file. Topic
/// segments separated by `/` become directory levels; the leaf
/// gets a `.jsonl` extension. Preserves the inspectability
/// invariant — adopters can `tail -f
/// .claude/brain/queues/_neurogrim/approvals.jsonl` directly.
pub fn topic_path(project_root: &Path, topic: &str) -> PathBuf {
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

/// Stats for a single topic. Cheap to compute (one JSONL read).
/// Used by `GET /api/brains/:id/queues` and `neurogrim queue stats`.
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
}

impl TopicStats {
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
        }
    }
}

/// Walk `<project>/.claude/brain/queues/` and enumerate every topic
/// that has a `.jsonl` file on disk. Topic names are reconstructed
/// from path segments (`subdir/leaf.jsonl` → `subdir/leaf`).
pub fn list_topics(project_root: &Path) -> Vec<String> {
    let queues_root = project_root
        .join(".claude")
        .join("brain")
        .join("queues");
    if !queues_root.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    walk_jsonl(&queues_root, &queues_root, &mut out);
    out.sort();
    out
}

fn walk_jsonl(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_jsonl(root, &path, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        if let Ok(rel) = path.strip_prefix(root) {
            // Reconstruct topic by joining path segments minus the
            // .jsonl extension.
            let with_ext = rel.to_string_lossy().replace('\\', "/");
            let topic = with_ext.trim_end_matches(".jsonl").to_string();
            if !topic.is_empty() {
                out.push(topic);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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

    #[tokio::test]
    async fn sender_for_creates_lazily_and_dedupes() {
        let bus = BusState::new();
        let s1 = bus.sender_for("ng/test").await;
        let s2 = bus.sender_for("ng/test").await;
        // Same channel — sending on one delivers to subscribers of
        // the other (broadcast::Sender uses an Arc-backed channel,
        // so they're both views of the same underlying broadcaster).
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
        // Disk persisted.
        let path = topic_path(dir.path(), "ng/test");
        let r = JsonlQueueReader::open(&path).unwrap();
        assert_eq!(r.len(), 1);
        // Live subscriber received the broadcast.
        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, msg.id);
    }

    #[tokio::test]
    async fn publish_with_no_subscribers_does_not_error() {
        let dir = TempDir::new().unwrap();
        let bus = BusState::new();
        // No subscribe() call — broadcast::send returns Err but
        // publish swallows it.
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
        // A non-jsonl file should be ignored.
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
        // rx_b should NOT have received anything; use try_recv.
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
        // Resume from offset 3 → 2 messages.
        assert_eq!(reader.iter_from(3).count(), 2);
    }
}
