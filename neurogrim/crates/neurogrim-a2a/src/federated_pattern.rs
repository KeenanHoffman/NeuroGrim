//! Federated-pattern A2A primitive — sender + receiver wiring per
//! LSP-Brains v2.12 §16.6.1 + the
//! `a2a-federated-pattern-v1.schema.json` schema.
//!
//! # What this module is
//!
//! `federated-pattern` is the FIRST cross-Brain primitive in the
//! Brains-2.0 campaign — a general anonymized-pattern federation
//! channel that mirrors the §16.6 supply-chain-signal posture. The
//! payload is anonymized BY CONSTRUCTION: the closed-set
//! `pattern_kind` enum is single-valued at v1 (`vigilance-pattern`);
//! the `feature_vector` is closed-set numeric-only; the
//! `anonymized_origin` is an opaque hash. Top-level
//! `additionalProperties: false` is the structural enforcement of
//! the privacy contract.
//!
//! # Bidirectional opt-in
//!
//! Both peers MUST declare `MessageType::FederatedPattern` in their
//! Agent Card before federation flows. Same posture as
//! supply-chain-signal — the rationale is identical (legal
//! exposure + false-positive multiplication + privacy under
//! composition). [`bidirectional_opt_in_satisfied`] mirrors the
//! supply-chain-signal helper exactly.
//!
//! # Sender flow
//!
//! 1. Caller constructs a `FederatedPatternPayload` and verifies
//!    locally that the values are well-formed (closed-set
//!    pattern_kind, ≤4-entry origin_set, etc.).
//! 2. Caller obtains a sender-side rate-limit
//!    [`PerPeerRateLimit`] (max 2 in-flight permits per peer + 6s
//!    per-permit minimum interval — effective ~10 messages per
//!    peer per minute, mirroring the R2-2 pattern from
//!    `supply_chain_vigilance/registry.rs:285-297`).
//! 3. Caller calls [`emit_federated_pattern`] which:
//!    - Verifies bidirectional opt-in.
//!    - Verifies payload well-formedness.
//!    - Source-level recursion guard: rejects pattern_kind values
//!      starting with `federated_patterns:` (the aggregator's
//!      finding-kind prefix). Defense-in-depth — the CLI also
//!      enforces this at parse time per Q9 lock.
//!    - Acquires the rate-limit emit permit (blocks if at
//!      capacity).
//!    - Builds the envelope.
//!    - Sends via the [`Transport`].
//!    - Returns [`EmitOutcome`] with the envelope's `message_id`.
//! 4. **Caller responsibility (Q12 log-before-transmit):** the
//!    caller (typically the `neurogrim federated-pattern emit`
//!    CLI) MUST write an `entry_kind=emitted` row to the
//!    `pattern-aggregation-ledger.jsonl` BEFORE calling
//!    [`emit_federated_pattern`]. This module does NOT write the
//!    ledger because the caller has the sender-side context (peer
//!    routing, operator audit) that the wiring layer does not.
//!
//! # Receiver flow
//!
//! 1. The transport layer hands the inbound envelope to
//!    [`handle_received_federated_pattern`] which:
//!    - Verifies envelope's `message_type == "federated-pattern"`.
//!    - **Wire-level recursion guard (Q9):** drops if the
//!      receiver's `local_brain_id_hash` appears in
//!      `payload.origin_set`.
//!    - **Hop limit (Q15):** drops if `origin_set.len() > 4`.
//!    - **Receiver-side rate-limit (Q6):** sliding-window counter
//!      per peer; drops if peer exceeded 10 federated-pattern
//!      receipts per 60-second window.
//!    - **Schema validation:** structural validation via
//!      typed-deserialization of the closed-set
//!      [`FederatedPatternPayload`] struct (the closed-set enums
//!      and numeric-only feature vector enforce the schema's
//!      `additionalProperties: false` discipline at the type
//!      level).
//!    - **Pattern-kind closed-set check (Q11 forgiveness):**
//!      drops with `UnknownPatternKind` for pattern_kind values
//!      not in v1's closed set — forward-compat for v2/v3
//!      pattern_kinds (graceful degradation, NOT an error).
//!    - Returns [`ReceiveOutcome::Accepted`] or
//!      [`ReceiveOutcome::Dropped`] with the structured drop
//!      reason.
//! 2. **Caller responsibility:** the transport handler (typically
//!    the `TaskServer` default federated-pattern handler) MUST
//!    write an `entry_kind=received` row to the
//!    `pattern-aggregation-ledger.jsonl` using the returned
//!    outcome — including the `dropped_reason` for drops, per the
//!    Q3 receiver disposition + Q6 BR-6 observability discipline.
//!
//! # The reference schema is embedded
//!
//! [`FEDERATED_PATTERN_SCHEMA_JSON`] embeds the canonical schema
//! text via `include_str!`. The Rust types in this module are the
//! type-level mirror of the schema; structural validation runs
//! via typed deserialization rather than runtime jsonschema
//! validation (no new Cargo dependency at the a2a-crate layer;
//! schema validation lives in the `neurogrim-sensory`
//! schema-conformance test suite).

