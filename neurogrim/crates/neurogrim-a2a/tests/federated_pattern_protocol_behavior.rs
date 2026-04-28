//! E-B2-7 C4 — federated-pattern protocol-behavior tests.
//!
//! Pins the §16.6.1 MUST-clauses at the type-and-wire boundary:
//!
//!  1. **`bidirectional_opt_in_required`** — both peers must
//!     declare `federated-pattern` in `accepts[]` AND `emits[]`
//!     (Q5).
//!  2. **`emit_rejects_when_opt_in_not_satisfied`** —
//!     [`Error::OptInNotSatisfied`] when bidirectional opt-in
//!     fails.
//!  3. **`emit_recursion_guard_source_level_rejects_federated_patterns_kind`**
//!     — [`Error::RecursionGuardSenderSide`] for pattern_kind
//!     values starting with `federated_patterns:` (Q9 source-
//!     level lock).
//!  4. **`receive_recursion_guard_drops_self_loop`** — wire-level
//!     recursion guard: receiver's own opaque-hash in
//!     `origin_set[]` ⇒ `Dropped(RecursionGuard)` (Q9).
//!  5. **`receive_hop_limit_drops_when_origin_set_too_long`** —
//!     `origin_set.len() > MAX_HOPS=4` ⇒
//!     `Dropped(HopLimitExceeded)` (Q15).
//!  6. **`receive_rate_limit_drops_after_threshold`** — sliding-
//!     window: 11th receipt within 60s ⇒
//!     `Dropped(RateLimitExceeded)` (Q6).
//!  7. **`receive_unknown_pattern_kind_drops_for_forward_compat_q11`**
//!     — graceful degradation: unknown pattern_kind ⇒
//!     `Dropped(UnknownPatternKind)` (Q11).
//!  8. **`emit_rate_limit_blocks_excess_in_flight`** — semaphore
//!     bounds concurrent in-flight to
//!     `FEDERATED_PATTERN_MAX_INFLIGHT=2` per peer (Q6
//!     sender-side).
//!  9. **`emit_rate_limit_enforces_min_interval`** — per-permit
//!     min interval observed (Q6 sender-side, scaled-down to
//!     keep wall-clock small).
//! 10. **`feature_vector_serialization_roundtrip`** — typed
//!     payload roundtrips byte-for-byte via JSON (Q1 + Q8 PRIVACY
//!     PIN — closed-set numeric-only fields).

use neurogrim_a2a::agent_card::{
    AgentCard, AuthScheme, Authentication, Capabilities, Transport as CardTransport,
    TransportProtocol,
};
use neurogrim_a2a::envelope::{A2aEnvelope, MessageType};
use neurogrim_a2a::error::A2aError;
use neurogrim_a2a::federated_pattern::{
    bidirectional_opt_in_satisfied, build_federated_pattern_envelope, emit_federated_pattern,
    handle_received_federated_pattern, AdmitOutcome, DroppedReason, Error,
    FederatedPatternEmitLimiter, FederatedPatternPayload, FederatedPatternReceiveLimiter,
    FeatureVector, PatternKind, SeverityClass, MAX_HOPS,
};
use neurogrim_a2a::transport::{EnvelopeStream, TaskAccepted, Transport as A2aTransport};

use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use url::Url;

// =========================================================================
// Test fixtures
// =========================================================================

fn agent_card_with_capabilities(id: &str, accepts_emits_fp: bool) -> AgentCard {
    let (accepts, emits) = if accepts_emits_fp {
        (
            vec![MessageType::FederatedPattern],
            vec![MessageType::FederatedPattern],
        )
    } else {
        (vec![], vec![])
    };
    AgentCard {
        schema_version: "1".to_string(),
        id: id.to_string(),
        name: id.to_string(),
        version: "0.1.0".to_string(),
        interface_version: "1".to_string(),
        capabilities: Capabilities {
            accepts,
            emits,
            streaming: false,
        },
        transport: CardTransport {
            protocol: TransportProtocol::HttpSse,
            endpoint: format!("http://127.0.0.1:8421/a2a/v1/{}", id),
            tasks_path: "/tasks".to_string(),
        },
        authentication: Authentication {
            scheme: AuthScheme::None,
        },
        topology: None,
    }
}

