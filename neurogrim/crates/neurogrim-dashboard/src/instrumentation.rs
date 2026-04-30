//! v4.5 — Self-monitoring instrumentation. Records dashboard activity
//! into the TSDB so operators (and the Brain itself) can ask
//! questions like "is the dashboard slowing down" or "is the cache
//! actually helping" *as time-series questions* rather than ad-hoc
//! ones.
//!
//! Five universally-useful series live here:
//!
//! - `request_duration_ms{path, status}` — handler latency.
//! - `cache_event{cache, kind=hit|miss|invalidate}` — counter.
//! - `peer_probe_ms{peer, outcome}` — federation reachability.
//! - `bus_publish{topic, backend}` — counter.
//! - `domain_score{domain}` — auto-ingested from `_neurogrim/score-snapshots`.
//!
//! `request_duration_ms` lands as axum middleware (zero touchpoints in
//! handlers); the rest are direct calls at the relevant sites. All are
//! best-effort — failure to record a metric must never affect the
//! request being measured.

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use neurogrim_core::metrics::Tags;
use std::time::Instant;

use crate::state::AppState;

// ── Path normalization ──────────────────────────────────────────────────

/// Replace cardinality-blowing path segments with placeholders so the
/// `path` tag has bounded distinct values. Without this, tags like
/// `/api/brains/neurogrim/domains/test-health` produce one series per
/// (brain, domain) pair and the cardinality grows with project size.
///
/// Examples:
/// - `/api/brains/neurogrim/domains/test-health` → `/api/brains/:id/domains/:name`
/// - `/api/brains/neurogrim/peers/python-starter/log` → `/api/brains/:id/peers/:peer/log`
/// - `/api/brains/neurogrim/queues/_neurogrim/approvals` → `/api/brains/:id/queues/:topic`
///
/// Static endpoints (`/api/health`, `/api/events`) pass through unchanged.
/// Non-API paths return `"static"` so we don't record per-asset latency
/// (the rust-embed serve path is a separate concern from API health).
pub fn normalize_path(path: &str) -> String {
    if !path.starts_with("/api/") {
        return "static".to_string();
    }
    let rest = &path["/api/".len()..];
    let segments: Vec<&str> = rest.split('/').collect();
    if segments.is_empty() {
        return "/api/".to_string();
    }
    if segments[0] != "brains" || segments.len() < 2 {
        // /api/health, /api/tls-status, /api/events, /api/brains, etc.
        return path.to_string();
    }
    // /api/brains/:id/...
    let mut out = String::from("/api/brains/:id");
    let tail = &segments[2..];
    let mut i = 0;
    while i < tail.len() {
        match tail[i] {
            "domains" => {
                out.push_str("/domains");
                if i + 1 < tail.len() {
                    out.push_str("/:name");
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "peers" => {
                out.push_str("/peers");
                if i + 1 < tail.len() {
                    out.push_str("/:peer");
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "queues" => {
                // Topic names contain `/` (e.g. `_neurogrim/approvals`)
                // — collapse the entire tail past `queues` into `:topic`.
                out.push_str("/queues/:topic");
                break;
            }
            "dashboard-pages" => {
                out.push_str("/dashboard-pages");
                if i + 1 < tail.len() {
                    out.push_str("/:page");
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "config-file" => {
                out.push_str("/config-file");
                if i + 1 < tail.len() {
                    out.push_str("/:file");
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "approvals" => {
                out.push_str("/approvals");
                if i + 1 < tail.len() {
                    out.push_str("/:action_id");
                    i += 2;
                    if i < tail.len() {
                        // /resolve, /retract, etc.
                        out.push('/');
                        out.push_str(tail[i]);
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            "secrets" => {
                out.push_str("/secrets");
                if i + 1 < tail.len() {
                    out.push_str("/:key");
                    i += 2;
                } else {
                    i += 1;
                }
            }
            seg => {
                out.push('/');
                out.push_str(seg);
                i += 1;
            }
        }
    }
    out
}

// ── Middleware: request_duration_ms ─────────────────────────────────────

/// Axum middleware that records handler latency for every `/api/`
/// request. Wired in `routes.rs::router` via
/// `.layer(middleware::from_fn_with_state(state.clone(), record_request_duration))`.
pub async fn record_request_duration(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let started = Instant::now();
    let path = request.uri().path().to_string();
    let response = next.run(request).await;

    // Record only after the response is built so the duration includes
    // the handler's full work. Any panic the next handler raised is
    // caught by axum higher up and we still see the resulting response.
    let duration_ms = started.elapsed().as_secs_f64() * 1000.0;
    let status = response.status().as_u16();

    if let Some(metrics) = &state.metrics {
        let tags = Tags::new()
            .with("path", normalize_path(&path))
            .with("status", status.to_string());
        metrics.record("request_duration_ms", &tags, duration_ms);
    }

    response
}

// ── Direct-call helpers ─────────────────────────────────────────────────

/// Counter increment for a cache event. `kind` should be one of
/// `"hit"`, `"miss"`, `"invalidate"`.
pub fn record_cache_event(state: &AppState, cache: &str, kind: &str) {
    if let Some(metrics) = &state.metrics {
        let tags = Tags::new().with("cache", cache).with("kind", kind);
        metrics.record("cache_event", &tags, 1.0);
    }
}

/// Peer probe latency in ms with outcome (`reachable`, `unreachable`,
/// `timeout`, `disabled`, etc.).
pub fn record_peer_probe(state: &AppState, peer: &str, outcome: &str, duration_ms: f64) {
    if let Some(metrics) = &state.metrics {
        let tags = Tags::new().with("peer", peer).with("outcome", outcome);
        metrics.record("peer_probe_ms", &tags, duration_ms);
    }
}

/// Counter increment for a bus publish event.
pub fn record_bus_publish(state: &AppState, topic: &str, backend: &str) {
    if let Some(metrics) = &state.metrics {
        let tags = Tags::new().with("topic", topic).with("backend", backend);
        metrics.record("bus_publish", &tags, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_static_assets() {
        assert_eq!(normalize_path("/"), "static");
        assert_eq!(normalize_path("/index.html"), "static");
        assert_eq!(normalize_path("/assets/foo.js"), "static");
    }

    #[test]
    fn normalize_path_static_api_endpoints() {
        assert_eq!(normalize_path("/api/health"), "/api/health");
        assert_eq!(normalize_path("/api/tls-status"), "/api/tls-status");
        assert_eq!(normalize_path("/api/events"), "/api/events");
        assert_eq!(normalize_path("/api/brains"), "/api/brains");
    }

    #[test]
    fn normalize_path_brain_id_collapsed() {
        assert_eq!(
            normalize_path("/api/brains/neurogrim/overview"),
            "/api/brains/:id/overview"
        );
        assert_eq!(
            normalize_path("/api/brains/python-starter/hats"),
            "/api/brains/:id/hats"
        );
    }

    #[test]
    fn normalize_path_domain_name_collapsed() {
        assert_eq!(
            normalize_path("/api/brains/neurogrim/domains/test-health"),
            "/api/brains/:id/domains/:name"
        );
        assert_eq!(
            normalize_path("/api/brains/neurogrim/domains"),
            "/api/brains/:id/domains"
        );
    }

    #[test]
    fn normalize_path_peer_name_collapsed() {
        assert_eq!(
            normalize_path("/api/brains/neurogrim/peers/python-starter/log"),
            "/api/brains/:id/peers/:peer/log"
        );
    }

    #[test]
    fn normalize_path_queue_topic_collapsed_even_with_slashes() {
        assert_eq!(
            normalize_path("/api/brains/neurogrim/queues/_neurogrim/approvals"),
            "/api/brains/:id/queues/:topic"
        );
        assert_eq!(
            normalize_path("/api/brains/neurogrim/queues/_neurogrim/score-snapshots"),
            "/api/brains/:id/queues/:topic"
        );
    }

    #[test]
    fn normalize_path_approvals_with_action_id() {
        assert_eq!(
            normalize_path("/api/brains/neurogrim/approvals/abc-123/resolve"),
            "/api/brains/:id/approvals/:action_id/resolve"
        );
    }

    #[test]
    fn normalize_path_unknown_endpoint_passes_through() {
        // Defensive: we don't list every endpoint above; unknown
        // segments are kept verbatim so the tag remains distinguishable.
        assert_eq!(
            normalize_path("/api/brains/neurogrim/some-future-endpoint"),
            "/api/brains/:id/some-future-endpoint"
        );
    }
}