use crate::agent_card::AgentCard;
use crate::envelope::{A2aEnvelope, MessageType};
use crate::error::A2aError;
use crate::transport::Transport;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use url::Url;

/// Embedded canonical schema text. The Rust types in this module
/// are the type-level mirror; runtime structural validation runs
/// via typed deserialization. The schema text is included here so
/// any future sensor-level jsonschema validator can pick it up
/// without depending on the LSP-Brains submodule layout.
///
/// v3.2.2: schema vendored into `data/schemas/` so the file resolves
/// in `cargo publish` tarballs (the LSP-Brains sibling repo isn't
/// included in published crates). Canonical source remains
/// `LSP-Brains/schemas/a2a-federated-pattern-v1.schema.json`; drift
/// between the two copies is caught by the schema-conformance tests.
pub const FEDERATED_PATTERN_SCHEMA_JSON: &str =
    include_str!("../data/schemas/a2a-federated-pattern-v1.schema.json");

// =========================================================================
// Closed-set rate-limit / hop-limit constants (Q6 + Q15 locks)
// =========================================================================

/// Hop limit (Q15 lock — sender + 3 relayers = 4). Receiver MUST
/// drop messages whose `origin_set.length > MAX_HOPS`.
pub const MAX_HOPS: usize = 4;

/// Sender-side: maximum concurrent in-flight emits per peer. Q6
/// lock; mirrors the R2-2 pattern from
/// `supply_chain_vigilance/registry.rs:285-297`.
pub const FEDERATED_PATTERN_MAX_INFLIGHT: usize = 2;

/// Sender-side: minimum interval between successive emits to the
/// same peer. Combined with `FEDERATED_PATTERN_MAX_INFLIGHT` this
/// yields ~10 messages per peer per minute. Q6 lock.
pub const FEDERATED_PATTERN_MIN_INTERVAL: Duration = Duration::from_secs(6);

/// Receiver-side: sliding-window size for the per-peer
/// drop-and-log threshold. Q6 lock.
pub const FEDERATED_PATTERN_RECV_WINDOW: Duration = Duration::from_secs(60);

/// Receiver-side: maximum federated-pattern receipts per peer per
/// `FEDERATED_PATTERN_RECV_WINDOW`. Receipts past this threshold
/// are dropped with `dropped_reason=rate-limit-exceeded`. Q6 lock.
pub const FEDERATED_PATTERN_RECV_MAX_PER_WINDOW: usize = 10;

/// Source-level recursion guard prefix (Q9). Pattern_kind values
/// starting with this prefix are the aggregator sensor's
/// meta-finding kinds and MUST NOT round-trip the wire — the
/// sender rejects them and the schema's closed-set enum doesn't
/// permit them at v1 either.
pub const FEDERATED_PATTERNS_FINDING_PREFIX: &str = "federated_patterns:";

// =========================================================================
// Payload types — the type-level mirror of
// `a2a-federated-pattern-v1.schema.json`
// =========================================================================

/// Closed-set v1 pattern-family discriminator (Q14 lock). v1 has
/// exactly ONE entry: `vigilance-pattern`. Future kinds (e.g.,
/// `operator-calibration-pattern`, `hat-contract-pattern`,
/// `trust-budget-pattern`) are v2/v3 candidates per BACKLOG B-23
/// and require additive spec change + schema bump.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum PatternKind {
    VigilancePattern,
}

/// Closed-set severity classification mirroring
/// `a2a-supply-chain-signal-v1.severity_class`. Same closed-set
/// discipline as the supply-chain-signal precedent.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum SeverityClass {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

/// Closed-set numeric-only feature vector (Q1 + Q8 PRIVACY PIN).
/// Bounded numeric features cannot exfiltrate operator-specific
/// patterns the way free-text could. The
/// `#[serde(deny_unknown_fields)]` is the type-level mirror of
/// the schema's `additionalProperties: false` on the FeatureVector
/// definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FeatureVector {
    /// Count of correlated observations underlying the pattern.
    /// Numeric-only; no semantics encoded in non-numeric form.
    pub numeric_count: u32,
    /// Closed-set severity discriminator (mirrors
    /// supply-chain-signal). The only enum-typed feature in the
    /// vector — every other feature is bounded-numeric.
    pub severity_class: SeverityClass,
    /// Window-size in days over which the correlation was
    /// observed. Bounded-numeric; no per-project timing leakage
    /// beyond a single integer.
    pub observation_window_days: u32,
}

