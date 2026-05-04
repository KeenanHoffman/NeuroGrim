//! Process-level cache for `BrainContext` to avoid re-running the
//! full scoring pipeline on every request.
//!
//! ## Why
//!
//! `BrainContext::load(registry, hat, persona)` reads the registry,
//! loads every CMDB, and runs the full scoring pipeline (scorecard,
//! trajectory, recommendations). For multi-widget Overview pages
//! (the NeuroGrim custom layout fires ~9 parallel requests on
//! first paint), the per-request cost compounds.
//!
//! ## Design
//!
//! - Cache keyed by `(registry_path, hat, persona)`. Different hats
//!   produce different opinionated outputs, so each combination is
//!   stored separately.
//! - Each entry holds an `Arc<BrainContext>` so concurrent requests
//!   share the same allocation.
//! - **Two-tier invalidation:**
//!   1. **SSE-driven (primary).** A background task subscribes to
//!      the dashboard's broadcast channel and clears the cache on
//!      `RegistryChanged` / `ScoreChanged` events. Live edits to
//!      registries or CMDBs invalidate within milliseconds.
//!   2. **TTL-based (defense).** Each entry expires after 30s
//!      regardless. Bounds staleness even if an event is missed
//!      or the watcher is unavailable.
//! - **Thundering herd**: not coalesced. When the cache is cold and
//!   N concurrent requests miss simultaneously, all N load
//!   BrainContext from disk. Acceptable for the dashboard's
//!   typical N=9 burst (each load is ~50ms; total amortized cost
//!   is negligible vs. the value of avoiding a per-key lock).
//!   Tracked for v3.5+ if it ever shows up in profiles.

use anyhow::Result;
use neurogrim_mcp::context::BrainContext;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};

use crate::events::DashboardEvent;

/// How long a cached `BrainContext` is considered fresh in the
/// absence of an explicit invalidation. Picked empirically: long
/// enough to absorb the 9-widget burst at first paint plus a
/// tab-switch round-trip; short enough that any missed
/// invalidation self-heals quickly. SSE invalidation is the
/// primary mechanism — the TTL is just a safety net.
const CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    registry_path: String,
    hat: Option<String>,
    persona: Option<String>,
}

struct Entry {
    ctx: Arc<BrainContext>,
    inserted_at: Instant,
}

pub struct BrainContextCache {
    inner: Arc<RwLock<HashMap<CacheKey, Entry>>>,
    /// v4.5 — optional metrics handle. When set, every load_or_get
    /// records a `cache_event{cache="brain_context", kind=hit|miss}`
    /// data point and every event-driven invalidation records
    /// `kind=invalidate`. None in tests that don't care.
    metrics: Option<neurogrim_core::metrics::MetricsHandle>,
}

impl std::fmt::Debug for BrainContextCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Don't try to print BrainContext (heavy + not Debug). Just
        // surface the cache size for diagnostics.
        f.debug_struct("BrainContextCache")
            .field("entries", &"<unknown — read-locked>")
            .finish()
    }
}

impl BrainContextCache {
    /// Construct a cache that auto-invalidates on broadcast events.
    /// When `events` is `None` (test path, or production with the
    /// watcher disabled), the cache still works — entries just
    /// expire on the TTL alone.
    pub fn new(events: Option<&broadcast::Sender<DashboardEvent>>) -> Self {
        Self::new_with_metrics(events, None)
    }

