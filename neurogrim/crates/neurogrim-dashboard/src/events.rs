//! Phase 2.1 — live updates via SSE + filesystem watcher.
//!
//! ## Architecture
//!
//! 1. A `notify::RecommendedWatcher` watches the project root.
//! 2. Filesystem events flow through a `tokio::mpsc` (the bridge from
//!    notify's sync callback into async land) and into the
//!    `classify_event` function, which maps file paths to typed
//!    `DashboardEvent` variants.
//! 3. Classified events are broadcast over a
//!    `tokio::sync::broadcast` channel held in `AppState`.
//! 4. The `/api/events` SSE handler subscribes a fresh receiver per
//!    connection and streams each event as `data: <json>` lines.
//!
//! ## What we listen for
//!
//! | File pattern                                   | Event           |
//! |------------------------------------------------|-----------------|
//! | `.claude/brain-registry.json`                  | RegistryChanged |
//! | `.claude/*-cmdb.json`                          | ScoreChanged    |
//! | `.claude/brain/score-history.json`             | ScoreChanged    |
//! | `.claude/brain/invocation-ledger.jsonl`        | SkillInvoked    |
//!
//! Anything else is ignored. We filter at the source so the broadcast
//! channel stays small and the frontend only invalidates queries that
//! could actually have changed.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::sync::broadcast;

/// One event the dashboard emits to live clients. Stringly-typed
/// `kind` field on the wire for forward compatibility (a frontend
/// running an older bundle against a newer server can still parse and
/// ignore unknown kinds).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DashboardEvent {
    /// `brain-registry.json` was modified — config (weights, peers,
    /// scoring policy) may have changed.
    RegistryChanged,
    /// A CMDB or score-history file was modified — scores or sparklines
    /// may have changed. The optional `domain` is the kebab-case domain
    /// id when we can derive it from the filename
    /// (`<name>-cmdb.json`); `None` for `score-history.json` and
    /// otherwise.
    ScoreChanged {
        domain: Option<String>,
    },
    /// The invocation ledger was appended — skill hygiene or recency
    /// may have shifted.
    SkillInvoked,
}

/// Classify a filesystem path into a `DashboardEvent`. Paths are
/// resolved relative to the project root so the matcher works
/// regardless of how the watcher reports the path (absolute on most
/// platforms, sometimes relative).
///
/// Returns `None` for files we don't care about — keeps the broadcast
/// channel quiet on routine writes (.git/, build artifacts, etc.).
pub fn classify_event(path: &Path, project_root: &Path) -> Option<DashboardEvent> {
    let rel = path.strip_prefix(project_root).unwrap_or(path);
    let rel_str = rel.to_string_lossy().replace('\\', "/");

    if rel_str == ".claude/brain-registry.json" {
        return Some(DashboardEvent::RegistryChanged);
    }
    if rel_str == ".claude/brain/score-history.json" {
        return Some(DashboardEvent::ScoreChanged { domain: None });
    }
    if rel_str == ".claude/brain/invocation-ledger.jsonl" {
        return Some(DashboardEvent::SkillInvoked);
    }
    if let Some(file_name) = rel.file_name().and_then(|n| n.to_str()) {
        // `.claude/<domain>-cmdb.json` lives directly under
        // `.claude/`, NOT in `.claude/brain/`. Be specific about the
        // parent dir so we don't match e.g. `.claude/brain/foo-cmdb.json`
        // (no such file exists today, but future drift would silently
        // re-classify it without this check).
        let parent_ok = rel
            .parent()
            .and_then(|p| p.to_str())
            .map(|s| s.replace('\\', "/") == ".claude")
            .unwrap_or(false);
        if parent_ok && file_name.ends_with("-cmdb.json") {
            let domain = file_name
                .strip_suffix("-cmdb.json")
                .map(|s| s.to_string());
            return Some(DashboardEvent::ScoreChanged { domain });
        }
    }
    None
}