/// Federated-pattern payload — the wire-format data carried by an
/// `a2a-envelope-v1` whose `message_type` is `federated-pattern`.
///
/// Structural enforcement of the privacy contract:
///
/// - `#[serde(deny_unknown_fields)]` mirrors the schema's
///   top-level `additionalProperties: false`. Smuggled fields
///   (e.g., a top-level `note`) fail to deserialize.
/// - The closed-set enums (`PatternKind`, `SeverityClass`)
///   reject values outside the v1 vocabulary.
/// - `feature_vector: FeatureVector` is itself
///   `deny_unknown_fields` — free-text smuggled inside the vector
///   fails to deserialize.
///
/// Relaxing any of these requires explicitly re-opening the
/// BR-6 + privacy-under-composition conversation at the charter
/// level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FederatedPatternPayload {
    pub schema_version: String,
    pub pattern_kind: PatternKind,
    pub feature_vector: FeatureVector,
    pub anonymized_origin: String,
    pub origin_set: Vec<String>,
    pub peer_brain_id: String,
    pub discovered_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity_class: Option<SeverityClass>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legal_disclaimer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl FederatedPatternPayload {
    /// Validate required fields per the §16.6.1 schema. Returns an
    /// error string describing the first violation. This pins the
    /// invariants typed-deserialization alone cannot enforce
    /// (non-empty strings; origin_set bounds; schema_version
    /// const).
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != "1" {
            return Err(format!(
                "schema_version must be \"1\"; got {:?}",
                self.schema_version
            ));
        }
        if self.anonymized_origin.trim().is_empty() {
            return Err("anonymized_origin must be non-empty".into());
        }
        if self.peer_brain_id.trim().is_empty() {
            return Err("peer_brain_id must be non-empty".into());
        }
        if self.discovered_at.trim().is_empty() {
            return Err("discovered_at must be non-empty (ISO 8601 UTC)".into());
        }
        if self.origin_set.is_empty() {
            return Err("origin_set must contain at least 1 entry (the originator)".into());
        }
        if self.origin_set.len() > MAX_HOPS {
            return Err(format!(
                "origin_set length {} exceeds MAX_HOPS={} (Q15 hop-limit lock)",
                self.origin_set.len(),
                MAX_HOPS
            ));
        }
        for (idx, entry) in self.origin_set.iter().enumerate() {
            if entry.trim().is_empty() {
                return Err(format!("origin_set[{}] must be non-empty", idx));
            }
        }
        Ok(())
    }
}

// =========================================================================
// Bidirectional opt-in helpers (Q5 — REUSE §16.6 precedent VERBATIM)
// =========================================================================

/// Returns true if BOTH peers declare
/// `MessageType::FederatedPattern` in their respective
/// `accepts[]` AND `emits[]` lists. Q5 lock — mirrors
/// `supply_chain_signal::bidirectional_opt_in_satisfied` verbatim.
///
/// Federation flows only when:
///  - the local Brain emits federated-patterns,
///  - the local Brain accepts federated-patterns,
///  - the peer accepts federated-patterns,
///  - the peer emits federated-patterns.
///
/// Per the brief (C4-2): "Both peers MUST declare `federated-pattern`
/// in their respective `capabilities.accepts[]` AND
/// `capabilities.emits[]`."
pub fn bidirectional_opt_in_satisfied(local: &AgentCard, peer: &AgentCard) -> bool {
    let advertised = |card: &AgentCard| {
        let accepts = card
            .capabilities
            .accepts
            .iter()
            .any(|m| matches!(m, MessageType::FederatedPattern));
        let emits = card
            .capabilities
            .emits
            .iter()
            .any(|m| matches!(m, MessageType::FederatedPattern));
        accepts && emits
    };
    advertised(local) && advertised(peer)
}

// =========================================================================
// Envelope construction
// =========================================================================

/// Build the canonical `A2aEnvelope` carrying a federated-pattern
/// payload. Sets `message_type = MessageType::FederatedPattern`,
/// generates a fresh `message_id` (UUID v4), stamps the current
/// UTC timestamp, and serializes the typed payload as the
/// envelope's `payload` field.
///
/// Returns `A2aError::InvalidEnvelope` if the payload fails
/// `validate()` or fails to serialize. The function does NOT
/// transmit; the caller is responsible for the
/// rate-limit + transport.send sequence.
pub fn build_federated_pattern_envelope(
    payload: FederatedPatternPayload,
    sender_brain_id: impl Into<String>,
) -> Result<A2aEnvelope, A2aError> {
    payload
        .validate()
        .map_err(A2aError::InvalidEnvelope)?;
    let payload_value = serde_json::to_value(&payload).map_err(|e| {
        A2aError::InvalidEnvelope(format!("serialize federated-pattern payload: {}", e))
    })?;
    Ok(A2aEnvelope::new(
        sender_brain_id,
        MessageType::FederatedPattern,
        payload_value,
    ))
}

// =========================================================================
// Sender-side rate limit (Q6 lock — R2-2 pattern)
// =========================================================================

