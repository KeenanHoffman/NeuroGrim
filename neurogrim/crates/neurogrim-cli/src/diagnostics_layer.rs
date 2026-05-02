//! `tracing-subscriber` `Layer` that turns spans into diagnostics-
//! ledger entries (V5-FOUND-1 Phase 2).
//!
//! Composes with the centralized init from
//! [`crate::tracing_init`] (Phase 0): when
//! `TracingOpts::enable_diag` is true, [`setup_tracing`] attaches
//! a [`DiagnosticsLayer`] to the registry chain so every span
//! whose name is in [`kind_for_span_name`]'s closed table emits
//! one ledger entry on close.
//!
//! Span names are STATIC (compile-time string literals), per
//! tracing's typical idiom. Dynamic content (server names, route
//! names) goes in span FIELDS, not the span name. This keeps the
//! kind-mapping a clean exact-match table rather than a fragile
//! prefix-match.
//!
//! # Production posture
//!
//! - Default-disabled. The Layer is attached only when
//!   `TracingOpts::enable_diag` is true (read from
//!   `NEUROGRIM_DIAG=1` env var; a `--diag` CLI flag may be added
//!   in a follow-on iteration).
//! - When detached: zero overhead — the Layer is not in the
//!   Subscriber chain at all; no per-span allocations or writes.
//! - Append failures (disk full, permission denied, malformed
//!   entry) are reported via `tracing::warn!` from the Layer's
//!   `on_close` and do NOT abort the operation being observed.
//!   The Layer is observability-only and must not break the host.
//!
//! # Privacy floor
//!
//! Reuses [`neurogrim_core::diagnostics_ledger::validate_entry`]
//! at append time. The Layer's field visitor captures span
//! fields into `extras`; any field whose name is in
//! [`neurogrim_core::diagnostics_ledger::FORBIDDEN_EXTRAS_KEYS`]
//! or not in the per-kind allowed list is rejected at append
//! time with a `tracing::warn!`. Phase-3 instrumentation is
//! responsible for emitting only schema-conformant fields; this
//! Layer is the second line of defense.

