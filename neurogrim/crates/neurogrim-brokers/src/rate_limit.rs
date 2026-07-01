//! A7 — Rate-limit pre-dispatch subgate.
//!
//! Sliding-window rate limiter that composes via the A4 PreDispatchSubgate
//! trait. Lifts the IDE's `browser/quotas.rs` shape into substrate so any
//! broker can declare per-pipeline rate limits without reinventing the
//! windowing machinery.
//!
//! ## Model
//!
//! Each rate limit is a `(scope_key, bucket, window, limit)` tuple:
//! - **scope_key:** a function from `&Pipeline` to a `String` (the "key"
//!   into the rate-limit table — e.g., `pipeline.id` for per-pipeline
//!   limits, or `pipeline.audit_class` for per-class limits).
//! - **bucket:** label identifying which rate-limit applies (e.g.,
//!   `"browser-navigate"` so the same key can have multiple buckets).
//! - **window:** duration over which `limit` invocations are allowed.
//! - **limit:** max invocations per window.
//!
//! State: per `(key, bucket)` a `VecDeque<Instant>` of recent invocation
//! timestamps. On check: prune timestamps older than `now - window`; if
//! remaining count >= limit, refuse; otherwise push `now` and pass.
//!
//! ## Scope (V0)
//!
//! Single subgate instance with one (scope_key_fn, bucket, window, limit)
//! configuration. Multiple rate limits = register multiple subgates (each
//! gets its own slot per A4).
//!
//! Time source: `std::time::Instant`. Tests use `std::thread::sleep` to
//! simulate window expiry. Migration to `tokio::time::Instant` (+ test-time
//! pause/advance) would require making `check()` async; deferred since the
//! V0 rate-limit check is intentionally a fast sync call on the dispatch
//! hot path. **F9 closure** (Phase A adversarial review) — earlier doc
//! claimed tokio::time; this aligns the doc with the implementation.

use crate::governance::{GovernanceRefusal, PreDispatchSubgate};
use crate::pipeline::{ParamMap, Pipeline};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Function type that derives the rate-limit scope key from a Pipeline +
/// the dispatch's params (**P5a** — params added so limits can key per-pane,
/// not only per-pipeline).
/// Examples:
/// - `|p, _params| p.id.clone()` — per-pipeline limits.
/// - `|p, _params| format!("{:?}", p.audit_class)` — per-audit-class limits.
/// - `|_p, _params| "global".to_string()` — single global limit (rare).
/// - `|p, params| format!("{}::{}", p.id,
///   params.get("pane_id").and_then(|v| v.as_str()).unwrap_or("__global__"))`
///   — **per-pane** limits (browser quotas): each pane gets its own window;
///   pipelines with no `pane_id` fall back to a single shared `__global__`
///   scope rather than mis-keying.
pub type ScopeKeyFn = Box<dyn Fn(&Pipeline, &ParamMap) -> String + Send + Sync>;

pub struct RateLimitSubgate {
    name: String,
    bucket: String,
    window: Duration,
    limit: u32,
    scope_key_fn: ScopeKeyFn,
    state: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl RateLimitSubgate {
    /// Build a new rate-limit subgate. `name` shows up in the
    /// `GovernanceRefusal::Subgate { name, .. }` refusal — operator-readable.
    pub fn new(
        name: impl Into<String>,
        bucket: impl Into<String>,
        window: Duration,
        limit: u32,
        scope_key_fn: ScopeKeyFn,
    ) -> Self {
        Self {
            name: name.into(),
            bucket: bucket.into(),
            window,
            limit,
            scope_key_fn,
            state: Mutex::new(HashMap::new()),
        }
    }

    /// Test/diagnostic helper: count of timestamps currently tracked for the
    /// given key (after window pruning).
    pub fn current_count(&self, key: &str) -> usize {
        let mut state = self.state.lock().expect("rate-limit state poisoned");
        if let Some(window) = state.get_mut(key) {
            let cutoff = Instant::now() - self.window;
            while let Some(&front) = window.front() {
                if front < cutoff {
                    window.pop_front();
                } else {
                    break;
                }
            }
            window.len()
        } else {
            0
        }
    }
}

impl PreDispatchSubgate for RateLimitSubgate {
    fn name(&self) -> &str {
        &self.name
    }