fn sample_payload() -> FederatedPatternPayload {
    FederatedPatternPayload {
        schema_version: "1".to_string(),
        pattern_kind: PatternKind::VigilancePattern,
        feature_vector: FeatureVector {
            numeric_count: 3,
            severity_class: SeverityClass::High,
            observation_window_days: 7,
        },
        anonymized_origin: "sha256-opaque-hash-aaaa".to_string(),
        origin_set: vec!["sha256-opaque-hash-aaaa".to_string()],
        peer_brain_id: "sha256-opaque-hash-bbbb".to_string(),
        discovered_at: "2026-04-27T20:00:00.000Z".to_string(),
        severity_class: Some(SeverityClass::High),
        legal_disclaimer: None,
        metadata: None,
    }
}

/// In-memory transport that records every `post_task` call without
/// touching the network. Mirrors the test-double pattern from the
/// supply-chain-signal test suites.
#[derive(Debug, Default, Clone)]
struct RecordingTransport {
    posted: Arc<Mutex<Vec<A2aEnvelope>>>,
    /// Optional artificial delay applied inside `post_task` so
    /// concurrency tests can observe the in-flight cap.
    delay: Option<Duration>,
}

impl RecordingTransport {
    fn new() -> Self {
        Self::default()
    }

    fn with_delay(delay: Duration) -> Self {
        Self {
            posted: Arc::new(Mutex::new(Vec::new())),
            delay: Some(delay),
        }
    }

    async fn posted_count(&self) -> usize {
        self.posted.lock().await.len()
    }
}

#[async_trait]
impl A2aTransport for RecordingTransport {
    async fn post_task(
        &self,
        _endpoint: &Url,
        envelope: &A2aEnvelope,
    ) -> Result<TaskAccepted, A2aError> {
        if let Some(d) = self.delay {
            tokio::time::sleep(d).await;
        }
        self.posted.lock().await.push(envelope.clone());
        Ok(TaskAccepted {
            task_id: format!("task-{}", envelope.message_id),
            accepted_at: chrono::Utc::now(),
        })
    }

    async fn poll_task(
        &self,
        _endpoint: &Url,
        _task_id: &str,
    ) -> Result<Option<A2aEnvelope>, A2aError> {
        Ok(None)
    }

    async fn stream_task(
        &self,
        _endpoint: &Url,
        _task_id: &str,
    ) -> Result<EnvelopeStream, A2aError> {
        Err(A2aError::Transport(
            "RecordingTransport does not implement stream_task".into(),
        ))
    }
}

// =========================================================================
// Tests
// =========================================================================

/// Test 1 — Q5 lock: both peers MUST declare federated-pattern in
/// accepts[] AND emits[].
#[tokio::test]
async fn bidirectional_opt_in_required() {
    let local_with = agent_card_with_capabilities("local", true);
    let peer_with = agent_card_with_capabilities("peer", true);
    let no_opt_in = agent_card_with_capabilities("nobody", false);

    // Both declare ⇒ true.
    assert!(bidirectional_opt_in_satisfied(&local_with, &peer_with));
    // Local missing ⇒ false.
    assert!(!bidirectional_opt_in_satisfied(&no_opt_in, &peer_with));
    // Peer missing ⇒ false.
    assert!(!bidirectional_opt_in_satisfied(&local_with, &no_opt_in));
    // Both missing ⇒ false.
    assert!(!bidirectional_opt_in_satisfied(&no_opt_in, &no_opt_in));

    // Asymmetric: local has accepts only; peer has both. Local
    // missing emits[] ⇒ false (not bidirectional).
    let mut accepts_only = local_with.clone();
    accepts_only.capabilities.emits = vec![];
    assert!(!bidirectional_opt_in_satisfied(&accepts_only, &peer_with));
}

/// Test 2 — Q5 lock surface: emit_federated_pattern returns
/// Error::OptInNotSatisfied when peers don't both advertise.
#[tokio::test]
async fn emit_rejects_when_opt_in_not_satisfied() {
    let local = agent_card_with_capabilities("local", true);
    let peer = agent_card_with_capabilities("peer", false); // no advertise
    let endpoint = Url::parse("http://127.0.0.1:8421/a2a/v1/").unwrap();
    let limiter = FederatedPatternEmitLimiter::with_limits(2, Duration::from_millis(0));
    let transport = RecordingTransport::new();

    let result = emit_federated_pattern(
        &local,
        &peer,
        &endpoint,
        sample_payload(),
        "vigilance-pattern",
        &limiter,
        &transport,
    )
    .await;

    assert!(matches!(result, Err(Error::OptInNotSatisfied)));
    assert_eq!(transport.posted_count().await, 0);
}

