//! HTTP route definitions.
//!
//! Phase 0.3: `/api/health` + static-asset fallback.
//! Phase 1: per-page endpoints (`/api/agent`, `/api/domains`,
//! `/api/domains/:name`, `/api/federation`, `/api/skills`).
//! Phase 2: `/api/events` (SSE live updates).

use axum::{
    extract::State,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use neurogrim_core::types::TrajectoryClassification;
use neurogrim_mcp::context::BrainContext;
use neurogrim_mcp::prose::first_sentence;
use rust_embed::RustEmbed;
use std::path::Path;

use crate::state::AppState;
use crate::types::{
    DomainSignalDto, HealthResponse, OverviewResponse, RecommendationDto,
};

/// Frontend bundle embedded at compile time. Built by `npm run build`
/// in `frontend/`. Empty during Phase 0 setup until the frontend has
/// been built at least once — `static_handler` falls back to an
/// inline placeholder page so the boot story is debuggable.
#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/frontend/dist"]
struct FrontendAssets;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/overview", get(overview))
        .fallback(static_handler)
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        registry_path: state.registry_path.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// `GET /api/overview` — landing-page summary for the Phase 1.1
/// Overview page. Loads a fresh `BrainContext` (registry + scoring
/// pipeline run) on every call; Phase 2.1 will add caching with
/// SSE-driven invalidation.
async fn overview(State(state): State<AppState>) -> Response {
    let ctx = match BrainContext::load(&state.registry_path, None, None).await {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("failed to load BrainContext: {e:#}")
                })),
            )
                .into_response();
        }
    };

    Json(build_overview(&ctx)).into_response()
}

/// Pure conversion: BrainContext → OverviewResponse. Extracted so
/// the route's failure handling stays simple AND the unit test can
/// exercise the mapping logic against a constructed context without
/// the http layer.
fn build_overview(ctx: &BrainContext) -> OverviewResponse {
    let registry = &ctx.registry;
    let agent_output = &ctx.agent_output;

    let project_label = if !registry.meta.description.is_empty() {
        first_sentence(&registry.meta.description, 80)
    } else {
        Path::new(&*ctx.project_root.to_string_lossy())
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("(unnamed)")
            .to_string()
    };

    let domain_count = registry.config.domain_weights.len() as u32;
    let weighted_count = registry
        .config
        .domain_weights
        .values()
        .filter(|w| **w > 0.0)
        .count() as u32;
    let advisory_count = domain_count.saturating_sub(weighted_count);

    // All-advisory Brain → score is structurally meaningless; surface
    // None and let the frontend render "N/A (observe-only posture)"
    // (matches the CLI's `agent --prose` behavior).
    let is_all_advisory = weighted_count == 0 && domain_count > 0;
    let score = if is_all_advisory {
        None
    } else {
        Some(agent_output.score)
    };
    let confidence = if is_all_advisory {
        None
    } else {
        Some(agent_output.unified_confidence)
    };

    let (trajectory_class, trajectory_velocity, trajectory_samples) =
        match &agent_output.trajectory {
            Some(t) => (
                classification_string(&t.classification).to_string(),
                t.velocity,
                t.samples as u32,
            ),
            None => ("no-data".to_string(), 0.0, 0),
        };

    // Top 3 strongest signals (by effective_score desc, then weight desc, then name asc).
    let mut signals: Vec<(&String, u8, u8, f64)> = agent_output
        .domains
        .iter()
        .map(|(k, d)| (k, d.effective_score, d.confidence, d.weight))
        .collect();
    signals.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.0.cmp(b.0))
    });
    let strongest_signals: Vec<DomainSignalDto> = signals
        .iter()
        .take(3)
        .map(|(name, eff, conf, weight)| DomainSignalDto {
            name: (*name).clone(),
            display_name: registry
                .config
                .principle_map
                .get(*name)
                .cloned()
                .unwrap_or_else(|| (*name).to_string()),
            effective_score: *eff,
            confidence: *conf,
            weight: *weight,
        })
        .collect();

    let top_recommendations: Vec<RecommendationDto> = agent_output
        .top_recommendations
        .iter()
        .take(3)
        .map(|r| RecommendationDto {
            domain: r.domain.clone(),
            gate: r.gate.clone(),
            status: r.status.clone(),
            command: r.command.clone(),
            description: r.description.clone().unwrap_or_default(),
        })
        .collect();

    let federation_peer_count = registry
        .config
        .extra
        .get("children")
        .and_then(|v| v.as_object())
        .map(|m| m.len() as u32)
        .unwrap_or(0);

    OverviewResponse {
        project_label,
        project_root: ctx.project_root.to_string_lossy().to_string(),
        domain_count,
        weighted_count,
        advisory_count,
        score,
        confidence,
        trajectory_class,
        trajectory_velocity,
        trajectory_samples,
        top_recommendations,
        strongest_signals,
        federation_peer_count,
    }
}

