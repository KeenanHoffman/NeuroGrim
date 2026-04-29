//! Shared application state passed to every route handler.
//!
//! Phase 0.3: just the registry path.
//! Phase 2.1: adds the SSE broadcast Sender so the events handler
//! can subscribe a fresh receiver per connection.
//! Path 2 (multi-Brain): adds the BrainTree so per-Brain handlers
//! can resolve the URL's `brain_id` to a registry path without
//! re-walking the federation on every request.

use std::path::Path;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::brains::BrainTree;
use crate::cache::BrainContextCache;
use crate::events::DashboardEvent;
use crate::services::ServiceRegistry;

#[derive(Clone)]
pub struct AppState {
    /// Path to the host Brain's `brain-registry.json`. Routes that
    /// pre-date Path 2 (overview, domains, etc. without a brain_id
    /// path segment) load this. The newer `/api/brains/:id/...`
    /// routes use [`AppState::brains`] to resolve the appropriate
    /// registry per request.
    pub registry_path: Arc<String>,
    /// Discovered Brain tree — host + transitively-walked children.
    /// Built once at server startup; immutable for the server's
    /// lifetime. A refresh-on-demand endpoint (e.g.,
    /// `POST /api/brains/refresh`) is queued for v3.5.1 alongside
    /// the other mutation endpoints.
    pub brains: Arc<BrainTree>,
    /// Process-level BrainContext cache shared across all routes.
    /// Avoids re-running the full scoring pipeline on every
    /// request — hot for the multi-widget Overview pages that
    /// fire ~9 parallel requests on first paint. Invalidated by
    /// SSE events on registry / score changes.
    pub cache: Arc<BrainContextCache>,
    /// Broadcast channel for live updates. The /api/events SSE
    /// handler subscribes one receiver per connection. Senders come
    /// from the filesystem watcher spawned at server startup.
    ///
    /// `None` in tests that don't exercise the SSE path. Production
    /// always has a Sender — `serve()` constructs the channel even
    /// if the watcher fails to start (events stop flowing in that
    /// case but the route stays available).
    pub events: Option<broadcast::Sender<DashboardEvent>>,
    /// v3.5.0 — when `true`, mutation endpoints (service start/stop,
    /// sensor refresh) are reachable. When `false`, those endpoints
    /// return 403 with `code: "mutations-disabled"` and the frontend
    /// hides their action buttons. Default: `false` (read-only).
    pub mutations_allowed: bool,
    /// v3.5.0 — in-memory registry of services this dashboard
    /// instance has spawned. Cleared on dashboard restart; spawned
    /// children survive (kill_on_drop is intentionally NOT set).
    pub service_registry: Arc<ServiceRegistry>,
}

impl AppState {
    /// Construct a state without live updates. Used by tests + Phase
    /// 0/1 routes that don't care about events. Re-discovers the
    /// brain tree from the registry path so the multi-Brain routes
    /// still work in tests. Cache uses TTL-only invalidation in
    /// this mode (no broadcast channel to subscribe to).
    ///
    /// Defaults `mutations_allowed: false`. Tests that need mutations
    /// enabled assign `.mutations_allowed = true` directly (the
    /// field is `pub`).
    pub fn new(registry_path: String) -> Self {
        let brains = BrainTree::discover(Path::new(&registry_path));
        let cache = BrainContextCache::new(None);
        Self {
            registry_path: Arc::new(registry_path),
            brains: Arc::new(brains),
            cache: Arc::new(cache),
            events: None,
            mutations_allowed: false,
            service_registry: Arc::new(ServiceRegistry::new()),
        }
    }

    /// Construct a state with a live-update channel. Production path.
    /// The cache subscribes to the broadcast channel so registry /
    /// score events invalidate cached BrainContexts within
    /// milliseconds, in addition to the 30s TTL.
    pub fn with_events(
        registry_path: String,
        events: broadcast::Sender<DashboardEvent>,
        mutations_allowed: bool,
    ) -> Self {
        let brains = BrainTree::discover(Path::new(&registry_path));
        let cache = BrainContextCache::new(Some(&events));
        Self {
            registry_path: Arc::new(registry_path),
            brains: Arc::new(brains),
            cache: Arc::new(cache),
            events: Some(events),
            mutations_allowed,
            service_registry: Arc::new(ServiceRegistry::new()),
        }
    }
}
