//! `neurogrim ui` — launch the v3.4 dashboard server.
//!
//! Spawns an HTTP server (axum) on `127.0.0.1:<port>` (default 8420)
//! that serves the embedded React frontend + JSON API. When `--no-browser`
//! is omitted the dashboard tries to open the URL in the user's default
//! browser; the URL is always printed to stderr as a fallback (works on
//! WSL, headless Linux, BROWSER=foo overrides).
//!
//! Phase 0.3: minimal — `/api/health` + static-asset serving + browser
//! launch. Phase 1 expands the route table to power 5 pages.

use anyhow::{Context, Result};
use std::net::SocketAddr;

pub async fn run(
    registry_path: String,
    port: u16,
    bind: String,
    no_browser: bool,
) -> Result<()> {
    let addr: SocketAddr = format!("{bind}:{port}")
        .parse()
        .with_context(|| format!("invalid bind/port: {bind}:{port}"))?;
    let url = format!("http://{bind}:{port}/");

    eprintln!("✦ NeuroGrim Dashboard");
    eprintln!("  Registry:  {}", registry_path);
    eprintln!("  Listening: {}", url);
    eprintln!();

    // Browser launch — best-effort, always print URL to stderr.
    // Honors BROWSER env var (webbrowser crate's default behavior).
    if !no_browser {
        match webbrowser::open(&url) {
            Ok(_) => eprintln!("  Opened in your default browser."),
            Err(e) => eprintln!(
                "  Could not open browser ({e}); navigate manually to the \
                 URL above. (Pass --no-browser to suppress this attempt.)"
            ),
        }
    } else {
        eprintln!("  --no-browser: navigate manually to the URL above.");
    }
    eprintln!();
    eprintln!("Press Ctrl+C to stop the server.");

    neurogrim_dashboard::serve(addr, registry_path).await
}