/// Per-peer sender-side rate-limit state. Each peer gets its own
/// instance keyed by `peer_brain_id`. The semaphore caps
/// concurrent in-flight emits at
/// [`FEDERATED_PATTERN_MAX_INFLIGHT`]; the `last_emit_at` tracks
/// the most recent permit-grant time so the
/// [`FEDERATED_PATTERN_MIN_INTERVAL`] minimum interval can be
/// enforced.
#[derive(Debug)]
pub struct PerPeerRateLimit {
    semaphore: Arc<Semaphore>,
    last_emit_at: Mutex<Option<Instant>>,
}

impl PerPeerRateLimit {
    /// Construct a fresh per-peer limiter with the locked v1
    /// constants ([`FEDERATED_PATTERN_MAX_INFLIGHT`] permits +
    /// [`FEDERATED_PATTERN_MIN_INTERVAL`] minimum interval).
    pub fn new() -> Self {
        Self::with_limits(
            FEDERATED_PATTERN_MAX_INFLIGHT,
            FEDERATED_PATTERN_MIN_INTERVAL,
        )
    }

    /// Construct a per-peer limiter with custom limits — used by
    /// tests to keep unit-test wall-clocks small. Production code
    /// should use [`PerPeerRateLimit::new`].
    pub fn with_limits(max_inflight: usize, _min_interval: Duration) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_inflight)),
            last_emit_at: Mutex::new(None),
        }
    }
}

impl Default for PerPeerRateLimit {
    fn default() -> Self {
        Self::new()
    }
}

/// Aggregate rate-limit registry: one `PerPeerRateLimit` per
/// peer brain id. Construct once per Brain instance; share via
/// `Arc::clone`. The `Mutex` protects the lookup-or-insert
/// step — the underlying per-peer semaphore is itself `Arc`'d so
/// concurrent permit acquisitions don't contend the registry
/// lock.
#[derive(Debug, Default)]
pub struct FederatedPatternEmitLimiter {
    peers: Mutex<HashMap<String, Arc<PerPeerRateLimit>>>,
    /// Tunable interval used by `acquire_emit_permit`. Defaults
    /// to [`FEDERATED_PATTERN_MIN_INTERVAL`]; tests may override
    /// to keep wall-clock small.
    min_interval: Duration,
    /// Tunable max-inflight per peer used when minting new
    /// `PerPeerRateLimit` entries. Defaults to
    /// [`FEDERATED_PATTERN_MAX_INFLIGHT`].
    max_inflight: usize,
}

impl FederatedPatternEmitLimiter {
    /// Construct the limiter with v1 locked constants.
    pub fn new() -> Self {
        Self {
            peers: Mutex::new(HashMap::new()),
            min_interval: FEDERATED_PATTERN_MIN_INTERVAL,
            max_inflight: FEDERATED_PATTERN_MAX_INFLIGHT,
        }
    }

    /// Construct the limiter with custom limits — used by tests
    /// to keep wall-clocks small. Production code should use
    /// [`FederatedPatternEmitLimiter::new`].
    pub fn with_limits(max_inflight: usize, min_interval: Duration) -> Self {
        Self {
            peers: Mutex::new(HashMap::new()),
            min_interval,
            max_inflight,
        }
    }

    /// Look up (or create) the per-peer limiter for `peer_brain_id`,
    /// wait for an in-flight permit, and enforce the
    /// `min_interval` since the last permit-grant. Returns an
    /// [`EmitPermit`] whose `Drop` releases the semaphore permit
    /// back to the pool.
    pub async fn acquire_emit_permit(
        &self,
        peer_brain_id: &str,
    ) -> Result<EmitPermit, A2aError> {
        let per_peer = {
            let mut peers = self.peers.lock().await;
            peers
                .entry(peer_brain_id.to_string())
                .or_insert_with(|| {
                    Arc::new(PerPeerRateLimit::with_limits(
                        self.max_inflight,
                        self.min_interval,
                    ))
                })
                .clone()
        };

        // Wait for a semaphore permit (caps concurrent in-flight).
        let permit = per_peer
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| {
                A2aError::Transport(format!("federated-pattern semaphore closed: {}", e))
            })?;

        // Enforce the per-permit minimum interval. We only sleep
        // when the previous emit was within `min_interval` ago;
        // otherwise we proceed immediately. This implements the
        // ~6s per-permit delay from the R2-2 pattern.
        let now = Instant::now();
        let mut last = per_peer.last_emit_at.lock().await;
        if let Some(prev) = *last {
            let elapsed = now.saturating_duration_since(prev);
            if elapsed < self.min_interval {
                let wait = self.min_interval - elapsed;
                drop(last); // release Mutex before sleep so other tasks can read
                tokio::time::sleep(wait).await;
                last = per_peer.last_emit_at.lock().await;
            }
        }
        *last = Some(Instant::now());

