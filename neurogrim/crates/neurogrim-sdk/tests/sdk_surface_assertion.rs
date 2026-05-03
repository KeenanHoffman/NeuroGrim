//! V5-SDK-1 Phase 4 (2026-05-03) — **SDK surface assertion test**.
//!
//! This file is the V5-SDK-1 semver gate. It pins the exact public
//! signature of every re-exported trait method by *using* it: each
//! `_pin_*` function takes a `&dyn Trait` and calls a method,
//! binding the return value to the expected return type. If any
//! re-exported trait's shape changes upstream (rename, parameter
//! retype, return retype, generic bounds tighten/loosen), the
//! wrapper here fails to compile — which is exactly what we want
//! at PR time.
//!
//! ## Why this approach instead of `cargo-semver-checks`?
//!
//! `cargo-semver-checks` is the standard Rust semver-gate tool,
//! and was the V5-SDK-1 Phase 4 plan default. **It does not work
//! for pure re-export crates.** rustdoc 2018+ does not inline
//! foreign-crate items into the re-exporting crate's rustdoc JSON
//! (rust#94338, blocked upstream pending rustdoc fixes), so
//! `cargo-semver-checks`'s lints — which read that JSON — see
//! only the `pub use` aliases, not the items they point at.
//!
//! Verified empirically 2026-05-03:
//! - Renaming a `pub use foo::Bar` to `as Baz` → not detected.
//! - Deleting a `pub use foo::Bar` line → not detected.
//! - Adding a required method (no default) to a re-exported trait
//!   → not detected.
//!
//! References: cargo-semver-checks issues #167, #291, #355, #629;
//! Predrag's "Four challenges" blog; rust#94338. See
//! [`SEMVER-OVERRIDE.md`](../SEMVER-OVERRIDE.md) for the full
//! rationale and the gate's override path.
//!
//! ## What this test catches
//!
//! - ✓ Re-export removed (`pub use neurogrim_core::sensor::Sensor;`
//!   deleted) — the wrapper's `&dyn neurogrim_sdk::Sensor` path
//!   fails to resolve.
//! - ✓ Re-export renamed (`Sensor` → `SensorXyz`) — same path
//!   resolution failure.
//! - ✓ Trait method renamed upstream (`Sensor::analyze` →
//!   `Sensor::analyse`) — `s.analyze(...)` no longer exists.
//! - ✓ Trait method parameter retyped (`&str` → `&Path`) — type
//!   mismatch at the call site.
//! - ✓ Trait method return retyped (`anyhow::Result<Value>` →
//!   `anyhow::Result<String>`) — the `let _: ExpectedT = ...`
//!   binding fails.
//! - ✓ Required method added without default — every existing impl
//!   would fail to compile, including the conformance suite stubs;
//!   indirectly catches at the workspace test level.
//!
//! ## What this test does NOT catch
//!
//! - ✗ Visibility-only changes to re-exported items (e.g., a field
//!   on a re-exported struct goes from `pub` to `pub(crate)`).
//!   These don't affect trait-method signatures, so the wrappers
//!   here compile; downstream consumers would break differently.
//!   Captured as a known gap (`BACKLOG.md` § B-MOD-SDK-SEMVER-GAP).
//!
//! ## Maintenance protocol
//!
//! When adding a new re-exported trait to `lib.rs`, add a matching
//! `_pin_<trait>_<method>` function here for every method on that
//! trait. The functions are compile-time-only; runtime cost is zero
//! (they are never called).
//!
//! When intentionally evolving a re-exported trait's shape, update
//! the wrapper signature here in the SAME PR that lands the change,
//! and bump `crates/neurogrim-sdk/Cargo.toml` `version` per
//! [`SEMVER-OVERRIDE.md`](../SEMVER-OVERRIDE.md).

#![allow(dead_code, clippy::extra_unused_lifetimes)]

use std::path::Path;
use std::sync::Arc;

use neurogrim_sdk::{
    QueueBackend, QueueBackendFactory, QueueMessage, ScoringSource, ScoringSourceFactory,
    SecretBackend, Sensor, SensorFactory, StoredMessage, Transport,
};

// ── Sensor (V5-MOD-2) ────────────────────────────────────────────────

/// Pins `Sensor::analyze(&self, project_root: &str) -> anyhow::Result<serde_json::Value>`.
/// Async via `#[async_trait]`; the `.await` enforces the future's output type.
async fn _pin_sensor_analyze<S: Sensor + ?Sized>(s: &S, project_root: &str) {
    let _: anyhow::Result<serde_json::Value> = s.analyze(project_root).await;
}