    /// v4.5 — explicit constructor that takes an optional metrics handle.
    /// `new()` defaults to no metrics (tests + the legacy `AppState::new`
    /// constructor). `AppState::with_events` calls this directly.
    pub fn new_with_metrics(
        events: Option<&broadcast::Sender<DashboardEvent>>,
        metrics: Option<neurogrim_core::metrics::MetricsHandle>,
    ) -> Self {
        let inner: Arc<RwLock<HashMap<CacheKey, Entry>>> =
            Arc::new(RwLock::new(HashMap::new()));

        if let Some(tx) = events {
            let inner_clone = Arc::clone(&inner);
            let metrics_clone = metrics.clone();
            let mut rx = tx.subscribe();
            tokio::spawn(async move {
                while let Ok(event) = rx.recv().await {
                    match event {
                        // Registry edits or score changes: every
                        // cached BrainContext is now potentially
                        // stale. Clear the whole cache.
                        DashboardEvent::RegistryChanged
                        | DashboardEvent::ScoreChanged { .. } => {
                            inner_clone.write().await.clear();
                            if let Some(m) = &metrics_clone {
                                let tags = neurogrim_core::metrics::Tags::new()
                                    .with("cache", "brain_context")
                                    .with("kind", "invalidate");
                                m.record("cache_event", &tags, 1.0);
                            }
                            tracing::debug!("BrainContext cache cleared via SSE event");
                        }
                        // Skill / layout / service-lifecycle / Logs-
                        // source events don't affect any BrainContext
                        // field — those are surfaced via separate
                        // routes that don't share the cache.
                        DashboardEvent::SkillInvoked
                        | DashboardEvent::LayoutChanged
                        | DashboardEvent::ServiceStarting { .. }
                        | DashboardEvent::ServiceStarted { .. }
                        | DashboardEvent::ServiceStopped { .. }
                        | DashboardEvent::ServiceFailed { .. }
                        | DashboardEvent::PublishGateLedgerAppended
                        | DashboardEvent::ApprovalResolved
                        | DashboardEvent::NotificationPublished
                        | DashboardEvent::ServicesLogAppended
                        | DashboardEvent::QueueConfigChanged => {}
                    }
                }
            });
        }

        Self { inner, metrics }
    }

    /// Get a cached `BrainContext` if fresh, otherwise load it.
    /// The returned `Arc` is shared with any concurrent caller
    /// holding a hit on the same key.
    pub async fn load_or_get(
        &self,
        registry_path: &str,
        hat: Option<String>,
        persona: Option<String>,
    ) -> Result<Arc<BrainContext>> {
        let key = CacheKey {
            registry_path: registry_path.to_string(),
            hat: hat.clone(),
            persona: persona.clone(),
        };

        // Fast path: read lock, return cached if fresh.
        {
            let guard = self.inner.read().await;
            if let Some(entry) = guard.get(&key) {
                if entry.inserted_at.elapsed() < CACHE_TTL {
                    self.record_event("hit");
                    return Ok(Arc::clone(&entry.ctx));
                }
            }
        }
        self.record_event("miss");

        // Slow path: load fresh.
        let ctx = BrainContext::load(registry_path, hat, persona).await?;
        let arc = Arc::new(ctx);

        // Insert (overwrite if present, e.g., race against a
        // concurrent loader — last write wins, both arcs are
        // semantically equivalent).
        self.inner.write().await.insert(
            key,
            Entry {
                ctx: Arc::clone(&arc),
                inserted_at: Instant::now(),
            },
        );

        Ok(arc)
    }

    /// Internal helper: record a `cache_event{cache="brain_context",
    /// kind=...}` data point. No-op when no metrics handle is wired.
    fn record_event(&self, kind: &str) {
        if let Some(m) = &self.metrics {
            let tags = neurogrim_core::metrics::Tags::new()
                .with("cache", "brain_context")
                .with("kind", kind);
            m.record("cache_event", &tags, 1.0);
        }
    }