        Ok(EmitPermit { _permit: permit })
    }
}

/// RAII guard returned by
/// [`FederatedPatternEmitLimiter::acquire_emit_permit`]. Holding
/// it represents one in-flight emit slot. Drop releases the slot
/// back to the per-peer semaphore.
#[derive(Debug)]
pub struct EmitPermit {
    _permit: OwnedSemaphorePermit,
}

// =========================================================================
// Sender entry point — emit_federated_pattern
// =========================================================================

/// Outcome of a successful federated-pattern emit. The
/// `envelope_message_id` is the UUID v4 the envelope was built
/// with — the caller (CLI) uses it to correlate the
/// `entry_kind=emitted` ledger row with on-the-wire envelope
/// traces (Q12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitOutcome {
    pub envelope_message_id: String,
    pub sent_at: chrono::DateTime<chrono::Utc>,
}

/// Sender-side errors specific to federated-pattern emission. The
/// general transport / envelope errors flow through `A2aError`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("bidirectional opt-in not satisfied — both peers MUST declare `federated-pattern` in their Agent Card capabilities (accepts[] + emits[])")]
    OptInNotSatisfied,

    #[error("source-level recursion guard tripped — pattern_kind values starting with `{prefix}` are reserved for the federated-patterns aggregator sensor's meta-findings (Q9 wire-level + source-level lock); rejected pattern_kind: {pattern_kind}")]
    RecursionGuardSenderSide {
        prefix: &'static str,
        pattern_kind: String,
    },

    #[error("invalid payload: {0}")]
    InvalidPayload(String),

    #[error(transparent)]
    Transport(#[from] A2aError),
}

/// Sender entry point — emit a federated-pattern under the §16.6.1
/// MUST-clauses.
///
/// **Caller responsibility (Q12 log-before-transmit):** the caller
/// MUST write the `entry_kind=emitted` row to the
/// `pattern-aggregation-ledger.jsonl` BEFORE invoking this
/// function. The logging happens at the CLI / sensor layer where
/// the operator audit context lives; this wiring layer is purely
/// the protocol primitive.
///
/// Steps:
///  1. Verify [`bidirectional_opt_in_satisfied`].
///  2. Verify payload well-formedness (`payload.validate()`).
///  3. Source-level recursion guard: reject pattern_kind values
///     starting with [`FEDERATED_PATTERNS_FINDING_PREFIX`].
///  4. Acquire emit permit via the rate-limit registry.
///  5. Build the envelope.
///  6. Send via `transport.post_task`.
///  7. Return [`EmitOutcome`] with the envelope's `message_id`.
pub async fn emit_federated_pattern<T: Transport + ?Sized>(
    local: &AgentCard,
    peer: &AgentCard,
    peer_endpoint: &Url,
    payload: FederatedPatternPayload,
    pattern_kind_wire: &str,
    rate_limit: &FederatedPatternEmitLimiter,
    transport: &T,
) -> Result<EmitOutcome, Error> {
    // Step 1: opt-in check.
    if !bidirectional_opt_in_satisfied(local, peer) {
        return Err(Error::OptInNotSatisfied);
    }

    // Step 2: payload well-formedness.
    payload.validate().map_err(Error::InvalidPayload)?;

    // Step 3: source-level recursion guard. We accept the
    // wire-format pattern_kind string from the caller because the
    // closed-set Rust enum may not yet contain v2/v3 names — this
    // guard is intentionally OPEN so the aggregator's
    // `federated_patterns:*` finding-kind prefix is rejected by
    // string-prefix even when the typed enum doesn't list it.
    if pattern_kind_wire.starts_with(FEDERATED_PATTERNS_FINDING_PREFIX) {
        return Err(Error::RecursionGuardSenderSide {
            prefix: FEDERATED_PATTERNS_FINDING_PREFIX,
            pattern_kind: pattern_kind_wire.to_string(),
        });
    }

    // Step 4: acquire emit permit.
    let _permit = rate_limit
        .acquire_emit_permit(&payload.peer_brain_id)
        .await?;

    // Step 5: build envelope.
    let envelope = build_federated_pattern_envelope(payload, &local.id)?;
    let envelope_message_id = envelope.message_id.clone();

    // Step 6: send.
    transport.post_task(peer_endpoint, &envelope).await?;

    // Step 7: return outcome. Permit is released as `_permit`
    // drops at end of scope.
    Ok(EmitOutcome {
        envelope_message_id,
        sent_at: chrono::Utc::now(),
    })
}

// =========================================================================
// Receiver-side rate limit (Q6 lock — sliding-window counter)
// =========================================================================

/// Per-peer receiver-side sliding-window state. Each peer gets
/// its own instance keyed by the sender's opaque
/// `anonymized_origin` (or `peer_brain_id` on the inbound
/// envelope). Receipts past
/// [`FEDERATED_PATTERN_RECV_MAX_PER_WINDOW`] within
/// [`FEDERATED_PATTERN_RECV_WINDOW`] are dropped.
#[derive(Debug)]
pub struct PerPeerReceiveRateLimit {
    window: Mutex<VecDeque<Instant>>,
    max_per_window: usize,
    window_duration: Duration,
}

