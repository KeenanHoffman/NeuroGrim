//! # `queue-backend-memory` — V5-MOD-3 third-party example
//!
//! This crate demonstrates the modularity claim of V5-MOD-3: a
//! third-party crate can ship a custom
//! [`neurogrim_core::queue_backend::QueueBackend`] impl that plugs
//! into NeuroGrim's bus dispatch **without forking
//! `neurogrim-core` or `neurogrim-dashboard`**. It depends only on
//! the public contract surface in `neurogrim-core`, registers
//! itself at startup via the consuming binary's
//! [`neurogrim_core::queue_backend::QueueBackendRegistry`], and
//! passes the V5-MOD-3 Phase 4 conformance suite shipped with
//! `neurogrim-core`.
//!
//! Companion to V5-MOD-1's `scoring-source-prom` (HTTP-fetch) and
//! V5-MOD-2's `sensor-readme-quality` (FS-read). This example is
//! the in-memory pattern: pure logic, zero IO, **full ack
//! semantics** — the V5-MOD-1/2 examples didn't exercise ack so
//! this fills the gap.
//!
//! ## What it does
//!
//! [`MemoryQueueBackend`] holds:
//!
//! - **Per-topic message log**: `RwLock<Vec<StoredMessage>>` —
//!   append-only ring buffer with bounded capacity.
//! - **Per-consumer-group ack tracking**:
//!   `RwLock<HashMap<String, BTreeSet<u64>>>` — set of acked
//!   offsets per group. **Out-of-order acks supported** (matches
//!   `SqliteBackend`'s per-row ack semantics; plan-critic
//!   Subagent 1's 🟡 C2 finding caught this — naïve
//!   `HashMap<String, u64>` high-water-mark wouldn't represent
//!   "acked: {1, 4}, pending: {2, 3, 5+}").
//!
//! When the bounded capacity is exceeded, oldest messages are
//! FIFO-evicted with a `tracing::warn!` per drop. Useful for
//! tests, ephemeral coordination, or in-memory dev fixtures
//! where disk persistence isn't wanted.
//!
//! ## Why ack semantics matter for this example
//!
//! V5-MOD-1's HTTP-fetch and V5-MOD-2's FS-read examples both
//! demonstrate read-only flows. The `QueueBackend` contract is
//! richer — it includes ack semantics for transactional consumer
//! coordination. Showing a third-party backend implementing the
//! FULL trait surface (including the optional ack methods) gives
//! V5-SDK adopters a complete reference for what their
//! conformant impl looks like.
//!
//! ## How a consuming binary registers it
//!
//! ```ignore
//! use neurogrim_core::queue_backend::QueueBackendRegistry;
//! use neurogrim_dashboard::bus::BusState;
//! use queue_backend_memory::MemoryQueueBackendFactory;
//! use std::sync::Arc;
//!
//! fn build_bus() -> BusState {
//!     let mut registry = QueueBackendRegistry::new();
//!     registry.register_all(neurogrim_core::queue_backend::built_in_factories());
//!     // Third-party in-memory factory.
//!     registry.register(Box::new(MemoryQueueBackendFactory::default()));
//!     BusState::with_registry(Arc::new(registry))
//! }
//! ```
//!
//! Then a `queue-config.yaml` topic entry like:
//!
//! ```yaml
//! schema_version: "1"
//! topics:
//!   _neurogrim/scratch:
//!     backend: memory
//!     ack_required: true
//! ```
//!
//! routes through `MemoryQueueBackend`. Note: in-memory state
//! does NOT persist across process restarts — appropriate only
//! for ephemeral coordination patterns.
//!
//! ## Conformance
//!
//! `tests/conformance.rs` runs the cross-crate suite from
//! [`neurogrim_core::queue_backend_conformance`] against
//! [`MemoryQueueBackendFactory`]. Third-party plugin authors
//! should copy that test pattern into their own crate as the
//! canonical contract check.

use neurogrim_core::queue::QueueMessage;
use neurogrim_core::queue_backend::{
    QueueBackend, QueueBackendFactory, StoredMessage,
};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, RwLock};

/// Stable wire-name for the `memory` backend.
pub const BACKEND_NAME: &str = "memory";

/// Default capacity for new `MemoryQueueBackend` instances.
/// Bounded so a runaway producer can't exhaust process memory;
/// FIFO-evict oldest on overflow.
const DEFAULT_CAPACITY: usize = 10_000;