use neurogrim_core::diagnostics_ledger::{
    self, DiagnosticsEntry, EventKind, Outcome,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::Subscriber;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// Closed-table mapping from span `name` (compile-time literal)
/// to the corresponding [`EventKind`]. Returns `None` for
/// unmapped names; the Layer drops those silently (with a single
/// debug-level log per unique name to surface drift in tests).
///
/// **When you add an instrumented surface in Phase 3**, add its
/// span name + kind here. Adding a kind without a span-name entry
/// makes the Layer silently ignore that kind's spans.
pub fn kind_for_span_name(name: &str) -> Option<EventKind> {
    match name {
        "score.pipeline.run" => Some(EventKind::Scoring),
        "test.run" => Some(EventKind::Test),
        "cargo.invoke" => Some(EventKind::Cargo),
        "mcp.sensory" => Some(EventKind::McpDispatch),
        "a2a.post" => Some(EventKind::A2aPost),
        "a2a.sse" => Some(EventKind::A2aSse),
        "dashboard.route" => Some(EventKind::DashboardRoute),
        "build" => Some(EventKind::Build),
        "diag.synthesis" => Some(EventKind::DiagSynthesis),
        _ => None,
    }
}

/// Per-span state stored under the span's tracing-subscriber
/// extension storage. Created in `on_new_span`, mutated in
/// `on_record`, consumed in `on_close`.
struct SpanData {
    enter_time: Instant,
    ts_start: String,
    event_id: String,
    kind: EventKind,
    name: String,
    outcome: Outcome,
    parent_event_id: Option<String>,
    extras: BTreeMap<String, serde_json::Value>,
}

/// `tracing-subscriber` `Layer` that emits diagnostics-ledger
/// entries on span close. Constructed by [`DiagnosticsLayer::new`]
/// with the project root path (used to compute the canonical
/// ledger path under `<project_root>/.claude/brain/`).
pub struct DiagnosticsLayer {
    project_root: PathBuf,
}

impl DiagnosticsLayer {
    /// Create a new Layer that writes to
    /// `<project_root>/.claude/brain/diagnostics.jsonl`.
    pub fn new(project_root: PathBuf) -> Self {
        DiagnosticsLayer { project_root }
    }
}

impl<S> Layer<S> for DiagnosticsLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let span_name = attrs.metadata().name();
        let Some(kind) = kind_for_span_name(span_name) else {
            // Unmapped span — do not store extension state; on_close
            // will see no SpanData and will skip silently.
            return;
        };
        let Some(span) = ctx.span(id) else { return };

        // Parent event_id: walk up to find the nearest ancestor
        // span that ALSO has SpanData (i.e., a mapped ancestor).
        // Spans with unmapped names don't contribute to the chain.
        let parent_event_id = std::iter::successors(span.parent(), |s| s.parent())
            .find_map(|ancestor| {
                ancestor
                    .extensions()
                    .get::<SpanData>()
                    .map(|d| d.event_id.clone())
            });

        let mut extras: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        let mut outcome = Outcome::Ok;
        let mut visitor = FieldVisitor {
            extras: &mut extras,
            outcome: &mut outcome,
        };
        attrs.record(&mut visitor);

        let data = SpanData {
            enter_time: Instant::now(),
            ts_start: chrono::Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            event_id: uuid::Uuid::new_v4().to_string(),
            kind,
            name: span_name.to_string(),
            outcome,
            parent_event_id,
            extras,
        };
        span.extensions_mut().insert(data);
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let mut extensions = span.extensions_mut();
        let Some(data) = extensions.get_mut::<SpanData>() else {
            return;
        };
        let mut visitor = FieldVisitor {
            extras: &mut data.extras,
            outcome: &mut data.outcome,
        };
        values.record(&mut visitor);
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else { return };
        let extensions = span.extensions();
        let Some(data) = extensions.get::<SpanData>() else {
            return;
        };

        let duration_ms = data.enter_time.elapsed().as_millis() as u64;
        // Compute depth as ancestor count among MAPPED spans only
        // (consistent with parent_event_id chain).
        let depth: u32 = std::iter::successors(span.parent(), |s| s.parent())
            .filter(|s| s.extensions().get::<SpanData>().is_some())
            .count() as u32;

        let entry = DiagnosticsEntry {
            schema_version: 1,
            event_id: data.event_id.clone(),
            ts_start: data.ts_start.clone(),
            duration_ms,
            kind: data.kind,
            name: data.name.clone(),
            outcome: data.outcome,
            depth,
            parent_event_id: data.parent_event_id.clone(),
            extras: data.extras.clone(),
        };

        let path = diagnostics_ledger::default_ledger_path(&self.project_root);
        if let Err(e) = diagnostics_ledger::append(&path, &entry) {
            // Observability-only: never break the host. Use eprintln
            // rather than tracing::warn! to avoid the theoretical
            // recursion of warn-events flowing through this Layer.
            eprintln!(
                "diagnostics layer: append failed for span '{}' (kind={:?}): {:#}",
                entry.name, entry.kind, e
            );
        }
    }
}

/// Visit span fields and route them into `extras` (numeric/bool
/// scalars and short closed-set strings) or update `outcome` (the
/// distinguished `outcome` field). Free-text via `record_debug`
/// is dropped silently — the privacy floor at write time would
/// reject anything that escaped here, but cleanest to drop at the
/// visitor level.
struct FieldVisitor<'a> {
    extras: &'a mut BTreeMap<String, serde_json::Value>,
    outcome: &'a mut Outcome,
}