impl PerPeerReceiveRateLimit {
    pub fn new() -> Self {
        Self::with_limits(
            FEDERATED_PATTERN_RECV_MAX_PER_WINDOW,
            FEDERATED_PATTERN_RECV_WINDOW,
        )
    }

    pub fn with_limits(max_per_window: usize, window_duration: Duration) -> Self {
        Self {
            window: Mutex::new(VecDeque::new()),
            max_per_window,
            window_duration,
        }
    }
}

impl Default for PerPeerReceiveRateLimit {
    fn default() -> Self {
        Self::new()
    }
}

/// Aggregate receiver-side rate-limit registry. Same shape as
/// [`FederatedPatternEmitLimiter`] but with sliding-window
/// counters instead of semaphores.
#[derive(Debug, Default)]
pub struct FederatedPatternReceiveLimiter {
    peers: Mutex<HashMap<String, Arc<PerPeerReceiveRateLimit>>>,
    max_per_window: usize,
    window_duration: Duration,
}

impl FederatedPatternReceiveLimiter {
    pub fn new() -> Self {
        Self {
            peers: Mutex::new(HashMap::new()),
            max_per_window: FEDERATED_PATTERN_RECV_MAX_PER_WINDOW,
            window_duration: FEDERATED_PATTERN_RECV_WINDOW,
        }
    }

    pub fn with_limits(max_per_window: usize, window_duration: Duration) -> Self {
        Self {
            peers: Mutex::new(HashMap::new()),
            max_per_window,
            window_duration,
        }
    }

    /// Sliding-window admission: append `now` to the per-peer
    /// deque, drop entries older than `window_duration`, and
    /// return [`AdmitOutcome::Dropped`] if the deque exceeds
    /// `max_per_window`. Otherwise [`AdmitOutcome::Accepted`].
    pub async fn admit_receipt(&self, peer_brain_id: &str) -> AdmitOutcome {
        let per_peer = {
            let mut peers = self.peers.lock().await;
            peers
                .entry(peer_brain_id.to_string())
                .or_insert_with(|| {
                    Arc::new(PerPeerReceiveRateLimit::with_limits(
                        self.max_per_window,
                        self.window_duration,
                    ))
                })
                .clone()
        };

        let mut window = per_peer.window.lock().await;
        let now = Instant::now();
        // Drop entries older than the window.
        let cutoff = now.checked_sub(per_peer.window_duration).unwrap_or(now);
        while let Some(&front) = window.front() {
            if front < cutoff {
                window.pop_front();
            } else {
                break;
            }
        }
        // Append the new receipt.
        window.push_back(now);
        if window.len() > per_peer.max_per_window {
            // Drop the just-added entry to keep the deque bounded
            // for the next admission.
            window.pop_back();
            AdmitOutcome::Dropped
        } else {
            AdmitOutcome::Accepted
        }
    }
}

/// Result of a receiver-side admission check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmitOutcome {
    Accepted,
    Dropped,
}

// =========================================================================
// Receiver entry point — handle_received_federated_pattern
// =========================================================================

/// Closed-set drop reasons mirroring the `DroppedReason` enum in
/// `pattern-aggregation-ledger-v1.schema.json`. Q4 + Q6 + Q11 +
/// Q15 lock — new vocabulary terms require a spec change with an
/// explicit METHODOLOGY-EVOLUTION entry.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum DroppedReason {
    /// Q6 BR-6 mitigation: peer exceeded the sliding-window
    /// receipt threshold.
    RateLimitExceeded,
    /// Q9 wire-level recursion guard: receiver's own opaque-hash
    /// was in `origin_set[]`.
    RecursionGuard,
    /// Q3: payload failed federated-pattern schema validation
    /// (typed-deserialization or payload.validate()).
    SchemaValidationFailed,
    /// Q15: `origin_set.length > MAX_HOPS=4`.
    HopLimitExceeded,
    /// Q11 cross-version compat: receiver does not understand
    /// the value of `pattern_kind`. NOT an error — graceful
    /// degradation for forward-compat with v2/v3 pattern_kinds.
    UnknownPatternKind,
}

impl DroppedReason {
    /// Wire-format string mirroring the schema's closed-set
    /// `DroppedReason` enum.
    pub fn wire_name(&self) -> &'static str {
        match self {
            DroppedReason::RateLimitExceeded => "rate-limit-exceeded",
            DroppedReason::RecursionGuard => "recursion-guard",
            DroppedReason::SchemaValidationFailed => "schema-validation-failed",
            DroppedReason::HopLimitExceeded => "hop-limit-exceeded",
            DroppedReason::UnknownPatternKind => "unknown-pattern-kind",
        }
    }
}