/// Third-party [`QueueBackend`] backed by an in-memory ring buffer
/// with full ack semantics. Stateless `Send + Sync` (V5-MOD-3
/// Fork A2 trait bound) via `RwLock`-wrapped interior state.
///
/// Each `MemoryQueueBackend` instance is bound to one topic; the
/// factory creates a fresh instance per topic, matching the
/// per-topic semantics of `JsonlBackend` and `SqliteBackend`.
pub struct MemoryQueueBackend {
    /// Topic this backend is bound to. Used in tracing breadcrumbs.
    topic: String,
    /// Maximum number of messages retained in the log. Older
    /// messages are FIFO-evicted with a warn log when this is
    /// exceeded.
    capacity: usize,
    /// Append-only message log. Wrapped in `RwLock` for
    /// `Send + Sync`.
    messages: RwLock<Vec<StoredMessage>>,
    /// Per-consumer-group acked offsets. `BTreeSet<u64>` per group
    /// supports out-of-order acks (matches `SqliteBackend`'s
    /// per-row ack semantics — caught by plan-critic Subagent 1's
    /// 🟡 C2 finding during V5-MOD-3 planning).
    acks: RwLock<HashMap<String, BTreeSet<u64>>>,
    /// Monotonic counter for assigning offsets. Wrapped in
    /// `RwLock` so concurrent `append` calls serialize through it
    /// (no TOCTOU race like `JsonlBackend`'s `len()`-then-`append`).
    next_offset: RwLock<u64>,
}

impl MemoryQueueBackend {
    /// Construct an empty in-memory backend for `topic` with the
    /// given capacity. When the log exceeds capacity, oldest
    /// messages are FIFO-evicted on each subsequent `append`.
    pub fn new(topic: impl Into<String>, capacity: usize) -> Self {
        Self {
            topic: topic.into(),
            capacity,
            messages: RwLock::new(Vec::with_capacity(capacity.min(1024))),
            acks: RwLock::new(HashMap::new()),
            next_offset: RwLock::new(0),
        }
    }
}

impl QueueBackend for MemoryQueueBackend {
    fn append(&self, msg: &QueueMessage) -> anyhow::Result<u64> {
        let mut next = self
            .next_offset
            .write()
            .map_err(|_| anyhow::anyhow!("memory queue offset lock poisoned"))?;
        let offset = *next;
        *next += 1;
        // Drop offset lock before touching the messages lock to
        // avoid lock-ordering surprises.
        drop(next);

        let mut log = self
            .messages
            .write()
            .map_err(|_| anyhow::anyhow!("memory queue log lock poisoned"))?;
        log.push(StoredMessage {
            offset,
            message: msg.clone(),
        });
        // Capacity enforcement: if we're over, drop the oldest. We
        // do this AFTER appending so the latest message is always
        // retained — under steady state this gives a sliding
        // window of the most-recent N messages.
        while log.len() > self.capacity {
            let dropped = log.remove(0);
            tracing::warn!(
                "memory queue {}: capacity {} exceeded, evicting offset {}",
                self.topic,
                self.capacity,
                dropped.offset
            );
        }
        Ok(offset)
    }

    fn read_from(
        &self,
        since_offset: u64,
        limit: usize,
    ) -> anyhow::Result<Vec<StoredMessage>> {
        let log = self
            .messages
            .read()
            .map_err(|_| anyhow::anyhow!("memory queue log lock poisoned"))?;
        Ok(log
            .iter()
            .filter(|sm| sm.offset >= since_offset)
            .take(limit)
            .cloned()
            .collect())
    }

    fn len(&self) -> anyhow::Result<u64> {
        let log = self
            .messages
            .read()
            .map_err(|_| anyhow::anyhow!("memory queue log lock poisoned"))?;
        Ok(log.len() as u64)
    }

    fn supports_ack(&self) -> bool {
        true
    }

    fn read_unacked(
        &self,
        consumer_group: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<StoredMessage>> {
        let log = self
            .messages
            .read()
            .map_err(|_| anyhow::anyhow!("memory queue log lock poisoned"))?;
        let acks = self
            .acks
            .read()
            .map_err(|_| anyhow::anyhow!("memory queue ack lock poisoned"))?;
        let acked = acks.get(consumer_group);
        Ok(log
            .iter()
            .filter(|sm| {
                acked
                    .map(|set| !set.contains(&sm.offset))
                    .unwrap_or(true)
            })
            .take(limit)
            .cloned()
            .collect())
    }

    fn ack(&self, offset: u64, consumer_group: &str) -> anyhow::Result<()> {
        // Verify the offset exists; matches SqliteBackend's
        // discipline (acking a non-existent offset is a clear
        // operator error rather than silent state corruption).
        {
            let log = self
                .messages
                .read()
                .map_err(|_| anyhow::anyhow!("memory queue log lock poisoned"))?;
            let exists = log.iter().any(|sm| sm.offset == offset);
            if !exists {
                anyhow::bail!(
                    "memory queue {}: cannot ack offset {} — no such message \
                     (was it FIFO-evicted under capacity pressure?)",
                    self.topic,
                    offset
                );
            }
        }
        let mut acks = self
            .acks
            .write()
            .map_err(|_| anyhow::anyhow!("memory queue ack lock poisoned"))?;
        // BTreeSet::insert is naturally idempotent — second ack of
        // same offset is a no-op (returns false but we don't care).
        acks.entry(consumer_group.to_string())
            .or_default()
            .insert(offset);
        Ok(())
    }

    fn last_acked(&self, consumer_group: &str) -> anyhow::Result<Option<u64>> {
        let acks = self
            .acks
            .read()
            .map_err(|_| anyhow::anyhow!("memory queue ack lock poisoned"))?;
        Ok(acks
            .get(consumer_group)
            .and_then(|set| set.iter().next_back().copied()))
    }
}

/// Factory for [`MemoryQueueBackend`]. Configurable per-topic
/// capacity (default 10_000); the same factory instance produces
/// backends with the same capacity for every topic.
pub struct MemoryQueueBackendFactory {
    capacity: usize,
}

impl Default for MemoryQueueBackendFactory {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_CAPACITY,
        }
    }
}