/// Test 3 — Q9 source-level lock: emit rejects pattern_kind values
/// starting with `federated_patterns:`. Defense-in-depth — even
/// though the closed-set Q14 enum doesn't list any
/// `federated_patterns:*` value, this guard catches v2/v3
/// vocabulary additions that would re-open the recursion-loop.
#[tokio::test]
async fn emit_recursion_guard_source_level_rejects_federated_patterns_kind() {
    let local = agent_card_with_capabilities("local", true);
    let peer = agent_card_with_capabilities("peer", true);
    let endpoint = Url::parse("http://127.0.0.1:8421/a2a/v1/").unwrap();
    let limiter = FederatedPatternEmitLimiter::with_limits(2, Duration::from_millis(0));
    let transport = RecordingTransport::new();

    let result = emit_federated_pattern(
        &local,
        &peer,
        &endpoint,
        sample_payload(),
        // Wire-format pattern_kind that starts with the
        // aggregator's finding-kind prefix. Even though our v1
        // typed PatternKind enum doesn't include this value, the
        // guard is OPEN at the wire-format-string level.
        "federated_patterns:low_confidence",
        &limiter,
        &transport,
    )
    .await;

    match result {
        Err(Error::RecursionGuardSenderSide {
            prefix,
            pattern_kind,
        }) => {
            assert_eq!(prefix, "federated_patterns:");
            assert_eq!(pattern_kind, "federated_patterns:low_confidence");
        }
        other => panic!("expected RecursionGuardSenderSide; got {:?}", other),
    }
    assert_eq!(transport.posted_count().await, 0);
}

/// Test 4 — Q9 wire-level lock: receiver drops messages whose
/// origin_set[] contains the receiver's own opaque hash.
#[tokio::test]
async fn receive_recursion_guard_drops_self_loop() {
    let local_hash = "sha256-opaque-hash-self";
    let receive_limiter = FederatedPatternReceiveLimiter::new();

    let mut payload = sample_payload();
    payload.origin_set = vec!["sha256-opaque-hash-zzzz".to_string(), local_hash.to_string()];
    payload.anonymized_origin = "sha256-opaque-hash-zzzz".to_string();

    let envelope = build_federated_pattern_envelope(payload, "peer-id").unwrap();

    let outcome = handle_received_federated_pattern(&envelope, local_hash, &receive_limiter)
        .await
        .unwrap();

    assert_eq!(outcome.dropped_reason(), Some(DroppedReason::RecursionGuard));
    assert!(!outcome.is_accepted());
}

/// Test 5 — Q15 lock: receiver drops messages whose origin_set
/// length exceeds MAX_HOPS=4. (The schema enforces maxItems 4
/// upstream too; this guard is defense-in-depth at the receiver.)
///
/// Because [`FederatedPatternPayload::validate`] also rejects
/// >MAX_HOPS, we have to construct the envelope by hand to bypass
/// the sender-side check — simulating a tampered payload.
#[tokio::test]
async fn receive_hop_limit_drops_when_origin_set_too_long() {
    let local_hash = "sha256-opaque-hash-self";
    let receive_limiter = FederatedPatternReceiveLimiter::new();

    // Build a payload JSON value with 5 entries in origin_set —
    // bypass the typed sender path so we can exercise the
    // receiver's defense-in-depth guard.
    let payload_value = serde_json::json!({
        "schema_version": "1",
        "pattern_kind": "vigilance-pattern",
        "feature_vector": {
            "numeric_count": 1,
            "severity_class": "low",
            "observation_window_days": 1
        },
        "anonymized_origin": "sha256-opaque-hash-aaaa",
        "origin_set": [
            "sha256-opaque-hash-aaaa",
            "sha256-opaque-hash-bbbb",
            "sha256-opaque-hash-cccc",
            "sha256-opaque-hash-dddd",
            "sha256-opaque-hash-eeee"
        ],
        "peer_brain_id": "sha256-opaque-hash-target",
        "discovered_at": "2026-04-27T20:00:00.000Z"
    });
    let envelope = A2aEnvelope::new("peer-id", MessageType::FederatedPattern, payload_value);

    let outcome = handle_received_federated_pattern(&envelope, local_hash, &receive_limiter)
        .await
        .unwrap();

    assert_eq!(
        outcome.dropped_reason(),
        Some(DroppedReason::HopLimitExceeded)
    );
    // Verify MAX_HOPS lock is actually 4 (Q15 spec compliance —
    // any future relaxation requires a deliberate spec change).
    assert_eq!(MAX_HOPS, 4);
}