/// Result of [`handle_received_federated_pattern`]. The caller
/// (transport handler) writes an `entry_kind=received` row to
/// the `pattern-aggregation-ledger.jsonl` regardless of outcome
/// — drops are observability per Q3 + Q6.
#[derive(Debug, Clone)]
pub enum ReceiveOutcome {
    Accepted(FederatedPatternPayload),
    Dropped {
        reason: DroppedReason,
        /// Best-effort parsed payload (when available) so the
        /// caller can include it in the ledger row for operator
        /// audit. `None` when typed-deserialization itself
        /// failed (`SchemaValidationFailed`).
        payload: Option<FederatedPatternPayload>,
    },
}

impl ReceiveOutcome {
    /// True if the receiver should treat the message as accepted
    /// (proceed to aggregator-sensor visibility).
    pub fn is_accepted(&self) -> bool {
        matches!(self, ReceiveOutcome::Accepted(_))
    }

    /// The structured drop reason for `Dropped` outcomes. `None`
    /// for `Accepted`.
    pub fn dropped_reason(&self) -> Option<DroppedReason> {
        match self {
            ReceiveOutcome::Accepted(_) => None,
            ReceiveOutcome::Dropped { reason, .. } => Some(*reason),
        }
    }
}

/// Wrong-message-type error returned by
/// [`handle_received_federated_pattern`] when the envelope's
/// `message_type` is not `federated-pattern`. The caller is
/// expected to dispatch by `message_type` BEFORE calling this
/// function; this is defense-in-depth.
#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
    #[error("envelope message_type must be `federated-pattern`; got {0:?}")]
    WrongMessageType(MessageType),
}

/// Receiver entry point — process an inbound federated-pattern
/// envelope under the §16.6.1 MUST-clauses.
///
/// **Caller responsibility:** the transport handler MUST write an
/// `entry_kind=received` row to the
/// `pattern-aggregation-ledger.jsonl` using the returned
/// outcome — including the `dropped_reason` for drops, per the
/// Q3 receiver disposition + Q6 BR-6 observability discipline.
///
/// Returns `HandlerError::WrongMessageType` when the envelope's
/// `message_type` is not `federated-pattern`. Otherwise returns
/// [`ReceiveOutcome::Accepted`] with the typed payload, or
/// [`ReceiveOutcome::Dropped`] with the structured drop reason.
pub async fn handle_received_federated_pattern(
    inbound: &A2aEnvelope,
    local_brain_id_hash: &str,
    receive_rate_limit: &FederatedPatternReceiveLimiter,
) -> Result<ReceiveOutcome, HandlerError> {
    // Step 1: defense-in-depth message-type check. The transport
    // handler dispatches by message_type before calling us; this
    // is belt-and-braces.
    if !matches!(inbound.message_type, MessageType::FederatedPattern) {
        return Err(HandlerError::WrongMessageType(inbound.message_type));
    }

    // Step 2: typed-deserialization is the schema-validation
    // step. The closed-set `PatternKind` enum is intentionally
    // OPEN at this layer — we accept any string for pattern_kind
    // by deserializing into a permissive type first, then doing
    // the closed-set check in step 6 to deliver the
    // forward-compat semantics from Q11. Required-field checks +
    // additionalProperties: false (via deny_unknown_fields) +
    // closed-set FeatureVector / SeverityClass are enforced at
    // typed-deserialize time.
    //
    // We do this in two passes: first deserialize into a
    // version that uses `String` for pattern_kind (so unknown
    // pattern_kinds don't fail typed-deserialization), then
    // check structure + closed-set membership.
    let permissive: PermissiveFederatedPatternPayload =
        match serde_json::from_value(inbound.payload.clone()) {
            Ok(p) => p,
            Err(_) => {
                return Ok(ReceiveOutcome::Dropped {
                    reason: DroppedReason::SchemaValidationFailed,
                    payload: None,
                });
            }
        };

    // Validate non-typed invariants (origin_set bounds, non-empty
    // strings, schema_version const).
    if let Err(_e) = permissive.validate_structure() {
        return Ok(ReceiveOutcome::Dropped {
            reason: DroppedReason::SchemaValidationFailed,
            payload: None,
        });
    }

    // Step 3: wire-level recursion guard (Q9). Drop if our own
    // opaque-hash appears in origin_set[].
    if permissive
        .origin_set
        .iter()
        .any(|h| h == local_brain_id_hash)
    {
        return Ok(ReceiveOutcome::Dropped {
            reason: DroppedReason::RecursionGuard,
            payload: permissive.try_into_typed().ok(),
        });
    }

    // Step 4: hop limit (Q15). The schema enforces maxItems 4 at
    // the schema layer too; this is defense-in-depth for tampered
    // payloads that bypassed schema validation upstream.
    if permissive.origin_set.len() > MAX_HOPS {
        return Ok(ReceiveOutcome::Dropped {
            reason: DroppedReason::HopLimitExceeded,
            payload: permissive.try_into_typed().ok(),
        });
    }

    // Step 5: receiver-side rate limit (Q6). Sliding-window
    // counter per-peer.
    if matches!(
        receive_rate_limit
            .admit_receipt(&permissive.anonymized_origin)
            .await,
        AdmitOutcome::Dropped
    ) {
        return Ok(ReceiveOutcome::Dropped {
            reason: DroppedReason::RateLimitExceeded,
            payload: permissive.try_into_typed().ok(),
        });
    }

    // Step 6: pattern-kind closed-set check (Q11 forgiveness).
    // Unknown pattern_kinds are dropped — NOT an error.
    let typed = match permissive.try_into_typed() {
        Ok(t) => t,
        Err(PatternKindError::Unknown(_)) => {
            return Ok(ReceiveOutcome::Dropped {
                reason: DroppedReason::UnknownPatternKind,
                payload: None,
            });
        }
    };

    Ok(ReceiveOutcome::Accepted(typed))
}

