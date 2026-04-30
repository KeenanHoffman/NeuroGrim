//! v3.5.0 — service lifecycle management for the dashboard.
//!
//! Power-user feature: start and stop the project's A2A peer
//! services from the dashboard UI without dropping to a terminal.
//! Each spawned child is tracked in an in-memory [`ServiceRegistry`]
//! — no `services.json` persistence in v3.5.0 (the user explicitly
//! accepted in-memory state as good enough; domain-level state is
//! already persistent in CMDBs). On dashboard restart, services
//! from the previous session that are still alive show up as
//! "alive" via the regular federation probe, but the Stop button
//! is disabled because we don't have a [`tokio::process::Child`]
//! handle for them anymore.
//!
//! All start/stop endpoints live behind `--allow-mutations` — see
//! [`crate::state::AppState::mutations_allowed`].
//!
//! # Why no kill_on_drop
//!
//! `tokio::process::Child::kill_on_drop` is explicitly NOT set on
//! spawned children. The user wants services to survive a dashboard
//! restart so they can keep doing their job (servicing federation
//! probes from peers) even when the operator closes the dashboard.
//! Code review reminder: if you ever feel tempted to add
//! `.kill_on_drop(true)`, don't — it breaks the v3.5.0 user
//! contract.

use crate::events::DashboardEvent;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Child;
use tokio::sync::{broadcast, Mutex, RwLock};
use ts_rs::TS;

/// One service this dashboard instance has spawned.
///
/// The `child` field is wrapped in `Arc<Mutex<…>>` because the
/// stop handler needs `&mut self` for `kill()`, while the registry
/// itself needs to stay clonable for `AppState::clone`.
pub struct ServiceHandle {
    pub peer_name: String,
    pub pid: u32,
    pub port: u16,
    pub started_at: DateTime<Utc>,
    pub log_path: PathBuf,
    pub child: Arc<Mutex<Child>>,
}

impl std::fmt::Debug for ServiceHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceHandle")
            .field("peer_name", &self.peer_name)
            .field("pid", &self.pid)
            .field("port", &self.port)
            .field("started_at", &self.started_at)
            .field("log_path", &self.log_path)
            // Skip `child` — tokio's Child doesn't impl Debug.
            .finish()
    }
}

/// In-memory registry of every service currently running under the
/// dashboard's supervision.
///
/// Reads dominate (every federation render checks state) so
/// `RwLock` is the right primitive.
#[derive(Clone, Default)]
pub struct ServiceRegistry {
    handles: Arc<RwLock<HashMap<String, ServiceHandle>>>,
}

impl std::fmt::Debug for ServiceRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceRegistry")
            // Avoid awaiting the RwLock here — Debug must not block.
            .field("handles", &"<rwlock>")
            .finish()
    }
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when a service is currently tracked under `peer_name`.
    pub async fn contains(&self, peer_name: &str) -> bool {
        self.handles.read().await.contains_key(peer_name)
    }

    /// Insert a freshly-spawned handle. Overwrites any prior entry
    /// (callers should check `contains` first to surface 409 on
    /// duplicate-start; this is the lower-level escape hatch).
    pub async fn insert(&self, handle: ServiceHandle) {
        let mut h = self.handles.write().await;
        h.insert(handle.peer_name.clone(), handle);
    }

    /// Remove and return the handle for `peer_name`, or `None` when
    /// not tracked. Caller is responsible for `kill()`-ing the
    /// returned child.
    pub async fn remove(&self, peer_name: &str) -> Option<ServiceHandle> {
        let mut h = self.handles.write().await;
        h.remove(peer_name)
    }

    /// Snapshot of every tracked service (no child handles, just
    /// metadata). Used by `GET /api/brains/:id/services`.
    pub async fn list(&self) -> Vec<ServiceSnapshot> {
        let h = self.handles.read().await;
        h.values()
            .map(|sh| ServiceSnapshot {
                peer_name: sh.peer_name.clone(),
                pid: sh.pid,
                port: sh.port,
                started_at: sh.started_at.to_rfc3339(),
                log_path: sh.log_path.to_string_lossy().to_string(),
            })
            .collect()
    }

    /// Lookup snapshot for a single peer.
    pub async fn snapshot(&self, peer_name: &str) -> Option<ServiceSnapshot> {
        let h = self.handles.read().await;
        h.get(peer_name).map(|sh| ServiceSnapshot {
            peer_name: sh.peer_name.clone(),
            pid: sh.pid,
            port: sh.port,
            started_at: sh.started_at.to_rfc3339(),
            log_path: sh.log_path.to_string_lossy().to_string(),
        })
    }
}

/// Read-side snapshot of a tracked service. Serializable; safe to
/// send over the wire. Does NOT hold the `Child` (caller doesn't
/// need it for display purposes).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ServiceSnapshot {
    pub peer_name: String,
    pub pid: u32,
    pub port: u16,
    /// RFC 3339 timestamp.
    pub started_at: String,
    pub log_path: String,
}