/// Spawn the filesystem watcher. Runs forever in a background tokio
/// task; the returned `broadcast::Sender` is cloned into `AppState`
/// for the SSE handler to subscribe against.
///
/// Errors during watcher setup are logged + the function returns a
/// channel anyway — a missing watcher must NOT crash the dashboard.
/// Live updates fall back to query polling if the watcher fails to
/// start (Phase 2.x can add a `/api/events?status` endpoint that
/// surfaces this to the operator).
pub fn spawn_watcher(project_root: PathBuf) -> broadcast::Sender<DashboardEvent> {
    use notify::{RecursiveMode, Watcher};
    let (bcast_tx, _) = broadcast::channel::<DashboardEvent>(64);
    let bcast_clone = bcast_tx.clone();

    let (fs_tx, mut fs_rx) =
        tokio::sync::mpsc::unbounded_channel::<notify::Result<notify::Event>>();

    let mut watcher = match notify::recommended_watcher(move |res| {
        let _ = fs_tx.send(res);
    }) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(
                "dashboard live updates disabled — failed to create watcher: {e}"
            );
            return bcast_tx;
        }
    };

    let claude_dir = project_root.join(".claude");
    if let Err(e) = watcher.watch(&claude_dir, RecursiveMode::Recursive) {
        tracing::warn!(
            "dashboard live updates disabled — failed to watch {:?}: {e}",
            claude_dir
        );
        return bcast_tx;
    }
    tracing::info!("dashboard watching {:?} for live updates", claude_dir);

    tokio::spawn(async move {
        // Move the watcher into the task so it lives as long as the
        // server. Dropping the watcher cancels all subscriptions.
        let _watcher = watcher;
        while let Some(res) = fs_rx.recv().await {
            let event = match res {
                Ok(e) => e,
                Err(e) => {
                    tracing::debug!("watcher reported error event: {e}");
                    continue;
                }
            };
            // notify groups multiple paths per event (rename across
            // dirs reports both old + new). Iterate; broadcast each
            // distinct DashboardEvent. If no receivers are subscribed
            // (no SSE clients connected), `send` errors silently.
            for path in &event.paths {
                if let Some(de) = classify_event(path, &project_root) {
                    let _ = bcast_clone.send(de);
                }
            }
        }
    });

    bcast_tx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn classifies_registry_change() {
        let root = PathBuf::from("/proj");
        let path = PathBuf::from("/proj/.claude/brain-registry.json");
        assert_eq!(
            classify_event(&path, &root),
            Some(DashboardEvent::RegistryChanged)
        );
    }

    #[test]
    fn classifies_score_history_change() {
        let root = PathBuf::from("/proj");
        let path = PathBuf::from("/proj/.claude/brain/score-history.json");
        assert_eq!(
            classify_event(&path, &root),
            Some(DashboardEvent::ScoreChanged { domain: None })
        );
    }

    #[test]
    fn classifies_cmdb_change_with_domain_name() {
        let root = PathBuf::from("/proj");
        let path = PathBuf::from("/proj/.claude/test-health-cmdb.json");
        assert_eq!(
            classify_event(&path, &root),
            Some(DashboardEvent::ScoreChanged {
                domain: Some("test-health".to_string())
            })
        );
    }

    #[test]
    fn classifies_invocation_ledger_change() {
        let root = PathBuf::from("/proj");
        let path = PathBuf::from("/proj/.claude/brain/invocation-ledger.jsonl");
        assert_eq!(
            classify_event(&path, &root),
            Some(DashboardEvent::SkillInvoked)
        );
    }

    #[test]
    fn ignores_unrelated_paths() {
        let root = PathBuf::from("/proj");
        assert_eq!(
            classify_event(&PathBuf::from("/proj/Cargo.toml"), &root),
            None
        );
        assert_eq!(
            classify_event(&PathBuf::from("/proj/src/lib.rs"), &root),
            None
        );
        // A nested directory file with `-cmdb.json` suffix that's
        // NOT directly under `.claude/` must not match — protects
        // against future structural drift accidentally re-classifying
        // unrelated files.
        assert_eq!(
            classify_event(
                &PathBuf::from("/proj/.claude/brain/test-cmdb.json"),
                &root
            ),
            None
        );
    }

    #[test]
    fn classifies_with_relative_path() {
        // notify sometimes reports relative paths on Linux; the
        // matcher must work either way.
        let root = PathBuf::from(".");
        let path = PathBuf::from(".claude/brain-registry.json");
        assert_eq!(
            classify_event(&path, &root),
            Some(DashboardEvent::RegistryChanged)
        );
    }

    #[tokio::test]
    async fn spawn_watcher_returns_channel_even_when_dir_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("nonexistent");
        let tx = spawn_watcher(root);
        // Channel must be functional even when the watcher fails
        // to start — fall back to polling at the route level.
        let _rx = tx.subscribe();
        // Smoke-send, no-op when no receivers subscribed before now.
        let _ = tx.send(DashboardEvent::RegistryChanged);
    }

    #[tokio::test]
    async fn spawn_watcher_emits_events_when_files_change() {
        // End-to-end: write a file in .claude/, expect an event on
        // the broadcast channel within a generous timeout. notify
        // can take ~100-500ms to deliver depending on the OS.
        let tmp = tempfile::tempdir().unwrap();
        let claude = tmp.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        // Pre-create the file so notify is watching the right inode
        // — some platforms (macOS especially) don't fire events for
        // files that didn't exist at watcher-start time.
        let registry = claude.join("brain-registry.json");
        std::fs::write(&registry, "{}").unwrap();

        let tx = spawn_watcher(tmp.path().to_path_buf());
        let mut rx = tx.subscribe();

        // Small delay so the watcher is fully wired before the write.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        std::fs::write(&registry, "{\"updated\": true}").unwrap();

        // Wait up to 3s for the event. The body retries with a
        // recv-or-timeout pattern — we just want to see at least
        // one RegistryChanged within the window.
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                loop {
                    match rx.recv().await {
                        Ok(DashboardEvent::RegistryChanged) => return true,
                        Ok(_) => continue, // unrelated event; keep waiting
                        Err(_) => return false,
                    }
                }
            })
            .await;
        assert!(
            matches!(result, Ok(true)),
            "expected RegistryChanged event within 3s"
        );
    }
}