    fn check(&self, pipeline: &Pipeline, params: &ParamMap) -> Result<(), GovernanceRefusal> {
        let key = format!("{}::{}", (self.scope_key_fn)(pipeline, params), self.bucket);
        let now = Instant::now();
        let cutoff = now - self.window;
        let mut state = self.state.lock().expect("rate-limit state poisoned");
        let window = state.entry(key.clone()).or_insert_with(VecDeque::new);
        // Prune timestamps older than the window.
        while let Some(&front) = window.front() {
            if front < cutoff {
                window.pop_front();
            } else {
                break;
            }
        }
        if window.len() as u32 >= self.limit {
            return Err(GovernanceRefusal::Subgate {
                name: self.name.clone(),
                reason: format!(
                    "rate limit exceeded: {} hits in last {}s for `{}` (limit {})",
                    window.len(),
                    self.window.as_secs(),
                    key,
                    self.limit
                ),
            });
        }
        window.push_back(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{AuditClass, EffectClass, Tunability, Visibility};

    fn make_test_pipeline(id: &str) -> Pipeline {
        Pipeline {
            id: id.to_string(),
            visibility: Visibility::Surfaced,
            tunability: Tunability::OperatorOnly,
            audit_class: AuditClass::Capability,
            effect_class: EffectClass::WorldEffect,
            params: serde_json::json!({}),
            preconditions: vec![],
            steps: vec![],
            description: String::new(),
            when_to_use: String::new(),
            bypasses_kill_switch: false,
        }
    }

    #[test]
    fn rate_limit_allows_dispatches_under_limit() {
        let subgate = RateLimitSubgate::new(
            "test-rate-limit",
            "default",
            Duration::from_secs(60),
            3,
            Box::new(|p, _params| p.id.clone()),
        );
        let p = make_test_pipeline("t/x");
        // 3 dispatches in a row — all permitted.
        for _ in 0..3 {
            subgate.check(&p, &ParamMap::new()).unwrap();
        }
    }

    #[test]
    fn rate_limit_refuses_dispatch_over_limit() {
        let subgate = RateLimitSubgate::new(
            "test-rate-limit",
            "default",
            Duration::from_secs(60),
            2,
            Box::new(|p, _params| p.id.clone()),
        );
        let p = make_test_pipeline("t/x");
        subgate.check(&p, &ParamMap::new()).unwrap();
        subgate.check(&p, &ParamMap::new()).unwrap();
        let err = subgate.check(&p, &ParamMap::new()).unwrap_err();
        match err {
            GovernanceRefusal::Subgate { name, reason } => {
                assert_eq!(name, "test-rate-limit");
                assert!(reason.contains("rate limit exceeded"));
            }
            other => panic!("expected Subgate refusal, got {:?}", other),
        }
    }

    #[test]
    fn rate_limit_scopes_independently_per_key() {
        let subgate = RateLimitSubgate::new(
            "test-rate-limit",
            "default",
            Duration::from_secs(60),
            1,
            Box::new(|p, _params| p.id.clone()),
        );
        let p1 = make_test_pipeline("t/one");
        let p2 = make_test_pipeline("t/two");
        // p1 uses its budget; p2 still has its own.
        subgate.check(&p1, &ParamMap::new()).unwrap();
        assert!(subgate.check(&p1, &ParamMap::new()).is_err());
        subgate.check(&p2, &ParamMap::new()).unwrap();
        assert!(subgate.check(&p2, &ParamMap::new()).is_err());
    }

    #[test]
    fn rate_limit_window_expiry_restores_capacity() {
        let subgate = RateLimitSubgate::new(
            "test-rate-limit",
            "default",
            Duration::from_millis(50),
            1,
            Box::new(|p, _params| p.id.clone()),
        );
        let p = make_test_pipeline("t/x");
        subgate.check(&p, &ParamMap::new()).unwrap();
        assert!(subgate.check(&p, &ParamMap::new()).is_err());
        // Wait for the window to expire.
        std::thread::sleep(Duration::from_millis(80));
        // Capacity restored.
        subgate.check(&p, &ParamMap::new()).unwrap();
    }

    #[test]
    fn current_count_returns_active_window_size() {
        let subgate = RateLimitSubgate::new(
            "test-rate-limit",
            "default",
            Duration::from_secs(60),
            5,
            Box::new(|p, _params| p.id.clone()),
        );
        let p = make_test_pipeline("t/x");
        assert_eq!(subgate.current_count("t/x::default"), 0);
        subgate.check(&p, &ParamMap::new()).unwrap();
        subgate.check(&p, &ParamMap::new()).unwrap();
        assert_eq!(subgate.current_count("t/x::default"), 2);
    }

    /// Per-pane scope key derived from `params` — a `pane_id`-aware
    /// scope_key_fn used by both P5a tests below (mirrors the browser-quota
    /// registration in the IDE).
    fn per_pane_key() -> ScopeKeyFn {
        Box::new(|p: &Pipeline, params: &ParamMap| {
            format!(
                "{}::{}",
                p.id,
                params
                    .get("pane_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("__global__")
            )
        })
    }

    #[test]
    fn rate_limit_keys_per_pane_via_params() {
        // P5a — the scope_key_fn now receives the dispatch params, so a
        // browser-style quota keys per-PANE: two panes running the SAME
        // pipeline get independent windows.
        let subgate = RateLimitSubgate::new(
            "browser-quotas-mutate",
            "mutate",
            Duration::from_secs(60),
            1,
            per_pane_key(),
        );
        let p = make_test_pipeline("browser-dom-write/click");
        let mut pane_a = ParamMap::new();
        pane_a.insert("pane_id".to_string(), serde_json::json!("pane-A"));
        let mut pane_b = ParamMap::new();
        pane_b.insert("pane_id".to_string(), serde_json::json!("pane-B"));

        // Pane A spends its single slot; a second A dispatch is refused.
        subgate.check(&p, &pane_a).unwrap();
        assert!(subgate.check(&p, &pane_a).is_err(), "pane A exhausted its window");
        // Pane B is independent — its own window is still open.
        subgate.check(&p, &pane_b).unwrap();
        assert!(subgate.check(&p, &pane_b).is_err(), "pane B window is independent");
    }

    #[test]
    fn rate_limit_missing_pane_id_falls_back_to_global_scope() {
        // P5a — a non-browser pipeline (no pane_id in params) must not
        // mis-key or panic; it collapses to a single shared `__global__`
        // scope and the window still enforces deterministically.
        let subgate = RateLimitSubgate::new(
            "browser-quotas-mutate",
            "mutate",
            Duration::from_secs(60),
            1,
            per_pane_key(),
        );
        let p = make_test_pipeline("work-broker/next");
        subgate.check(&p, &ParamMap::new()).unwrap();
        assert!(subgate.check(&p, &ParamMap::new()).is_err());
    }
}