/// Pins `SensorFactory::name(&self) -> &'static str`.
fn _pin_sensor_factory_name<F: SensorFactory + ?Sized>(f: &F) {
    let _: &'static str = f.name();
}

/// Pins `SensorFactory::build(&self) -> Box<dyn Sensor>`.
fn _pin_sensor_factory_build<F: SensorFactory + ?Sized>(f: &F) {
    let _: Box<dyn Sensor> = f.build();
}

// ── ScoringSource (V5-MOD-1) ─────────────────────────────────────────

/// Pins `ScoringSource::source_type_name(&self) -> &'static str`.
fn _pin_scoring_source_type_name<S: ScoringSource + ?Sized>(s: &S) {
    let _: &'static str = s.source_type_name();
}

/// Pins `ScoringSource::load(&self, domain_key, config, project_root) ->
/// Option<CmdbData>`. The support types `ScoringSourceConfig` and
/// `CmdbData` are NOT re-exported by the SDK (intentional: bound to
/// `brain-registry.json` schema; see V5-MOD-1 hand-off note in
/// `roadmap/epics/v5-sdk.md`). We therefore reach for them via direct
/// `neurogrim_core::*` paths — but the trait method itself is gated
/// through the SDK's re-export.
async fn _pin_scoring_source_load<S: ScoringSource + ?Sized>(
    s: &S,
    domain_key: &str,
    config: &neurogrim_core::registry::ScoringSourceConfig,
    project_root: &Path,
) {
    let _: Option<neurogrim_core::scoring::CmdbData> =
        s.load(domain_key, config, project_root).await;
}

/// Pins `ScoringSourceFactory::source_type_name(&self) -> &'static str`.
fn _pin_scoring_source_factory_type_name<F: ScoringSourceFactory + ?Sized>(f: &F) {
    let _: &'static str = f.source_type_name();
}

/// Pins `ScoringSourceFactory::build(&self) -> Box<dyn ScoringSource>`.
fn _pin_scoring_source_factory_build<F: ScoringSourceFactory + ?Sized>(f: &F) {
    let _: Box<dyn ScoringSource> = f.build();
}

// ── QueueBackend (V5-MOD-3) ──────────────────────────────────────────

/// Pins `QueueBackend::append(&self, msg: &QueueMessage) -> anyhow::Result<u64>`.
fn _pin_queue_backend_append<B: QueueBackend + ?Sized>(b: &B, msg: &QueueMessage) {
    let _: anyhow::Result<u64> = b.append(msg);
}

/// Pins `QueueBackend::read_from(&self, since_offset: u64, limit: usize) ->
/// anyhow::Result<Vec<StoredMessage>>`.
fn _pin_queue_backend_read_from<B: QueueBackend + ?Sized>(b: &B, since_offset: u64, limit: usize) {
    let _: anyhow::Result<Vec<StoredMessage>> = b.read_from(since_offset, limit);
}

/// Pins `QueueBackend::len(&self) -> anyhow::Result<u64>`.
fn _pin_queue_backend_len<B: QueueBackend + ?Sized>(b: &B) {
    let _: anyhow::Result<u64> = b.len();
}

/// Pins `QueueBackend::supports_ack(&self) -> bool` (default-impl method).
fn _pin_queue_backend_supports_ack<B: QueueBackend + ?Sized>(b: &B) {
    let _: bool = b.supports_ack();
}

/// Pins `QueueBackend::read_unacked(&self, consumer_group: &str, limit: usize) ->
/// anyhow::Result<Vec<StoredMessage>>` (default-impl method).
fn _pin_queue_backend_read_unacked<B: QueueBackend + ?Sized>(
    b: &B,
    consumer_group: &str,
    limit: usize,
) {
    let _: anyhow::Result<Vec<StoredMessage>> = b.read_unacked(consumer_group, limit);
}

/// Pins `QueueBackend::ack(&self, offset: u64, consumer_group: &str) ->
/// anyhow::Result<()>` (default-impl method).
fn _pin_queue_backend_ack<B: QueueBackend + ?Sized>(b: &B, offset: u64, consumer_group: &str) {
    let _: anyhow::Result<()> = b.ack(offset, consumer_group);
}

/// Pins `QueueBackend::last_acked(&self, consumer_group: &str) ->
/// anyhow::Result<Option<u64>>` (default-impl method).
fn _pin_queue_backend_last_acked<B: QueueBackend + ?Sized>(b: &B, consumer_group: &str) {
    let _: anyhow::Result<Option<u64>> = b.last_acked(consumer_group);
}

/// Pins `QueueBackendFactory::name(&self) -> &'static str`.
fn _pin_queue_backend_factory_name<F: QueueBackendFactory + ?Sized>(f: &F) {
    let _: &'static str = f.name();
}