    /// Test/diagnostic helper: number of currently-cached entries.
    /// Not exposed via API; used by unit tests and the in-process
    /// debug surface.
    #[allow(dead_code)]
    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    /// Test helper: forget every cached entry.
    #[allow(dead_code)]
    pub async fn clear(&self) {
        self.inner.write().await.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Build a minimal valid registry on disk that BrainContext::load
    /// will accept. Returns the TempDir (owns the lifetime) + path.
    fn make_registry() -> (TempDir, String) {
        let tmp = TempDir::new().unwrap();
        let registry_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&registry_dir).unwrap();
        let registry_path = registry_dir.join("brain-registry.json");
        let registry = serde_json::json!({
            "meta": {
                "schema_version": "2",
                "description": "cache test",
                "updated_by": "test"
            },
            "tools": {},
            "data_sources": {},
            "config": {
                "domain_weights": {"placeholder": 0.0},
                "domain_definitions": {
                    "placeholder": {
                        "principle": "x",
                        "scoring_source": null,
                        "exported_variables": {}
                    }
                }
            }
        });
        std::fs::write(&registry_path, registry.to_string()).unwrap();
        let path_str = registry_path.to_string_lossy().to_string();
        (tmp, path_str)
    }

    #[tokio::test]
    async fn cache_hit_returns_same_arc() {
        let (_tmp, path) = make_registry();
        let cache = BrainContextCache::new(None);
        let a = cache.load_or_get(&path, None, None).await.unwrap();
        let b = cache.load_or_get(&path, None, None).await.unwrap();
        assert!(Arc::ptr_eq(&a, &b), "second hit should return the same Arc");
        assert_eq!(cache.len().await, 1);
    }

    #[tokio::test]
    async fn different_hats_keyed_separately() {
        let (_tmp, path) = make_registry();
        let cache = BrainContextCache::new(None);
        let _ = cache.load_or_get(&path, None, None).await.unwrap();
        let _ = cache
            .load_or_get(&path, Some("engineer".to_string()), None)
            .await
            .unwrap();
        assert_eq!(cache.len().await, 2);
    }

    #[tokio::test]
    async fn registry_changed_event_clears_cache() {
        let (_tmp, path) = make_registry();
        let (tx, _rx) = broadcast::channel::<DashboardEvent>(16);
        let cache = BrainContextCache::new(Some(&tx));
        let _ = cache.load_or_get(&path, None, None).await.unwrap();
        assert_eq!(cache.len().await, 1);

        // Fire a RegistryChanged event.
        let _ = tx.send(DashboardEvent::RegistryChanged);
        // Give the spawned listener a tick to process.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(cache.len().await, 0);
    }

    #[tokio::test]
    async fn score_changed_event_clears_cache() {
        let (_tmp, path) = make_registry();
        let (tx, _rx) = broadcast::channel::<DashboardEvent>(16);
        let cache = BrainContextCache::new(Some(&tx));
        let _ = cache.load_or_get(&path, None, None).await.unwrap();
        assert_eq!(cache.len().await, 1);

        let _ = tx.send(DashboardEvent::ScoreChanged { domain: None });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(cache.len().await, 0);
    }

    #[tokio::test]
    async fn skill_invoked_event_does_not_clear_cache() {
        // Skill ledger / layout edits don't affect any
        // BrainContext field. The cache must NOT clear on those —
        // doing so would cause unnecessary re-loads on every
        // skill invocation in the operator's session.
        let (_tmp, path) = make_registry();
        let (tx, _rx) = broadcast::channel::<DashboardEvent>(16);
        let cache = BrainContextCache::new(Some(&tx));
        let _ = cache.load_or_get(&path, None, None).await.unwrap();

        let _ = tx.send(DashboardEvent::SkillInvoked);
        let _ = tx.send(DashboardEvent::LayoutChanged);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(cache.len().await, 1, "skill/layout events must NOT clear");
    }

    #[tokio::test]
    async fn cache_returns_a_valid_brain_context() {
        let (_tmp, path) = make_registry();
        let cache = BrainContextCache::new(None);
        let ctx = cache.load_or_get(&path, None, None).await.unwrap();
        // Smoke: confirm the loaded context has at least one domain.
        assert!(!ctx.registry.config.domain_weights.is_empty());
    }
}