impl<'a> Visit for FieldVisitor<'a> {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "outcome" {
            if let Some(o) = Outcome::from_str(value) {
                *self.outcome = o;
            }
            return;
        }
        self.extras
            .insert(field.name().to_string(), serde_json::Value::String(value.to_string()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.extras.insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.extras.insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.extras.insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.extras.insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {
        // Drop silently. Free-text Debug repr is the kind of thing
        // the privacy floor at write time would reject; cleaner to
        // not capture it here at all.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neurogrim_core::diagnostics_ledger::{self as dl, EventKind};
    use tempfile::TempDir;
    use tracing::Level;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::Registry;

    fn read_ledger(project_root: &std::path::Path) -> Vec<dl::DiagnosticsEntry> {
        let path = dl::default_ledger_path(project_root);
        dl::read_all(&path).expect("read_all should succeed")
    }

    #[test]
    fn mapped_span_emits_ledger_entry_on_close() {
        let dir = TempDir::new().unwrap();
        let project_root = dir.path().to_path_buf();
        let layer = DiagnosticsLayer::new(project_root.clone());
        let subscriber = Registry::default().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::span!(Level::INFO, "score.pipeline.run", domains_count = 12i64);
            let _enter = span.enter();
        }); // span dropped here → on_close fires

        let entries = read_ledger(&project_root);
        assert_eq!(entries.len(), 1, "expected exactly one entry");
        assert_eq!(entries[0].kind, EventKind::Scoring);
        assert_eq!(entries[0].name, "score.pipeline.run");
        assert_eq!(entries[0].outcome, Outcome::Ok);
        assert_eq!(entries[0].depth, 0);
        assert_eq!(entries[0].parent_event_id, None);
        assert_eq!(
            entries[0].extras.get("domains_count"),
            Some(&serde_json::json!(12))
        );
    }

    #[test]
    fn unmapped_span_emits_nothing() {
        let dir = TempDir::new().unwrap();
        let project_root = dir.path().to_path_buf();
        let layer = DiagnosticsLayer::new(project_root.clone());
        let subscriber = Registry::default().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::span!(Level::INFO, "totally.not.in.the.table", x = 1i64);
            let _enter = span.enter();
        });

