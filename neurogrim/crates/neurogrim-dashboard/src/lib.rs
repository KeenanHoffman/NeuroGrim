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

pub mod routes;
pub mod skills;
pub mod state;
pub mod types;

pub use routes::router;
pub use state::AppState;

use anyhow::Result;
use std::net::SocketAddr;

/// Spin up the dashboard HTTP server on the given socket address.
/// Blocks until the server exits (Ctrl+C, kill, or fatal error).
///
/// Phase 0.3: minimal — just `/api/health` + static-asset fallback.
/// Phase 1 expands the route table.
pub async fn serve(addr: SocketAddr, registry_path: String) -> Result<()> {
    let state = AppState::new(registry_path);
    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("neurogrim dashboard listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