/// Pins `QueueBackendFactory::supports_ack(&self) -> bool` (default-impl method).
fn _pin_queue_backend_factory_supports_ack<F: QueueBackendFactory + ?Sized>(f: &F) {
    let _: bool = f.supports_ack();
}

/// Pins `QueueBackendFactory::build(&self, queue_root: &Path, topic: &str) ->
/// anyhow::Result<Arc<dyn QueueBackend>>`.
fn _pin_queue_backend_factory_build<F: QueueBackendFactory + ?Sized>(
    f: &F,
    queue_root: &Path,
    topic: &str,
) {
    let _: anyhow::Result<Arc<dyn QueueBackend>> = f.build(queue_root, topic);
}

// ── Transport (v3.x A2A) ─────────────────────────────────────────────

/// Pins `Transport::post_task(&self, endpoint: &Url, envelope: &A2aEnvelope) ->
/// Result<TaskAccepted, A2aError>`. Support types are not re-exported by SDK;
/// imported directly from `neurogrim_a2a` since the trait re-export carries them
/// transitively.
async fn _pin_transport_post_task<T: Transport + ?Sized>(
    t: &T,
    endpoint: &url::Url,
    envelope: &neurogrim_a2a::A2aEnvelope,
) {
    let _: Result<neurogrim_a2a::TaskAccepted, neurogrim_a2a::A2aError> =
        t.post_task(endpoint, envelope).await;
}

/// Pins `Transport::poll_task(&self, endpoint: &Url, task_id: &str) ->
/// Result<Option<A2aEnvelope>, A2aError>`.
async fn _pin_transport_poll_task<T: Transport + ?Sized>(
    t: &T,
    endpoint: &url::Url,
    task_id: &str,
) {
    let _: Result<Option<neurogrim_a2a::A2aEnvelope>, neurogrim_a2a::A2aError> =
        t.poll_task(endpoint, task_id).await;
}

/// Pins `Transport::stream_task(&self, endpoint: &Url, task_id: &str) ->
/// Result<EnvelopeStream, A2aError>`.
async fn _pin_transport_stream_task<T: Transport + ?Sized>(
    t: &T,
    endpoint: &url::Url,
    task_id: &str,
) {
    let _: Result<neurogrim_a2a::transport::EnvelopeStream, neurogrim_a2a::A2aError> =
        t.stream_task(endpoint, task_id).await;
}

// ── SecretBackend (v4.2 S14) ─────────────────────────────────────────

/// Pins `SecretBackend::get(&self, key: &SecretKey) ->
/// Result<Option<EncryptedSecretValue>, SecretError>`.
fn _pin_secret_backend_get<B: SecretBackend + ?Sized>(b: &B, key: &neurogrim_secrets::SecretKey) {
    let _: Result<Option<neurogrim_secrets::EncryptedSecretValue>, neurogrim_secrets::SecretError> =
        b.get(key);
}

/// Pins `SecretBackend::set(&self, key: &SecretKey, value: SecretValue) ->
/// Result<(), SecretError>`.
fn _pin_secret_backend_set<B: SecretBackend + ?Sized>(
    b: &B,
    key: &neurogrim_secrets::SecretKey,
    value: neurogrim_secrets::SecretValue,
) {
    let _: Result<(), neurogrim_secrets::SecretError> = b.set(key, value);
}

/// Pins `SecretBackend::delete(&self, key: &SecretKey) -> Result<(), SecretError>`.
fn _pin_secret_backend_delete<B: SecretBackend + ?Sized>(
    b: &B,
    key: &neurogrim_secrets::SecretKey,
) {
    let _: Result<(), neurogrim_secrets::SecretError> = b.delete(key);
}

/// Pins `SecretBackend::list(&self, brain_id: &str) ->
/// Result<Vec<SecretMetadata>, SecretError>`.
fn _pin_secret_backend_list<B: SecretBackend + ?Sized>(b: &B, brain_id: &str) {
    let _: Result<Vec<neurogrim_secrets::SecretMetadata>, neurogrim_secrets::SecretError> =
        b.list(brain_id);
}

// ── Tests ────────────────────────────────────────────────────────────
//
// One test verifies the file compiled — that's the actual assertion.
// The wrappers above are the gate; this test exists so `cargo test -p
// neurogrim-sdk` reports a green check for "SDK surface assertion".

#[test]
fn sdk_surface_signatures_unchanged() {
    // The compile-time pinning above is the assertion. If we're
    // here, every re-exported trait method's signature still
    // matches what was pinned on this file's last edit. Proceed
    // through the gate.
}