/// Test 6 — Q6 lock: receiver-side sliding-window drops the 11th
/// receipt within 60s from the same peer. We use a scaled-down
/// limiter (max=10, window=60s) to verify behavior without
/// blocking the test.
#[tokio::test]
async fn receive_rate_limit_drops_after_threshold() {
    let limiter = FederatedPatternReceiveLimiter::with_limits(10, Duration::from_secs(60));

    // 10 receipts admitted, 11th dropped.
    for i in 0..10 {
        let r = limiter.admit_receipt("sha256-peer-x").await;
        assert_eq!(r, AdmitOutcome::Accepted, "receipt {} should be accepted", i);
    }
    let r = limiter.admit_receipt("sha256-peer-x").await;
    assert_eq!(
        r,
        AdmitOutcome::Dropped,
        "11th receipt should be dropped (Q6 sliding-window)"
    );

    // Verify per-peer isolation: a different peer is admitted.
    let other = limiter.admit_receipt("sha256-peer-y").await;
    assert_eq!(other, AdmitOutcome::Accepted);
}

/// Test 7 — Q11 forward-compat lock: receiver drops payloads with
/// unknown pattern_kind (e.g., `operator-calibration-pattern`)
/// with `UnknownPatternKind` — NOT an error, graceful
/// degradation.
#[tokio::test]
async fn receive_unknown_pattern_kind_drops_for_forward_compat_q11() {
    let local_hash = "sha256-opaque-hash-self";
    let receive_limiter = FederatedPatternReceiveLimiter::new();

    // Build a payload with v2 pattern_kind that isn't in v1
    // closed-set. Bypass the typed sender path because the typed
    // PatternKind enum at v1 doesn't include this value.
    let payload_value = serde_json::json!({
        "schema_version": "1",
        "pattern_kind": "operator-calibration-pattern",
        "feature_vector": {
            "numeric_count": 1,
            "severity_class": "low",
            "observation_window_days": 1
        },
        "anonymized_origin": "sha256-opaque-hash-aaaa",
        "origin_set": ["sha256-opaque-hash-aaaa"],
        "peer_brain_id": "sha256-opaque-hash-target",
        "discovered_at": "2026-04-27T20:00:00.000Z"
    });
    let envelope = A2aEnvelope::new("peer-id", MessageType::FederatedPattern, payload_value);

    let outcome = handle_received_federated_pattern(&envelope, local_hash, &receive_limiter)
        .await
        .unwrap();

    assert_eq!(
        outcome.dropped_reason(),
        Some(DroppedReason::UnknownPatternKind),
        "unknown pattern_kinds drop with UnknownPatternKind for forward-compat (Q11)"
    );
}

/// Test 8 — Q6 sender-side lock: per-peer semaphore caps
/// concurrent in-flight emits at 2.
///
/// We construct a limiter with `max_inflight=2`, install an
/// artificial 50ms delay in the transport, and launch 3
/// concurrent emits. The first two acquire permits and complete
/// concurrently; the third must wait for one of the first two
/// to drop its permit. Wall-clock for 3 emits should exceed
/// 2 × 50ms (the third can't start until one finishes).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn emit_rate_limit_blocks_excess_in_flight() {
    let local = agent_card_with_capabilities("local", true);
    let peer = agent_card_with_capabilities("peer", true);
    let endpoint = Url::parse("http://127.0.0.1:8421/a2a/v1/").unwrap();
    let limiter = Arc::new(FederatedPatternEmitLimiter::with_limits(
        2,
        Duration::from_millis(0),
    ));
    let transport = Arc::new(RecordingTransport::with_delay(Duration::from_millis(50)));

    let start = Instant::now();
    let mut handles = Vec::new();
    for i in 0..3 {
        let local = local.clone();
        let peer = peer.clone();
        let endpoint = endpoint.clone();
        let limiter = Arc::clone(&limiter);
        let transport = Arc::clone(&transport);
        let mut payload = sample_payload();
        // Ensure each emit has a distinct payload so we can
        // verify all 3 made it.
        payload.anonymized_origin = format!("sha256-opaque-hash-{}", i);
        payload.origin_set = vec![format!("sha256-opaque-hash-{}", i)];
        handles.push(tokio::spawn(async move {
            emit_federated_pattern(
                &local,
                &peer,
                &endpoint,
                payload,
                "vigilance-pattern",
                &limiter,
                transport.as_ref(),
            )
            .await
        }));
    }

    for h in handles {
        h.await.unwrap().expect("emit should succeed");
    }
    let elapsed = start.elapsed();

    // 3 emits with 2-permit semaphore + 50ms transport delay
    // should take MORE than 1 batch (50ms) — the third emit
    // can't start until one of the first two finishes. Verify
    // we paid a full second batch worth (≥50ms total) which
    // would not be true if all 3 ran in parallel.
    assert!(
        elapsed >= Duration::from_millis(75),
        "elapsed {}ms is too low — all 3 emits seem to have run in parallel; \
         the semaphore should have queued the 3rd until one of the first two dropped",
        elapsed.as_millis()
    );
    // All 3 should have been transmitted.
    assert_eq!(transport.posted_count().await, 3);
}