/// Wire-format response for `POST /api/brains/:id/peers/:peer/start`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct StartPeerResponse {
    /// "starting" — the spawn handler returns immediately and the
    /// readiness watcher emits SSE events as the state evolves.
    pub state: String,
    pub peer_name: String,
    pub pid: u32,
    pub port: u16,
}

/// Wire-format response for `POST /api/brains/:id/peers/:peer/stop`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct StopPeerResponse {
    /// "stopped" — child killed, registry entry removed.
    pub state: String,
    pub peer_name: String,
}

/// Wire-format response for `GET /api/brains/:id/services`. Lists
/// every service currently tracked by this dashboard.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ServicesListResponse {
    pub services: Vec<ServiceSnapshot>,
}

/// Wire-format error for the start/stop/probe endpoints. The
/// `code` field carries a stable string the frontend can switch
/// on (rather than parsing the human-readable `error`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ServiceErrorDto {
    pub error: String,
    /// One of: `mutations-disabled`, `peer-not-found`,
    /// `already-running`, `not-running`, `port-conflict`,
    /// `spawn-failed`, `internal`.
    pub code: String,
}

impl ServiceErrorDto {
    pub fn new(code: &str, error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: code.to_string(),
        }
    }
}

/// Send a [`DashboardEvent`] on the broadcast channel, ignoring
/// "no subscribers" errors. Used by the start/stop handlers to
/// notify connected SSE clients.
pub fn broadcast_event(
    events: &Option<broadcast::Sender<DashboardEvent>>,
    ev: DashboardEvent,
) {
    if let Some(tx) = events {
        let _ = tx.send(ev);
    }
}

// ── services.jsonl persistence (S15-C-3 expansion follow-on) ─────────────

/// One on-disk row in `<project>/.claude/brain/services.jsonl`.
///
/// Append-only ledger of terminal service-lifecycle events (started
/// / failed / stopped). The transient "starting" state is NOT logged
/// — only the resolved outcome lands on disk so the timeline reads
/// as a sequence of facts rather than wishes.
///
/// Used by both:
/// - `crate::logs::read_services_log` (Logs page source)
/// - Future v5 work: dashboard-restart recovery (re-attach to
///   already-running peers by reading the most-recent `started`
///   that has no matching `stopped`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct ServiceLogEntry {
    /// RFC3339 timestamp of the event.
    pub ts: String,
    /// One of: `"started"` | `"failed"` | `"stopped"`.
    pub kind: String,
    pub peer_name: String,
    /// PID of the spawned child. Present on `started` + `stopped`;
    /// `None` on `failed` when the spawn itself didn't yield a PID
    /// (port-conflict pre-spawn, etc.).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pid: Option<u32>,
    /// Bound port. Present on `started`; `None` on `failed` /
    /// `stopped` (where it would be redundant or unknown).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub port: Option<u16>,
    /// Failure reason for `failed` entries (port conflict, readiness
    /// timeout, kill failure, etc.). `None` on success outcomes.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reason: Option<String>,
}

/// On-disk path for the legacy services ledger. Kept for backward
/// compatibility with projects that haven't yet emitted a service
/// event under the new (SQLite-bus) writer.
pub fn services_log_path(project_root: &std::path::Path) -> PathBuf {
    project_root
        .join(".claude")
        .join("brain")
        .join("services.jsonl")
}

/// On-disk path for the SQLite-backed services bus topic.
pub fn services_topic_sqlite_path(project_root: &std::path::Path) -> PathBuf {
    project_root
        .join(".claude")
        .join("brain")
        .join("queues")
        .join("_neurogrim")
        .join("services.sqlite")
}

/// Append a service lifecycle event to the `_neurogrim/services`
/// SQLite bus topic.
///
/// Best-effort: failures (parent dir missing, disk full, permissions)
/// are logged via `tracing` but NOT propagated. Service lifecycle
/// must complete even if the audit ledger can't be written;
/// otherwise a degraded filesystem could cascade into operators
/// being unable to start/stop their peers.
///
/// On first call for a project, auto-migrates any existing
/// `services.jsonl` into the SQLite topic so operators don't lose
/// audit history when upgrading.
pub fn append_service_event(
    project_root: &std::path::Path,
    entry: &ServiceLogEntry,
) {
    use neurogrim_core::queue::{QueueMessage, SERVICES_TOPIC};
    use neurogrim_core::queue_backend::{QueueBackend, SqliteBackend};

    let sqlite_path = services_topic_sqlite_path(project_root);
    if let Some(parent) = sqlite_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                "services topic parent dir create failed at {}: {e}",
                parent.display()
            );
            return;
        }
    }

    let needs_migration = !sqlite_path.exists();
    let mut backend = match SqliteBackend::open(&sqlite_path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                "services topic backend open failed at {}: {e}",
                sqlite_path.display()
            );
            return;
        }
    };
    if needs_migration {
        let json_path = services_log_path(project_root);
        migrate_services_jsonl(&mut backend, &json_path);
    }

    let payload = match serde_json::to_value(entry) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                "services event serialize failed for kind={}: {e}",
                entry.kind
            );
            return;
        }
    };
    let msg = QueueMessage::new(SERVICES_TOPIC, payload);
    if let Err(e) = backend.append(&msg) {
        tracing::warn!(
            "services event append to bus failed at {}: {e}",
            sqlite_path.display()
        );
    }
}