/// Map `TrajectoryClassification` to a stable wire-format string.
/// Frontend mirrors this set in a TS union. Stringly-typed at the
/// wire keeps the JSON small + frontend-debuggable; the Rust enum
/// stays the source of truth.
fn classification_string(c: &TrajectoryClassification) -> &'static str {
    match c {
        TrajectoryClassification::Improving => "improving",
        TrajectoryClassification::Degrading => "degrading",
        TrajectoryClassification::Stable => "stable",
        TrajectoryClassification::Volatile => "volatile",
        TrajectoryClassification::NoData => "no-data",
    }
}

/// Serve embedded frontend assets; fall back to `index.html` for
/// client-side routing (TanStack Router uses pushState; refreshing
/// `/domains/foo` should serve `index.html` and let the frontend
/// resolve the route).
async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/').to_string();
    let path = if path.is_empty() { "index.html".to_string() } else { path };

    if let Some(content) = FrontendAssets::get(&path) {
        let mime = guess_mime(&path);
        return ([(header::CONTENT_TYPE, mime)], content.data).into_response();
    }

    // SPA fallback: any unknown path serves index.html so client
    // routing works on refresh / direct-link.
    if let Some(index) = FrontendAssets::get("index.html") {
        return (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            index.data,
        )
            .into_response();
    }

    // Phase 0 fallback when no frontend has been built yet — the
    // boot story stays debuggable. Once `npm run build` produces a
    // bundle, this branch is unreachable.
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>NeuroGrim Dashboard — boot</title></head>
<body style="font-family: system-ui; max-width: 40em; margin: 4em auto; line-height: 1.5;">
  <h1>NeuroGrim Dashboard</h1>
  <p>The Rust server is running, but no frontend bundle has been embedded
     yet. The dashboard is in Phase 0 of its v3.4 boot — Rust API works,
     UI is next.</p>
  <p>Try the API: <a href="/api/health"><code>/api/health</code></a></p>
</body></html>"#,
    )
        .into_response()
}

/// Tiny MIME-type guesser covering the file types a Vite build emits.
/// Avoids pulling in the full `mime_guess` crate (~100 KB) for ten
/// extensions.
fn guess_mime(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "application/javascript; charset=utf-8"
    } else if lower.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if lower.ends_with(".json") {
        "application/json; charset=utf-8"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else if lower.ends_with(".woff2") {
        "font/woff2"
    } else if lower.ends_with(".woff") {
        "font/woff"
    } else if lower.ends_with(".map") {
        "application/json; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        AppState::new(".claude/brain-registry.json".to_string())
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let app = router(test_state());
        let req = Request::builder()
            .uri("/api/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["ok"], true);
        assert!(v["version"].is_string());
    }

    #[tokio::test]
    async fn unknown_path_falls_back_to_html() {
        let app = router(test_state());
        let req = Request::builder()
            .uri("/some/spa/route")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.starts_with("text/html"), "got content-type: {ct}");
    }

    #[test]
    fn guess_mime_covers_common_vite_outputs() {
        assert_eq!(guess_mime("index.html"), "text/html; charset=utf-8");
        assert_eq!(guess_mime("assets/main-abc123.js"), "application/javascript; charset=utf-8");
        assert_eq!(guess_mime("assets/index.css"), "text/css; charset=utf-8");
        assert_eq!(guess_mime("favicon.svg"), "image/svg+xml");
        assert_eq!(guess_mime("font.woff2"), "font/woff2");
    }
}