/// Test 9 — Q6 sender-side lock: per-peer minimum interval
/// observed. Configure a small `min_interval` and verify two
/// back-to-back emits to the same peer pay the interval.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn emit_rate_limit_enforces_min_interval() {
    let local = agent_card_with_capabilities("local", true);
    let peer = agent_card_with_capabilities("peer", true);
    let endpoint = Url::parse("http://127.0.0.1:8421/a2a/v1/").unwrap();
    // Use 100ms min interval to keep the test wall-clock small
    // while still proving the mechanism works. The locked v1
    // constant is FEDERATED_PATTERN_MIN_INTERVAL=6s; tests scale
    // it down per the supply-chain-signal precedent.
    let limiter = FederatedPatternEmitLimiter::with_limits(2, Duration::from_millis(100));
    let transport = RecordingTransport::new();

    // First emit — should not pay the interval (no previous emit).
    let start1 = Instant::now();
    emit_federated_pattern(
        &local,
        &peer,
        &endpoint,
        sample_payload(),
        "vigilance-pattern",
        &limiter,
        &transport,
    )
    .await
    .unwrap();
    let elapsed1 = start1.elapsed();
    assert!(
        elapsed1 < Duration::from_millis(50),
        "first emit should not pay the min-interval penalty; got {}ms",
        elapsed1.as_millis()
    );

    // Second emit immediately after — must pay ≥ 100ms.
    let start2 = Instant::now();
    emit_federated_pattern(
        &local,
        &peer,
        &endpoint,
        sample_payload(),
        "vigilance-pattern",
        &limiter,
        &transport,
    )
    .await
    .unwrap();
    let elapsed2 = start2.elapsed();
    assert!(
        elapsed2 >= Duration::from_millis(80),
        "second emit must pay the min-interval penalty (≈100ms); got {}ms",
        elapsed2.as_millis()
    );

    assert_eq!(transport.posted_count().await, 2);
}

/// Test 10 — Q1 + Q8 PRIVACY PIN: typed payload roundtrips
/// byte-for-byte via JSON. Closed-set numeric-only fields
/// preserve fidelity; smuggled fields would fail to deserialize
/// (covered upstream by C3 schema-conformance tests).
#[test]
fn feature_vector_serialization_roundtrip() {
    let original = sample_payload();
    let json = serde_json::to_string(&original).unwrap();
    let back: FederatedPatternPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(original, back);

    // Verify FeatureVector itself roundtrips with byte-exact
    // closed-set field names.
    let vec = FeatureVector {
        numeric_count: 42,
        severity_class: SeverityClass::Critical,
        observation_window_days: 14,
    };
    let vec_json = serde_json::to_string(&vec).unwrap();
    assert!(vec_json.contains("\"numeric_count\":42"));
    assert!(vec_json.contains("\"severity_class\":\"critical\""));
    assert!(vec_json.contains("\"observation_window_days\":14"));
    let back_vec: FeatureVector = serde_json::from_str(&vec_json).unwrap();
    assert_eq!(vec, back_vec);

    // Verify deny_unknown_fields rejects smuggled fields.
    let smuggled = serde_json::json!({
        "schema_version": "1",
        "pattern_kind": "vigilance-pattern",
        "feature_vector": {
            "numeric_count": 1,
            "severity_class": "low",
            "observation_window_days": 1
        },
        "anonymized_origin": "sha256-aaaa",
        "origin_set": ["sha256-aaaa"],
        "peer_brain_id": "sha256-bbbb",
        "discovered_at": "2026-04-27T20:00:00.000Z",
        "note": "smuggled free-text — MUST be rejected per Q8 PRIVACY PIN"
    });
    let r: Result<FederatedPatternPayload, _> = serde_json::from_value(smuggled);
    assert!(
        r.is_err(),
        "deny_unknown_fields must reject top-level smuggled `note`"
    );
}

/// Honesty pin: VecDeque is a public dependency assumption that
/// FederatedPatternReceiveLimiter relies on; changing the import
/// triggers test compilation and is detectable. Suppresses the
/// unused-import warning on the test-internal use.
#[test]
fn _vecdeque_is_in_scope() {
    let _v: VecDeque<i32> = VecDeque::new();
}