// =========================================================================
// Permissive-payload helper — used internally by the receiver
// =========================================================================

/// Permissive variant of [`FederatedPatternPayload`] that uses
/// `String` for `pattern_kind` and `severity_class` — used so
/// unknown enum values map to `DroppedReason::UnknownPatternKind`
/// (graceful degradation per Q11) rather than
/// `DroppedReason::SchemaValidationFailed`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PermissiveFederatedPatternPayload {
    schema_version: String,
    pattern_kind: String,
    feature_vector: PermissiveFeatureVector,
    anonymized_origin: String,
    origin_set: Vec<String>,
    peer_brain_id: String,
    discovered_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    severity_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    legal_disclaimer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PermissiveFeatureVector {
    numeric_count: u32,
    severity_class: String,
    observation_window_days: u32,
}

#[derive(Debug)]
enum PatternKindError {
    /// The carried `String` is read via the `Debug` impl when the
    /// caller logs an unknown-pattern-kind drop; `dead_code` lints
    /// don't see that path so we explicitly silence here.
    #[allow(dead_code)]
    Unknown(String),
}

impl PermissiveFederatedPatternPayload {
    fn validate_structure(&self) -> Result<(), String> {
        if self.schema_version != "1" {
            return Err(format!(
                "schema_version must be \"1\"; got {:?}",
                self.schema_version
            ));
        }
        if self.anonymized_origin.trim().is_empty() {
            return Err("anonymized_origin must be non-empty".into());
        }
        if self.peer_brain_id.trim().is_empty() {
            return Err("peer_brain_id must be non-empty".into());
        }
        if self.discovered_at.trim().is_empty() {
            return Err("discovered_at must be non-empty".into());
        }
        if self.origin_set.is_empty() {
            return Err("origin_set must be non-empty".into());
        }
        if self.feature_vector.observation_window_days < 1 {
            return Err("feature_vector.observation_window_days must be ≥ 1".into());
        }
        Ok(())
    }

    fn try_into_typed(&self) -> Result<FederatedPatternPayload, PatternKindError> {
        let pattern_kind = match self.pattern_kind.as_str() {
            "vigilance-pattern" => PatternKind::VigilancePattern,
            other => return Err(PatternKindError::Unknown(other.to_string())),
        };
        let feature_severity = parse_severity(&self.feature_vector.severity_class)
            .map_err(|s| PatternKindError::Unknown(format!("feature_vector.severity_class: {}", s)))?;
        let opt_severity = match &self.severity_class {
            None => None,
            Some(s) => Some(
                parse_severity(s).map_err(|s| {
                    PatternKindError::Unknown(format!("severity_class: {}", s))
                })?,
            ),
        };
        Ok(FederatedPatternPayload {
            schema_version: self.schema_version.clone(),
            pattern_kind,
            feature_vector: FeatureVector {
                numeric_count: self.feature_vector.numeric_count,
                severity_class: feature_severity,
                observation_window_days: self.feature_vector.observation_window_days,
            },
            anonymized_origin: self.anonymized_origin.clone(),
            origin_set: self.origin_set.clone(),
            peer_brain_id: self.peer_brain_id.clone(),
            discovered_at: self.discovered_at.clone(),
            severity_class: opt_severity,
            legal_disclaimer: self.legal_disclaimer.clone(),
            metadata: self.metadata.clone(),
        })
    }
}

fn parse_severity(s: &str) -> Result<SeverityClass, String> {
    match s {
        "info" => Ok(SeverityClass::Info),
        "low" => Ok(SeverityClass::Low),
        "medium" => Ok(SeverityClass::Medium),
        "high" => Ok(SeverityClass::High),
        "critical" => Ok(SeverityClass::Critical),
        other => Err(other.to_string()),
    }
}