impl MemoryQueueBackendFactory {
    /// Construct a factory producing backends with the given
    /// capacity. Useful for tests that want to exercise FIFO
    /// eviction with small numbers (e.g., `with_capacity(3)` then
    /// append 5 messages → only the last 3 retained).
    pub fn with_capacity(capacity: usize) -> Self {
        Self { capacity }
    }
}

impl QueueBackendFactory for MemoryQueueBackendFactory {
    fn name(&self) -> &'static str {
        BACKEND_NAME
    }

    fn supports_ack(&self) -> bool {
        true
    }

    fn build(
        &self,
        _queue_root: &Path,
        topic: &str,
    ) -> anyhow::Result<Arc<dyn QueueBackend>> {
        // _queue_root is unused: in-memory backends don't touch
        // disk. The factory still receives it to match the trait
        // signature; backends that DO want to use the path
        // (e.g., for snapshot persistence) can.
        Ok(Arc::new(MemoryQueueBackend::new(topic, self.capacity)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neurogrim_core::queue::Priority;
    use serde_json::json;

    fn msg(n: u64) -> QueueMessage {
        QueueMessage {
            id: uuid::Uuid::nil(),
            topic: "_neurogrim/example-test".to_string(),
            payload: json!({ "n": n }),
            produced_at: chrono::Utc::now(),
            priority: Priority::Normal,
            expires_at: None,
        }
    }

    #[test]
    fn append_and_read_round_trip() {
        let be = MemoryQueueBackend::new("_neurogrim/test-round-trip", 100);
        for n in 0..5 {
            be.append(&msg(n)).unwrap();
        }
        let read = be.read_from(0, 100).unwrap();
        assert_eq!(read.len(), 5);
        assert_eq!(be.len().unwrap(), 5);
    }

    #[test]
    fn capacity_enforces_fifo_eviction() {
        let be = MemoryQueueBackend::new("_neurogrim/test-fifo", 3);
        for n in 0..5 {
            be.append(&msg(n)).unwrap();
        }
        // Only 3 retained; oldest 2 evicted.
        assert_eq!(be.len().unwrap(), 3);
        let read = be.read_from(0, 100).unwrap();
        // Offsets 2, 3, 4 retained.
        assert_eq!(read.len(), 3);
        assert_eq!(read[0].offset, 2);
        assert_eq!(read[2].offset, 4);
    }

    #[test]
    fn out_of_order_acks_supported() {
        // Plan-critic Subagent 1's 🟡 C2 finding: BTreeSet (not
        // u64 high-water-mark) is required to model out-of-order
        // ack arrivals.
        let be = MemoryQueueBackend::new("_neurogrim/test-ack", 100);
        for n in 0..5 {
            be.append(&msg(n)).unwrap();
        }
        // Ack 1, 4 (skipping 0, 2, 3).
        be.ack(1, "group-A").unwrap();
        be.ack(4, "group-A").unwrap();
        // read_unacked must return {0, 2, 3} for group-A.
        let unacked = be.read_unacked("group-A", 100).unwrap();
        let offsets: Vec<u64> = unacked.iter().map(|sm| sm.offset).collect();
        assert_eq!(offsets, vec![0, 2, 3]);
        // last_acked is the highest in the set: 4.
        assert_eq!(be.last_acked("group-A").unwrap(), Some(4));
    }

    #[test]
    fn ack_idempotency() {
        let be = MemoryQueueBackend::new("_neurogrim/test-idem", 100);
        be.append(&msg(0)).unwrap();
        be.ack(0, "group-A").unwrap();
        // Second ack of same offset is a no-op.
        be.ack(0, "group-A").unwrap();
        assert_eq!(be.last_acked("group-A").unwrap(), Some(0));
        assert!(be.read_unacked("group-A", 100).unwrap().is_empty());
    }

    #[test]
    fn unsupported_offset_ack_errors() {
        let be = MemoryQueueBackend::new("_neurogrim/test-bad", 100);
        be.append(&msg(0)).unwrap();
        // Offset 99 doesn't exist.
        assert!(be.ack(99, "group-A").is_err());
    }

    #[test]
    fn factory_builds_a_working_backend() {
        let factory = MemoryQueueBackendFactory::default();
        assert_eq!(factory.name(), "memory");
        assert!(factory.supports_ack());
        let be = factory
            .build(Path::new("/tmp"), "_neurogrim/factory-test")
            .unwrap();
        assert_eq!(be.len().unwrap(), 0);
        assert!(be.supports_ack());
    }
}
