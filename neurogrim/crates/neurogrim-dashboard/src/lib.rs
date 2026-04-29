//! NeuroGrim v3.4 dashboard server.
//!
//! HTTP server + embedded React frontend that gives humans a visual
//! inspection surface for the Brain. The dashboard is the "show, don't
//! tell" surface for the LSP Brains methodology — the third audience
//! after agents (CLI/MCP) and operators (CLI prose mode).
//!
//! ## Architecture
//!
//! - **Per-Brain primary, federation-aware secondary** — each Brain
//!   ships its own dashboard via `neurogrim ui`, looking at itself
//!   first; federation views fetch peer state via A2A.
//! - **Self-contained** — frontend is built into `frontend/dist/` and
//!   embedded at compile time via `rust-embed`. Users `cargo install
//!   neurogrim-cli` and the dashboard ships with it.
//! - **Read-only by default** — mutation endpoints (sensor refresh,
//!   registry edits) are gated behind `--allow-mutations` opt-in
//!   (planned for v3.5; v3.4 is read-only).
//!
//! ## Phase status (v3.4 in progress)
//!
//! - **Phase 0** (foundation refactor + skeleton) — IN PROGRESS
//! - **Phase 1** (5 pages: Overview, Domains, Domain detail, Federation,
//!   Skills) — pending
//! - **Phase 2** (SSE live updates, hat lens, browser launch, ship) — pending
//!
//! See `audit/v3.4-CHARTER.md` and the workspace `CHANGELOG.md` for
//! the full plan.

pub mod brains;
pub mod bus;
pub mod cache;
pub mod events;
pub mod layout;
pub mod routes;
pub mod services;
pub mod skills;
pub mod state;
pub mod types;

pub use routes::router;
pub use state::AppState;

use anyhow::Result;
use std::net::SocketAddr;
use std::path::Path;

/// Spin up the dashboard HTTP server on the given socket address.
/// Blocks until the server exits (Ctrl+C, kill, or fatal error).
///
/// `allow_mutations` controls whether v3.5+ mutation endpoints
/// (service start/stop, sensor refresh) are reachable. When false,
/// those endpoints return 403 with `code: "mutations-disabled"` and
/// the frontend hides their action buttons.
///
/// Spawns the filesystem watcher so SSE clients connected to
/// `/api/events` receive live updates when CMDBs, the registry,
/// the invocation ledger, or the dashboard layout change.
pub async fn serve(
    addr: SocketAddr,
    registry_path: String,
    allow_mutations: bool,
) -> Result<()> {
    // Derive project_root from the registry path
    // (`<project>/.claude/brain-registry.json`). Canonicalize so
    // notify's absolute event paths can be `strip_prefix`'d cleanly.
    // PathBuf::parent returns `""` (empty path) — not None — when
    // the path has only one component, so we also treat empty paths
    // as cwd before canonicalizing.
    let registry_path_buf = std::path::PathBuf::from(&registry_path);
    let project_root_raw = registry_path_buf
        .parent()
        .and_then(Path::parent)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let project_root_normalized = if project_root_raw.as_os_str().is_empty() {
        std::path::PathBuf::from(".")
    } else {
        project_root_raw
    };
    let project_root = std::fs::canonicalize(&project_root_normalized)
        .unwrap_or(project_root_normalized);

    let events_tx = events::spawn_watcher(project_root);
    let state = AppState::with_events(registry_path, events_tx, allow_mutations);
    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("neurogrim dashboard listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