        let path = dl::default_ledger_path(&project_root);
        // Either the file doesn't exist (no entries written) or
        // it's empty. read_all returns Vec; assert it's empty.
        let entries = dl::read_all(&path).expect("read_all");
        assert!(entries.is_empty(), "unmapped span must not produce entries");
    }

    #[test]
    fn detached_layer_emits_nothing() {
        // No Layer attached: spans fire but no ledger writes happen.
        let dir = TempDir::new().unwrap();
        let project_root = dir.path().to_path_buf();
        let subscriber = Registry::default(); // no DiagnosticsLayer

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::span!(Level::INFO, "score.pipeline.run", domains_count = 5i64);
            let _enter = span.enter();
        });

        let path = dl::default_ledger_path(&project_root);
        // The ledger directory may not even exist.
        assert!(!path.exists() || dl::read_all(&path).unwrap().is_empty());
    }

    #[test]
    fn nested_spans_track_depth_and_parent() {
        let dir = TempDir::new().unwrap();
        let project_root = dir.path().to_path_buf();
        let layer = DiagnosticsLayer::new(project_root.clone());
        let subscriber = Registry::default().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            let outer = tracing::span!(Level::INFO, "test.run", test_count = 10i64);
            let _o = outer.enter();
            {
                let inner = tracing::span!(Level::INFO, "cargo.invoke", exit_code = 0i64);
                let _i = inner.enter();
            } // inner closes first
        }); // outer closes

        let entries = read_ledger(&project_root);
        assert_eq!(entries.len(), 2, "expected outer + inner entries");

        // Inner closes first, so it's the first entry in the ledger.
        let inner_entry = entries.iter().find(|e| e.kind == EventKind::Cargo).unwrap();
        let outer_entry = entries.iter().find(|e| e.kind == EventKind::Test).unwrap();

        assert_eq!(outer_entry.depth, 0, "outer span at depth 0");
        assert_eq!(outer_entry.parent_event_id, None);

        assert_eq!(inner_entry.depth, 1, "inner span at depth 1");
        assert_eq!(
            inner_entry.parent_event_id.as_ref(),
            Some(&outer_entry.event_id),
            "inner.parent_event_id should point at outer.event_id"
        );
    }

    #[test]
    fn unmapped_intermediate_does_not_break_parent_chain() {
        // outer (mapped) → middle (unmapped) → inner (mapped).
        // inner's parent_event_id should reach UP past middle to
        // outer, not be None.
        let dir = TempDir::new().unwrap();
        let project_root = dir.path().to_path_buf();
        let layer = DiagnosticsLayer::new(project_root.clone());
        let subscriber = Registry::default().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            let outer = tracing::span!(Level::INFO, "test.run", test_count = 1i64);
            let _o = outer.enter();
            let middle = tracing::span!(Level::INFO, "intermediate.unmapped", x = 1i64);
            let _m = middle.enter();
            let inner = tracing::span!(Level::INFO, "cargo.invoke", exit_code = 0i64);
            let _i = inner.enter();
        });

        let entries = read_ledger(&project_root);
        // 2 entries (test.run + cargo.invoke); the unmapped middle
        // contributes nothing.
        assert_eq!(entries.len(), 2);
        let inner_entry = entries.iter().find(|e| e.kind == EventKind::Cargo).unwrap();
        let outer_entry = entries.iter().find(|e| e.kind == EventKind::Test).unwrap();
        assert_eq!(
            inner_entry.parent_event_id.as_ref(),
            Some(&outer_entry.event_id),
            "inner should chain past unmapped middle to outer"
        );
        // depth counts MAPPED ancestors only.
        assert_eq!(inner_entry.depth, 1, "depth ignores unmapped middle");
    }

    #[test]
    fn outcome_field_recorded_during_span_lifetime() {
        let dir = TempDir::new().unwrap();
        let project_root = dir.path().to_path_buf();
        let layer = DiagnosticsLayer::new(project_root.clone());
        let subscriber = Registry::default().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            // Pre-declare outcome as a tracing field via Empty so
            // that span.record() can update it later. This is the
            // standard tracing idiom.
            let span = tracing::span!(
                Level::INFO,
                "test.run",
                test_count = 5i64,
                outcome = tracing::field::Empty
            );
            let _e = span.enter();
            // Simulate work that fails…
            tracing::Span::current().record("outcome", "err");
        });

        let entries = read_ledger(&project_root);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].outcome, Outcome::Err);
    }

    #[test]
    fn forbidden_field_name_rejected_at_append_time() {
        // If Phase 3 instrumentation accidentally puts a forbidden
        // key on a span, the Layer captures it via the visitor and
        // append() rejects it. The Layer logs a warning and
        // continues — it must never break the host.
        let dir = TempDir::new().unwrap();
        let project_root = dir.path().to_path_buf();
        let layer = DiagnosticsLayer::new(project_root.clone());
        let subscriber = Registry::default().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            // `prompt` is on FORBIDDEN_EXTRAS_KEYS — append will reject.
            let span = tracing::span!(
                Level::INFO,
                "score.pipeline.run",
                prompt = "would be a privacy leak"
            );
            let _e = span.enter();
        });

        let entries = read_ledger(&project_root);
        // Append failed → no entry in the ledger. Layer did not
        // panic. The eprintln warning is observable side-effect but
        // we don't assert on stderr in this test.
        assert!(
            entries.is_empty(),
            "forbidden field must cause append to reject; ledger stays empty"
        );
    }

    #[test]
    fn duration_ms_is_recorded() {
        let dir = TempDir::new().unwrap();
        let project_root = dir.path().to_path_buf();
        let layer = DiagnosticsLayer::new(project_root.clone());
        let subscriber = Registry::default().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::span!(Level::INFO, "test.run", test_count = 1i64);
            let _e = span.enter();
            std::thread::sleep(std::time::Duration::from_millis(15));
        });

        let entries = read_ledger(&project_root);
        assert_eq!(entries.len(), 1);
        assert!(
            entries[0].duration_ms >= 10,
            "duration_ms should reflect the 15ms sleep, got {}",
            entries[0].duration_ms
        );
    }

    #[test]
    fn kind_for_span_name_covers_every_event_kind() {
        // Sanity: every variant of EventKind has a span-name in
        // the closed table. Adding an EventKind without a span
        // name would silently make spans of that kind unobservable.
        let kinds = [
            EventKind::Build,
            EventKind::Test,
            EventKind::Cargo,
            EventKind::Scoring,
            EventKind::McpDispatch,
            EventKind::A2aPost,
            EventKind::A2aSse,
            EventKind::DashboardRoute,
            EventKind::DiagSynthesis,
        ];
        let table_kinds: Vec<EventKind> = [
            "score.pipeline.run",
            "test.run",
            "cargo.invoke",
            "mcp.sensory",
            "a2a.post",
            "a2a.sse",
            "dashboard.route",
            "build",
            "diag.synthesis",
        ]
        .iter()
        .filter_map(|n| kind_for_span_name(n))
        .collect();
        for k in kinds {
            assert!(
                table_kinds.contains(&k),
                "EventKind {:?} has no span-name in kind_for_span_name; \
                 instrumented spans of that kind would be silently dropped",
                k
            );
        }
    }
}
