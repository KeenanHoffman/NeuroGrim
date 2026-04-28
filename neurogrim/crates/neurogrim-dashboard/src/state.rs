//! Shared application state passed to every route handler.
//!
//! Phase 0.3: just the registry path. Phase 1 adds cached
//! `BrainContext`-equivalent data (registry parsed, last AgentOutput).
//! Phase 2 adds the file watcher + SSE broadcast channel.

use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    /// Path to the Brain's `brain-registry.json`. Loaded fresh per
    /// request in Phase 0.3 (cheap; <50ms). Phase 1 will cache
    /// `BrainContext` and invalidate on file-watcher events.
    pub registry_path: Arc<String>,
}

impl AppState {
    pub fn new(registry_path: String) -> Self {
        Self {
            registry_path: Arc::new(registry_path),
        }
    }
}
