//! Shared application state passed to every route handler.
//!
//! Phase 0.3: just the registry path.
//! Phase 2.1: adds the SSE broadcast Sender so the events handler
//! can subscribe a fresh receiver per connection.

use std::sync::Arc;
use tokio::sync::broadcast;

use crate::events::DashboardEvent;

#[derive(Clone)]
pub struct AppState {
    /// Path to the Brain's `brain-registry.json`. Loaded fresh per
    /// request.
    pub registry_path: Arc<String>,
    /// Broadcast channel for live updates. The /api/events SSE
    /// handler subscribes one receiver per connection. Senders come
    /// from the filesystem watcher spawned at server startup.
    ///
    /// `None` in tests that don't exercise the SSE path. Production
    /// always has a Sender — `serve()` constructs the channel even
    /// if the watcher fails to start (events stop flowing in that
    /// case but the route stays available).
    pub events: Option<broadcast::Sender<DashboardEvent>>,
}

impl AppState {
    /// Construct a state without live updates. Used by tests + Phase
    /// 0/1 routes that don't care about events.
    pub fn new(registry_path: String) -> Self {
        Self {
            registry_path: Arc::new(registry_path),
            events: None,
        }
    }

    /// Construct a state with a live-update channel. Production path.
    pub fn with_events(
        registry_path: String,
        events: broadcast::Sender<DashboardEvent>,
    ) -> Self {
        Self {
            registry_path: Arc::new(registry_path),
            events: Some(events),
        }
    }
}