/// Migrate the legacy `services.jsonl` into the SQLite bus topic.
/// Called once on first write per project. Silent on missing /
/// unreadable JSONL — the topic just starts fresh.
fn migrate_services_jsonl(
    backend: &mut neurogrim_core::queue_backend::SqliteBackend,
    json_path: &std::path::Path,
) {
    use neurogrim_core::queue::{QueueMessage, SERVICES_TOPIC};
    use neurogrim_core::queue_backend::QueueBackend;

    let text = match std::fs::read_to_string(json_path) {
        Ok(t) => t,
        Err(_) => return,
    };
    let mut migrated = 0usize;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let msg = QueueMessage::new(SERVICES_TOPIC, entry);
        if let Err(e) = backend.append(&msg) {
            tracing::warn!("services migration: append failed for entry {migrated}: {e}");
            break;
        }
        migrated += 1;
    }
    if migrated > 0 {
        tracing::info!(
            "services log migrated {migrated} entries from {} to SQLite bus topic",
            json_path.display()
        );
    }
}

/// Convenience: append a `"started"` entry. Call this right before
/// broadcasting `DashboardEvent::ServiceStarted` so the on-disk
/// ledger and the SSE channel stay in lockstep.
pub fn log_service_started(
    project_root: &std::path::Path,
    peer_name: &str,
    pid: u32,
    port: u16,
) {
    append_service_event(
        project_root,
        &ServiceLogEntry {
            ts: Utc::now().to_rfc3339(),
            kind: "started".to_string(),
            peer_name: peer_name.to_string(),
            pid: Some(pid),
            port: Some(port),
            reason: None,
        },
    );
}

/// Convenience: append a `"failed"` entry. Pass `pid` when known
/// (post-spawn failures); `None` when the spawn itself didn't yield
/// a PID (port conflict pre-spawn, etc.).
pub fn log_service_failed(
    project_root: &std::path::Path,
    peer_name: &str,
    reason: &str,
    pid: Option<u32>,
) {
    append_service_event(
        project_root,
        &ServiceLogEntry {
            ts: Utc::now().to_rfc3339(),
            kind: "failed".to_string(),
            peer_name: peer_name.to_string(),
            pid,
            port: None,
            reason: Some(reason.to_string()),
        },
    );
}

/// Convenience: append a `"stopped"` entry.
pub fn log_service_stopped(
    project_root: &std::path::Path,
    peer_name: &str,
    pid: u32,
) {
    append_service_event(
        project_root,
        &ServiceLogEntry {
            ts: Utc::now().to_rfc3339(),
            kind: "stopped".to_string(),
            peer_name: peer_name.to_string(),
            pid: Some(pid),
            port: None,
            reason: None,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registry_starts_empty() {
        let reg = ServiceRegistry::new();
        assert!(reg.list().await.is_empty());
        assert!(!reg.contains("anything").await);
        assert!(reg.snapshot("anything").await.is_none());
    }

    #[tokio::test]
    async fn snapshot_reflects_listed_handle_metadata() {
        // We can't construct a real `ServiceHandle` without a
        // spawned `Child`. Instead, spawn a no-op binary that
        // exits quickly (cargo is portable across CI envs and
        // already on the workspace `PATH`), insert, list, remove.
        let reg = ServiceRegistry::new();
        let mut child = tokio::process::Command::new("cargo")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn cargo --version");
        let pid = child.id().unwrap_or(0);
        let handle = ServiceHandle {
            peer_name: "test-peer".into(),
            pid,
            port: 65000,
            started_at: Utc::now(),
            log_path: PathBuf::from("/tmp/x.log"),
            child: Arc::new(Mutex::new(child)),
        };
        reg.insert(handle).await;

        assert!(reg.contains("test-peer").await);
        let snaps = reg.list().await;
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].peer_name, "test-peer");
        assert_eq!(snaps[0].port, 65000);
        let single = reg.snapshot("test-peer").await.unwrap();
        assert_eq!(single.peer_name, "test-peer");
        assert_eq!(single.pid, pid);

        // Reap the child to avoid leaving a zombie.
        let removed = reg.remove("test-peer").await.expect("removed");
        let _ = removed.child.lock().await.wait().await;
        assert!(!reg.contains("test-peer").await);
    }

    #[test]
    fn service_error_dto_carries_stable_code() {
        let e = ServiceErrorDto::new("port-conflict", "port 51234 already bound");
        assert_eq!(e.code, "port-conflict");
        assert!(e.error.contains("51234"));
    }
}
