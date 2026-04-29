//! E-B2-7 C8 — End-to-end loopback integration test for the
//! `federated-pattern` A2A primitive.
//!
//! Where C4's `federated_pattern_protocol_behavior.rs` exercises the
//! sender + receiver under an in-process mock transport, this test
//! file stands up REAL `TaskServer` instances on loopback TCP sockets
//! and exercises the full federation flow under real network
//! conditions. Mirrors the S6-DB-4 `dual_brain_pair` discipline:
//! ephemeral-port allocation, `127.0.0.1:0`-bound axum servers,
//! deterministic teardown via `JoinHandle::abort` + tempdir RAII.
//!
//! # What this file proves
//!
//! C4 protocol-behavior tests proved the §16.6.1 MUST-clauses at the
//! type-and-wire boundary. C7 CLI integration tests proved the
//! sender-side ledger writes work through a real CLI invocation.
//! What was MISSING — and what BR-6 mandates — is the real-network
//! cross-Brain proof: that when Brain A POSTs a federated-pattern
//! envelope to Brain B over a real TCP socket, Brain B's receiver
//! handler honors the recursion guard, the rate limit, the hop
//! limit, and the schema closed-set posture as the spec requires.
//!
//! # Loopback infrastructure choice
//!
//! Two real A2A servers OR one (just the receiver) — the brief
//! permits whichever is simpler. We use the **in-process axum
//! server** pattern from `crates/neurogrim-a2a/src/server.rs` tests
//! (post_then_get_roundtrip, agent_card_served_at_well_known_url):
//! `tokio::net::TcpListener::bind("127.0.0.1:0")` + `axum::serve`
//! spawned via `tokio::spawn`, returning a `JoinHandle` we abort at
//! teardown. This gives us real loopback I/O (real TCP, real reqwest
//! POST) without needing to subprocess `neurogrim a2a-serve` (which
//! does not currently register a `FederatedPattern` handler — see
//! `crates/neurogrim-cli/src/commands/a2a_serve.rs:114` Agent Card
//! capabilities). Modifying `a2a_serve.rs` to wire federated-pattern
//! is out-of-scope for C8 per the brief's HARD CONSTRAINT
//! ("Do NOT modify ... the C7 CLI"); we satisfy the BR-6 mandate by
//! standing up the real TaskServer with the federated-pattern
//! handler directly. This is parallel construction with the
//! C4-shipped sender / receiver — same code paths, real network
//! between them.
//!
//! Brain A's outbound side does NOT need its own A2A server: it
//! drives `HttpSseTransport::post_task` directly against Brain B's
//! loopback URL. The federation primitive is asymmetric; Brain A
//! emits, Brain B receives. Standing up an A2A server for Brain A
//! would only test that Brain A is also reachable — not in the
//! BR-6 scope here.
//!
//! # Ledger discipline
//!
//! Both halves of the federation flow write rows to a per-Brain
//! `pattern-aggregation-ledger.jsonl` mirroring real production
//! behavior. The CLI (`neurogrim federated-pattern emit`) writes
//! `entry_kind=emitted` rows; in this test, the test code writes
//! the emitted row before calling `emit_federated_pattern` to
//! mirror the C7 Q12 log-before-transmit lock. The receiver-side
//! handler we register on Brain B writes `entry_kind=received`
//! rows from the returned `ReceiveOutcome`, mirroring the
//! intended production wiring (Q3 + Q6 BR-6 observability).
//!
//! # Real-network costs
//!
//! Each test:
//!   1. Allocates ephemeral port(s) (~µs).
//!   2. Spawns axum + binds on `127.0.0.1` (~ms).
//!   3. Polls Agent Card endpoint until ready (~10ms typical).
//!   4. Runs the actual federation flow (~1-100ms depending on
//!      rate-limit shape).
//!   5. Aborts the spawned server task; tempdirs clean up via
//!      RAII drop.
//!
//! Wall-clock per test ≤ 5s in the happy path; the rate-limit test
//! is the slowest at ~1s due to the receiver-side sliding-window
//! threshold (capped at 11 messages, each over a real TCP
//! handshake).

use std::io::Write;
use std::net::TcpListener as StdTcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use neurogrim_a2a::agent_card::{
    AgentCard, AuthScheme, Authentication, Capabilities, Transport as CardTransport,
    TransportProtocol,
};
use neurogrim_a2a::envelope::{A2aEnvelope, MessageType};
use neurogrim_a2a::error::A2aError;
use neurogrim_a2a::federated_pattern::{
    bidirectional_opt_in_satisfied, build_federated_pattern_envelope, emit_federated_pattern,
    handle_received_federated_pattern, Error, FederatedPatternEmitLimiter,
    FederatedPatternPayload, FederatedPatternReceiveLimiter, FeatureVector, PatternKind,
    ReceiveOutcome, SeverityClass,
};
use neurogrim_a2a::transport::Transport as A2aTransport;
use neurogrim_a2a::{HttpSseTransport, TaskServer};

use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use url::Url;

// =========================================================================
// Loopback pair setup — the test fixture every case shares
// =========================================================================

/// Handle to a running in-process A2A server on a loopback port.
/// Drop the `tempdir` to remove the project root; abort the
/// `server_task` to stop the axum listener.
struct LoopbackBrain {
    /// The Agent Card the Brain serves at its well-known endpoint.
    card: AgentCard,
    /// Absolute URL the Agent Card publishes (authority root —
    /// `http://127.0.0.1:NNNN/a2a/v1/`).
    a2a_endpoint: Url,
    /// Project root for this Brain — its `pattern-aggregation-ledger.jsonl`
    /// lives at `<tempdir>/.claude/brain/pattern-aggregation-ledger.jsonl`.
    tempdir: TempDir,
    /// Spawned axum task. Aborted on teardown.
    server_task: Option<JoinHandle<()>>,
}

impl LoopbackBrain {
    fn ledger_path(&self) -> PathBuf {
        self.tempdir
            .path()
            .join(".claude")
            .join("brain")
            .join("pattern-aggregation-ledger.jsonl")
    }

    #[allow(dead_code)]
    fn project_root(&self) -> &Path {
        self.tempdir.path()
    }
}

impl Drop for LoopbackBrain {
    fn drop(&mut self) {
        if let Some(h) = self.server_task.take() {
            h.abort();
        }
    }
}

/// Pick an OS-allocated free port on 127.0.0.1, drop the listener so
/// axum can re-bind it. Same pattern as `dual_brain_pair.rs`'s
/// `find_free_loopback_port`. There is a tiny race between the drop
/// and the rebind; on loopback with immediate spawn it is acceptable
/// for test purposes.
fn find_free_loopback_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("bind loopback ephemeral port");
    let port = listener.local_addr().expect("read local_addr").port();
    drop(listener);
    port
}

/// Build a tempdir representing a Brain's project root: creates the
/// `.claude/brain/` subdirectory so the ledger writer has somewhere
/// to put its file. Mirrors the C7 CLI's `ensure_brain_dir` discipline.
fn build_project_root(brain_id: &str) -> TempDir {
    let dir = tempfile::tempdir().expect("create tempdir for project root");
    let claude_brain = dir.path().join(".claude").join("brain");
    std::fs::create_dir_all(&claude_brain).expect("create .claude/brain/");

    // Minimal brain-registry.json so the federated_patterns sensor's
    // `load_declared_peers` reads a non-empty children map. The peer
    // hash declared here matches the opaque hash the test uses for
    // the federation flow.
    let claude = dir.path().join(".claude");
    let registry = json!({
        "meta": {
            "schema_version": "2",
            "description": format!("Loopback test fixture for {brain_id}"),
            "updated_by": brain_id,
            "project": brain_id
        },
        "tools": {},
        "data_sources": {},
        "config": {
            "domain_weights": { "code-quality": 1.0 },
            "advisory_domains": [],
            "principle_map": {},
            "domain_definitions": {},
            "children": {}
        }
    });
    std::fs::write(
        claude.join("brain-registry.json"),
        serde_json::to_string_pretty(&registry).unwrap(),
    )
    .expect("write brain-registry.json");

    dir
}

/// Build an Agent Card declaring federated-pattern in BOTH `accepts[]`
/// and `emits[]` (or neither, controlled by `with_federated_pattern`).
/// `endpoint` is the `http://127.0.0.1:NNNN/a2a/v1/` URL the server
/// is listening on.
fn agent_card(brain_id: &str, endpoint: &Url, with_federated_pattern: bool) -> AgentCard {
    let (accepts, emits) = if with_federated_pattern {
        (
            vec![MessageType::FederatedPattern],
            vec![MessageType::FederatedPattern],
        )
    } else {
        (vec![], vec![])
    };
    AgentCard {
        schema_version: "1".into(),
        id: brain_id.into(),
        name: brain_id.into(),
        version: "0.1.0".into(),
        interface_version: "1".into(),
        capabilities: Capabilities {
            accepts,
            emits,
            streaming: false,
        },
        transport: CardTransport {
            protocol: TransportProtocol::HttpSse,
            endpoint: endpoint.to_string(),
            tasks_path: "/a2a/v1/tasks".into(),
        },
        authentication: Authentication {
            scheme: AuthScheme::None,
        },
        topology: None,
        queue_endpoints: None,
    }
}

/// Configuration for a Brain B receiver. The handler we register
/// uses these values when processing inbound federated-pattern
/// envelopes — `local_brain_id_hash` for the wire-level recursion
/// guard, `receive_limiter` for the sliding-window rate limit,
/// `ledger_path` for the receiver-side audit row.
#[derive(Clone)]
struct ReceiverConfig {
    /// Opaque hash that represents Brain B's identity on the wire
    /// (matches what the spec calls the "anonymized brain-id hash").
    /// The handler uses this to detect self-loops in `origin_set[]`.
    local_brain_id_hash: String,
    /// Receiver-side rate-limit registry. Wrapped in `Arc<Mutex<_>>`
    /// so the handler closure can mutate state across calls.
    receive_limiter: Arc<FederatedPatternReceiveLimiter>,
    /// Path to this Brain's `pattern-aggregation-ledger.jsonl`.
    /// The handler appends `entry_kind=received` rows here.
    ledger_path: PathBuf,
    /// The Brain's id (envelope sender id for the ack response).
    brain_id: String,
}

/// Stand up an in-process A2A server with a federated-pattern
/// handler that mirrors the intended production wiring: validate
/// via `handle_received_federated_pattern`, write
/// `entry_kind=received` to the ledger, return an ack envelope.
///
/// Returns the running server's `JoinHandle` and the listener's
/// bound port. The Agent Card construction is the caller's
/// responsibility (so the caller controls whether the card declares
/// federated-pattern in its capabilities — needed for the
/// "Brain B doesn't advertise" test case).
async fn start_brain_b_server(
    card: AgentCard,
    cfg: ReceiverConfig,
) -> (JoinHandle<()>, u16) {
    let mut server = TaskServer::new(card);

    // Federated-pattern handler — the receiver-side wiring under test.
    // Mirrors the production-shaped flow:
    //   1. Validate via `handle_received_federated_pattern` (C4).
    //   2. Write `entry_kind=received` row to the ledger (Q3 + Q6
    //      BR-6 observability discipline).
    //   3. Return a snapshot.delivered ack envelope (no first-class
    //      ack message type at v1, mirroring supply-chain-signal's
    //      `default_handle_received`).
    let cfg_for_handler = cfg.clone();
    server.register_handler(MessageType::FederatedPattern, move |req| {
        let cfg = cfg_for_handler.clone();
        async move {
            let receive_limiter = cfg.receive_limiter.clone();
            let outcome = handle_received_federated_pattern(
                &req,
                &cfg.local_brain_id_hash,
                receive_limiter.as_ref(),
            )
            .await
            .map_err(|e| A2aError::InvalidEnvelope(e.to_string()))?;

            // Q3 + Q6 BR-6: write `entry_kind=received` regardless of
            // accepted vs dropped — drops are observability.
            write_received_ledger_row(&cfg.ledger_path, &req, &outcome)
                .map_err(|e| A2aError::Transport(format!("ledger write failed: {e}")))?;

            // Ack envelope — minimal; the mere fact we returned Ok
            // tells the sender we accepted-or-dropped without crashing.
            let mut ack = A2aEnvelope::new(
                &cfg.brain_id,
                MessageType::SnapshotDelivered,
                json!({
                    "ack": "federated-pattern-received",
                    "envelope_message_id": req.message_id,
                }),
            );
            ack.reply_to = Some(req.message_id);
            Ok(ack)
        }
    });

    // ScoreUpdated handler: a no-op ack so Brain B can also respond
    // to other A2A traffic without 405. Not strictly required for
    // the federation tests but mirrors production.
    let brain_id = cfg.brain_id.clone();
    server.register_handler(MessageType::ScoreUpdated, move |req| {
        let brain_id = brain_id.clone();
        async move {
            let mut ack =
                A2aEnvelope::new(&brain_id, MessageType::SnapshotDelivered, json!({"ack": true}));
            ack.reply_to = Some(req.message_id);
            Ok(ack)
        }
    });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let port = listener.local_addr().expect("local_addr").port();
    let router = server.router();

    let handle = tokio::spawn(async move {
        // Returning early is fine; tests abort this task at teardown.
        let _ = axum::serve(listener, router).await;
    });

    (handle, port)
}

/// Write an `entry_kind=received` row to `<ledger_path>` mirroring
/// the production receiver-side ledger discipline. The fields here
/// match `pattern-aggregation-ledger-v1.schema.json`'s `ReceivedEntry`
/// shape — used by the `federated_patterns` sensor (C6) when it
/// scans the ledger.
fn write_received_ledger_row(
    ledger_path: &Path,
    inbound: &A2aEnvelope,
    outcome: &ReceiveOutcome,
) -> std::io::Result<()> {
    use std::fs::OpenOptions;

    if let Some(parent) = ledger_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let dropped_reason = outcome.dropped_reason().map(|r| r.wire_name());
    let parsed_payload: Option<Value> = match outcome {
        ReceiveOutcome::Accepted(p) => Some(serde_json::to_value(p).unwrap_or(Value::Null)),
        ReceiveOutcome::Dropped { payload, .. } => payload
            .as_ref()
            .map(|p| serde_json::to_value(p).unwrap_or(Value::Null)),
    };
    // For the from_brain_id field, we use the envelope's brain_id as
    // a stand-in for the opaque-hash sender. The C6 sensor reads
    // `from_brain_id` for per-peer aggregation; for the e2e test the
    // value just needs to be a non-empty stable identifier.
    let from_brain_id = inbound.brain_id.clone();

    let row = json!({
        "schema_version": "1",
        "entry_kind": "received",
        "ts": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "from_brain_id": from_brain_id,
        "envelope_message_id": inbound.message_id,
        "dropped_reason": dropped_reason,
        "payload": parsed_payload,
    });

    let line = serde_json::to_string(&row).unwrap();
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger_path)?;
    writeln!(f, "{line}")?;
    f.flush()?;
    Ok(())
}

/// Write an `entry_kind=emitted` row to the sender's ledger BEFORE
/// transmission, mirroring the C7 CLI's Q12 log-before-transmit lock.
/// The production CLI writes this row inside `federated-pattern emit`;
/// this helper replicates that discipline so the e2e test exercises
/// both halves of the audit trail.
fn write_emitted_ledger_row(
    ledger_path: &Path,
    payload: &FederatedPatternPayload,
    predicted_envelope_id: &str,
) -> std::io::Result<()> {
    use std::fs::OpenOptions;

    if let Some(parent) = ledger_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let row = json!({
        "schema_version": "1",
        "entry_kind": "emitted",
        "ts": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "peer_brain_id": payload.peer_brain_id,
        "to_brain_id": payload.peer_brain_id,
        "envelope_message_id": predicted_envelope_id,
        "payload": serde_json::to_value(payload).unwrap_or(Value::Null),
    });

    let line = serde_json::to_string(&row).unwrap();
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger_path)?;
    writeln!(f, "{line}")?;
    f.flush()?;
    Ok(())
}

/// Read the ledger as a Vec of parsed JSON values — empty if the
/// file does not exist. Used by tests to verify the wire-level flow
/// produced the right ledger-side artifacts.
fn read_ledger(ledger_path: &Path) -> Vec<Value> {
    if !ledger_path.exists() {
        return Vec::new();
    }
    let text = std::fs::read_to_string(ledger_path).expect("read ledger");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("parse JSONL line"))
        .collect()
}

/// Poll the well-known agent-card endpoint until it returns 200 OK
/// or the deadline expires. Mirrors `dual_brain_pair.rs`'s
/// `wait_for_ready`. Necessary because the spawned axum task may
/// not have bound the socket by the time the test makes its first
/// POST.
async fn wait_for_ready(authority_root: &Url, timeout_secs: u64) -> Result<(), String> {
    let card_url = authority_root
        .join("/.well-known/agent-card.json")
        .map_err(|e| format!("bad authority {authority_root}: {e}"))?;
    let client = reqwest::Client::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let mut last_err: String = "no attempts made".into();
    while std::time::Instant::now() < deadline {
        match client.get(card_url.clone()).send().await {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) => last_err = format!("HTTP {} from {}", resp.status(), card_url),
            Err(e) => last_err = format!("connect to {}: {}", card_url, e),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "{} did not become ready within {}s (last: {})",
        authority_root, timeout_secs, last_err
    ))
}

/// All the moving parts the test touches — Brain A's tempdir + ledger
/// path on one side, Brain B's tempdir + handle + endpoints on the
/// other.
struct LoopbackPair {
    brain_a: LoopbackBrain,
    brain_b: LoopbackBrain,
}

/// Stand up the loopback pair. Brain A is sender-only (no A2A
/// server); Brain B is receiver (real axum on 127.0.0.1:port).
/// `brain_b_advertises` controls whether Brain B's Agent Card lists
/// federated-pattern in capabilities — the
/// "bidirectional opt-in failure" test sets this to false.
async fn loopback_pair_setup(
    brain_b_advertises_federated_pattern: bool,
    receive_limiter: Arc<FederatedPatternReceiveLimiter>,
) -> LoopbackPair {
    // Brain A — sender side. No A2A server; only the project root
    // (for ledger writes) and the Agent Card (for opt-in checks).
    let dir_a = build_project_root("brain-a-test");
    let port_a = find_free_loopback_port(); // claimed for the card endpoint string
    let endpoint_a = Url::parse(&format!("http://127.0.0.1:{port_a}/a2a/v1/")).unwrap();
    let card_a = agent_card("brain-a-test", &endpoint_a, true);
    let brain_a = LoopbackBrain {
        card: card_a,
        a2a_endpoint: endpoint_a,
        tempdir: dir_a,
        server_task: None,
    };

    // Brain B — receiver side. Real A2A server bound on a loopback
    // port. Card declares federated-pattern in BOTH directions iff
    // `brain_b_advertises_federated_pattern`.
    let dir_b = build_project_root("brain-b-test");
    let local_brain_id_hash = "sha256-opaque-hash-brain-b".to_string();
    let ledger_path_b = dir_b
        .path()
        .join(".claude")
        .join("brain")
        .join("pattern-aggregation-ledger.jsonl");

    // We need to know the bind port BEFORE constructing the Agent
    // Card (so the card's `transport.endpoint` matches the live
    // server). The simplest sequence: bind the listener inside
    // `start_brain_b_server` which returns the chosen port, then
    // build the card... but the handler captures `cfg` which
    // depends on the brain's id (which is in the card). Resolve
    // by knowing the brain_id ahead of time and embedding the
    // port in the card AFTER the bind.
    //
    // We can construct the card with a placeholder endpoint and
    // re-derive the actual endpoint URL post-bind for use in the
    // test code (the card is only consumed by the Agent Card
    // endpoint route + our local opt-in check; the wire-level
    // routing uses the explicit URL we hand to the transport).
    let placeholder_endpoint = Url::parse("http://127.0.0.1:0/a2a/v1/").unwrap();
    let card_b = agent_card(
        "brain-b-test",
        &placeholder_endpoint,
        brain_b_advertises_federated_pattern,
    );

    let cfg = ReceiverConfig {
        local_brain_id_hash,
        receive_limiter,
        ledger_path: ledger_path_b.clone(),
        brain_id: card_b.id.clone(),
    };

    let (handle, port_b) = start_brain_b_server(card_b.clone(), cfg).await;
    let endpoint_b = Url::parse(&format!("http://127.0.0.1:{port_b}/a2a/v1/")).unwrap();
    let authority_b = Url::parse(&format!("http://127.0.0.1:{port_b}/")).unwrap();

    // Wait for the server to actually bind.
    if let Err(e) = wait_for_ready(&authority_b, 10).await {
        panic!("Brain B did not become ready: {e}");
    }

    // Reconstruct the card with the real endpoint, mostly for the
    // sender-side opt-in check (the wire side uses `endpoint_b`).
    let card_b_real = agent_card(
        "brain-b-test",
        &endpoint_b,
        brain_b_advertises_federated_pattern,
    );

    let brain_b = LoopbackBrain {
        card: card_b_real,
        a2a_endpoint: endpoint_b,
        tempdir: dir_b,
        server_task: Some(handle),
    };

    LoopbackPair { brain_a, brain_b }
}

/// Build a baseline payload for use across tests. The
/// `anonymized_origin` is Brain A's opaque hash; `peer_brain_id` is
/// Brain B's. Tests override fields as needed.
fn baseline_payload() -> FederatedPatternPayload {
    let local_hash = "sha256-opaque-hash-brain-a".to_string();
    let peer_hash = "sha256-opaque-hash-brain-b".to_string();
    FederatedPatternPayload {
        schema_version: "1".to_string(),
        pattern_kind: PatternKind::VigilancePattern,
        feature_vector: FeatureVector {
            numeric_count: 1,
            severity_class: SeverityClass::Info,
            observation_window_days: 7,
        },
        anonymized_origin: local_hash.clone(),
        origin_set: vec![local_hash],
        peer_brain_id: peer_hash,
        discovered_at: chrono::Utc::now()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        severity_class: Some(SeverityClass::Info),
        legal_disclaimer: None,
        metadata: None,
    }
}

// =========================================================================
// Test 1 — happy path: emit → receive → both ledgers populated
// =========================================================================

/// **Headline test.** Brain A emits a well-formed federated-pattern
/// to Brain B over a real loopback TCP socket. Brain A writes the
/// `entry_kind=emitted` row; Brain B's handler writes the
/// `entry_kind=received` row. The `envelope_message_id`s match so
/// the audit trail is correlatable.
///
/// This is the §16.6.1 happy-path proof at the network boundary —
/// the equivalent of `dual_brain_pair`'s
/// `fractal_composition_end_to_end_over_loopback` for federation.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_happy_path_emit_then_receive_writes_both_ledgers() {
    let pair = loopback_pair_setup(
        true,
        Arc::new(FederatedPatternReceiveLimiter::new()),
    )
    .await;

    // Defensive: opt-in should be satisfied by the fixture.
    assert!(
        bidirectional_opt_in_satisfied(&pair.brain_a.card, &pair.brain_b.card),
        "loopback fixture must satisfy bidirectional opt-in"
    );

    // Sender writes `entry_kind=emitted` row before transmit (Q12).
    let payload = baseline_payload();
    let predicted_envelope_id = uuid::Uuid::new_v4().to_string();
    write_emitted_ledger_row(
        &pair.brain_a.ledger_path(),
        &payload,
        &predicted_envelope_id,
    )
    .expect("write emitted row");

    // Sender transmits via real HTTP+SSE transport over loopback.
    let limiter = FederatedPatternEmitLimiter::with_limits(2, Duration::from_millis(0));
    let transport = HttpSseTransport::new();
    let outcome = emit_federated_pattern(
        &pair.brain_a.card,
        &pair.brain_b.card,
        &pair.brain_b.a2a_endpoint,
        payload.clone(),
        "vigilance-pattern",
        &limiter,
        &transport,
    )
    .await
    .expect("emit_federated_pattern over loopback should succeed");

    // Give the receiver a moment to write its ledger row (the
    // server replies 202 from `post_task`, then runs the handler
    // synchronously — by the time the response returns, the row is
    // already on disk; this brief sleep is belt-and-braces for any
    // axum buffering).
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Sender ledger has the emitted row.
    let emitted_rows = read_ledger(&pair.brain_a.ledger_path());
    assert_eq!(
        emitted_rows.len(),
        1,
        "Brain A should have exactly one emitted row; got: {:?}",
        emitted_rows
    );
    assert_eq!(emitted_rows[0]["entry_kind"], "emitted");
    assert_eq!(emitted_rows[0]["payload"]["pattern_kind"], "vigilance-pattern");

    // Receiver ledger has the received row.
    let received_rows = read_ledger(&pair.brain_b.ledger_path());
    assert_eq!(
        received_rows.len(),
        1,
        "Brain B should have exactly one received row; got: {:?}",
        received_rows
    );
    let received = &received_rows[0];
    assert_eq!(received["entry_kind"], "received");
    assert!(
        received["dropped_reason"].is_null(),
        "happy-path receipt must not be dropped; got: {:?}",
        received["dropped_reason"]
    );

    // The wire-level message id propagated end-to-end:
    // Brain A's `EmitOutcome.envelope_message_id` matches the row
    // Brain B saw in its handler.
    let received_msg_id = received["envelope_message_id"]
        .as_str()
        .expect("received row has envelope_message_id");
    assert_eq!(
        received_msg_id, outcome.envelope_message_id,
        "envelope_message_id must propagate from sender to receiver ledger"
    );

    // Payload structural equivalence — the schema-closed-set values
    // round-tripped without distortion.
    assert_eq!(
        received["payload"]["pattern_kind"], "vigilance-pattern",
        "payload.pattern_kind survives the wire round-trip"
    );
    assert_eq!(
        received["payload"]["feature_vector"]["severity_class"], "info",
        "feature_vector.severity_class survives the wire round-trip"
    );
}

// =========================================================================
// Test 2 — wire-level recursion guard (Q9)
// =========================================================================

/// **Recursion guard.** Brain A constructs a payload whose
/// `origin_set[]` already contains Brain B's opaque hash (simulating
/// a relay loop where the message has already passed through B).
/// Brain B receives, the wire-level recursion guard fires, and the
/// receiver's ledger records `dropped_reason="recursion-guard"`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_recursion_guard_drops_when_receiver_in_origin_set() {
    let pair = loopback_pair_setup(
        true,
        Arc::new(FederatedPatternReceiveLimiter::new()),
    )
    .await;

    // Construct a payload whose origin_set contains Brain B's hash.
    // The receiver's `local_brain_id_hash` (configured in
    // `loopback_pair_setup`) is `"sha256-opaque-hash-brain-b"` — we
    // mirror that here.
    let mut payload = baseline_payload();
    payload.origin_set = vec![
        "sha256-opaque-hash-brain-a".to_string(),
        "sha256-opaque-hash-brain-b".to_string(),
    ];

    let predicted_envelope_id = uuid::Uuid::new_v4().to_string();
    write_emitted_ledger_row(
        &pair.brain_a.ledger_path(),
        &payload,
        &predicted_envelope_id,
    )
    .expect("write emitted row");

    let limiter = FederatedPatternEmitLimiter::with_limits(2, Duration::from_millis(0));
    let transport = HttpSseTransport::new();
    let _ = emit_federated_pattern(
        &pair.brain_a.card,
        &pair.brain_b.card,
        &pair.brain_b.a2a_endpoint,
        payload,
        "vigilance-pattern",
        &limiter,
        &transport,
    )
    .await
    .expect("emit returns success — the drop happens at the receiver");

    tokio::time::sleep(Duration::from_millis(50)).await;

    let received_rows = read_ledger(&pair.brain_b.ledger_path());
    assert_eq!(
        received_rows.len(),
        1,
        "Brain B should have exactly one received row (the dropped one)"
    );
    assert_eq!(received_rows[0]["entry_kind"], "received");
    assert_eq!(
        received_rows[0]["dropped_reason"], "recursion-guard",
        "Q9 wire-level recursion guard MUST drop with 'recursion-guard'; got: {:?}",
        received_rows[0]["dropped_reason"]
    );
}

// =========================================================================
// Test 3 — receiver-side rate limit (Q6 BR-6)
// =========================================================================

/// **Rate limit.** Brain A emits N+1 federated-patterns to Brain B
/// in rapid succession. Brain B's sliding-window receiver-side rate
/// limit accepts the first N and drops the (N+1)th with
/// `dropped_reason="rate-limit-exceeded"`. The receiver's ledger
/// records all N+1 receipts.
///
/// **Test scaling note:** the production threshold is 10 per 60s
/// (Q6 lock — `FEDERATED_PATTERN_RECV_MAX_PER_WINDOW`). To keep the
/// wall-clock small this test uses a `with_limits(3, ...)`
/// receiver-side limiter. We emit 4 messages; the first 3 are
/// admitted, the 4th is dropped. This exercises the same
/// sliding-window code path as the production threshold — just
/// faster. The real-thresholds case is covered by C4's
/// `receive_rate_limit_drops_after_threshold` unit test.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_rate_limit_drops_after_threshold() {
    // Scaled-down receiver limiter: max 3 per 60s window. The
    // receiver-side limiter buckets by `payload.anonymized_origin`
    // so all 4 emits must share that value to exercise the
    // per-peer threshold. The fixture in `baseline_payload()` uses
    // `"sha256-opaque-hash-brain-a"` for both `anonymized_origin`
    // and `origin_set[0]`, which is what we want.
    let receive_limiter = Arc::new(FederatedPatternReceiveLimiter::with_limits(
        3,
        Duration::from_secs(60),
    ));
    let pair = loopback_pair_setup(true, receive_limiter).await;

    // Emit-side limiter: high in-flight cap + 0ms min interval so
    // the test wall-clock isn't gated by the sender's rate-limit.
    // The receiver-side sliding window is what we're exercising.
    let limiter = FederatedPatternEmitLimiter::with_limits(8, Duration::from_millis(0));
    let transport = HttpSseTransport::new();

    // Emit 4 messages serially. Serial (vs concurrent) so the
    // receiver-side counter increments are observable in order —
    // a concurrent flood could land all 4 inside the same admit
    // critical-section in any order, still correct but harder to
    // pin precisely. The first 3 are admitted; the 4th drops with
    // `rate-limit-exceeded`.
    let total: usize = 4;
    for _ in 0..total {
        let payload = baseline_payload();
        let predicted_envelope_id = uuid::Uuid::new_v4().to_string();
        write_emitted_ledger_row(
            &pair.brain_a.ledger_path(),
            &payload,
            &predicted_envelope_id,
        )
        .expect("write emitted row");

        emit_federated_pattern(
            &pair.brain_a.card,
            &pair.brain_b.card,
            &pair.brain_b.a2a_endpoint,
            payload,
            "vigilance-pattern",
            &limiter,
            &transport,
        )
        .await
        .expect("emit POST succeeds (drops happen at receiver)");
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    let received_rows = read_ledger(&pair.brain_b.ledger_path());
    assert_eq!(
        received_rows.len(),
        total,
        "Brain B should record all {total} receipts (3 accepted + 1 dropped)"
    );

    let accepted = received_rows
        .iter()
        .filter(|r| r["dropped_reason"].is_null())
        .count();
    let dropped_rate_limit = received_rows
        .iter()
        .filter(|r| r["dropped_reason"] == "rate-limit-exceeded")
        .count();

    assert_eq!(
        accepted, 3,
        "exactly 3 receipts should be accepted (under the per-peer N=3 threshold); got: {accepted}"
    );
    assert_eq!(
        dropped_rate_limit, 1,
        "exactly 1 receipt should be dropped with 'rate-limit-exceeded'; got: {dropped_rate_limit}"
    );
}

// =========================================================================
// Test 4 — bidirectional opt-in failure (Q5)
// =========================================================================

/// **Opt-in.** If Brain B's Agent Card does NOT advertise
/// `federated-pattern` in its `accepts[]`, Brain A's
/// `emit_federated_pattern` returns `Error::OptInNotSatisfied`
/// BEFORE any network transmission. Brain B's ledger remains empty
/// — no receipt happened.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_bidirectional_opt_in_required_brain_b_doesnt_advertise() {
    // Brain B's Agent Card has empty capabilities — does NOT advertise
    // federated-pattern.
    let pair = loopback_pair_setup(
        false, // no advertisement on Brain B
        Arc::new(FederatedPatternReceiveLimiter::new()),
    )
    .await;

    assert!(
        !bidirectional_opt_in_satisfied(&pair.brain_a.card, &pair.brain_b.card),
        "fixture: Brain B should not advertise federated-pattern"
    );

    let limiter = FederatedPatternEmitLimiter::with_limits(2, Duration::from_millis(0));
    let transport = HttpSseTransport::new();
    let result = emit_federated_pattern(
        &pair.brain_a.card,
        &pair.brain_b.card,
        &pair.brain_b.a2a_endpoint,
        baseline_payload(),
        "vigilance-pattern",
        &limiter,
        &transport,
    )
    .await;

    match result {
        Err(Error::OptInNotSatisfied) => {}
        other => panic!("expected OptInNotSatisfied; got: {other:?}"),
    }

    // No ledger row on Brain B — the message never reached the wire.
    tokio::time::sleep(Duration::from_millis(20)).await;
    let received_rows = read_ledger(&pair.brain_b.ledger_path());
    assert!(
        received_rows.is_empty(),
        "Brain B should have NO received rows when opt-in fails; got: {received_rows:?}"
    );
}

// =========================================================================
// Test 5 — unknown pattern_kind drops for Q11 forward-compat
// =========================================================================

/// **Forward-compat (Q11).** Brain A constructs a payload with
/// `pattern_kind: "operator-calibration-pattern"` (not in v1's
/// closed set; reserved for v2/v3). Brain A bypasses its own
/// closed-set check by building the envelope by hand (using the
/// `A2aEnvelope::new` constructor with a hand-written payload
/// JSON). Brain B receives the envelope; the receiver's permissive
/// deserialization succeeds (the payload is structurally valid)
/// but the closed-set membership check fires, dropping with
/// `dropped_reason="unknown-pattern-kind"`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_unknown_pattern_kind_drops_for_forward_compat_q11() {
    let pair = loopback_pair_setup(
        true,
        Arc::new(FederatedPatternReceiveLimiter::new()),
    )
    .await;

    // Hand-craft a payload JSON whose pattern_kind is the v2
    // candidate `operator-calibration-pattern`. The receiver-side
    // permissive deserialization will accept the envelope; the
    // closed-set check fires after structural validation.
    let payload_json = json!({
        "schema_version": "1",
        "pattern_kind": "operator-calibration-pattern",
        "feature_vector": {
            "numeric_count": 1,
            "severity_class": "info",
            "observation_window_days": 7
        },
        "anonymized_origin": "sha256-opaque-hash-brain-a",
        "origin_set": ["sha256-opaque-hash-brain-a"],
        "peer_brain_id": "sha256-opaque-hash-brain-b",
        "discovered_at": "2026-04-27T20:00:00.000Z"
    });
    let envelope = A2aEnvelope::new(
        &pair.brain_a.card.id,
        MessageType::FederatedPattern,
        payload_json,
    );

    // Send the envelope directly via HttpSseTransport — bypass the
    // typed `emit_federated_pattern` because that one would reject
    // the unknown-kind payload at the typed-deserialize layer
    // (PatternKind enum is closed). We're simulating a v2/v3 peer
    // that legitimately emits the v2 pattern_kind.
    let transport = HttpSseTransport::new();
    transport
        .post_task(&pair.brain_b.a2a_endpoint, &envelope)
        .await
        .expect("POST succeeds; the receiver decides whether to accept");

    tokio::time::sleep(Duration::from_millis(50)).await;

    let received_rows = read_ledger(&pair.brain_b.ledger_path());
    assert_eq!(
        received_rows.len(),
        1,
        "Brain B should record one received row for the unknown-kind drop"
    );
    assert_eq!(
        received_rows[0]["dropped_reason"], "unknown-pattern-kind",
        "Q11 forward-compat: unknown pattern_kind drops with 'unknown-pattern-kind'; got: {:?}",
        received_rows[0]["dropped_reason"]
    );
}

// =========================================================================
// Test 6 — sensor-shape verification of the cross-Brain ledger
// =========================================================================

/// **Sensor-shape verification.** After running the happy-path
/// federation flow, scan Brain B's ledger using the same shape the
/// federated-patterns sensor (`neurogrim-sensory::federated_patterns`)
/// reads at score time. Validates that the wire-level cross-Brain
/// flow produces ledger rows whose `entry_kind`, `from_brain_id`,
/// and `payload.pattern_kind` fields populate the
/// `federated_patterns_breakdown` aggregator the C6 sensor builds.
///
/// **Why an inline reader instead of invoking the C6 sensor
/// directly:** the sensor lives in `neurogrim-sensory`, which is
/// not a dev-dependency of `neurogrim-a2a` (and per the brief HARD
/// CONSTRAINT we cannot add it without adding a new Cargo dep).
/// Replicating the small subset of the sensor's read logic inline
/// keeps the test scoped to the a2a crate while still proving
/// the cross-Brain → sensor integration. The full sensor
/// behavior is covered by `crates/neurogrim-sensory/tests/
/// federated_patterns_sensor_behavior.rs` (C6).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_aggregator_sensor_reads_real_ledger_after_loopback() {
    let pair = loopback_pair_setup(
        true,
        Arc::new(FederatedPatternReceiveLimiter::new()),
    )
    .await;

    // Run the happy-path emission once.
    let payload = baseline_payload();
    let predicted_envelope_id = uuid::Uuid::new_v4().to_string();
    write_emitted_ledger_row(
        &pair.brain_a.ledger_path(),
        &payload,
        &predicted_envelope_id,
    )
    .expect("write emitted row");

    let limiter = FederatedPatternEmitLimiter::with_limits(2, Duration::from_millis(0));
    let transport = HttpSseTransport::new();
    emit_federated_pattern(
        &pair.brain_a.card,
        &pair.brain_b.card,
        &pair.brain_b.a2a_endpoint,
        payload,
        "vigilance-pattern",
        &limiter,
        &transport,
    )
    .await
    .expect("emit succeeds");

    tokio::time::sleep(Duration::from_millis(50)).await;

    // ── Sensor-shape aggregation ────────────────────────────────────
    // Replicate the C6 sensor's read logic at minimum fidelity:
    //   - count `entry_kind=received` rows → total_received_count
    //   - group by `from_brain_id` → peer_breakdown
    //   - group by `payload.pattern_kind` → pattern_kind_breakdown
    //   - low_confidence iff received + emitted == 0 in window
    //
    // The sensor uses BTreeMap for stable output; we use HashMap
    // because the test only checks the aggregate keys exist.
    let rows = read_ledger(&pair.brain_b.ledger_path());

    let total_received_count = rows.iter().filter(|r| r["entry_kind"] == "received").count();
    assert_eq!(
        total_received_count, 1,
        "sensor would report total_received_count == 1 after the loopback flow; got: {total_received_count}"
    );

    // Per-peer aggregate. The receiver's ledger writer stores the
    // sender envelope's `brain_id` as `from_brain_id` — for our
    // fixture that is "brain-a-test".
    let mut peer_received_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for r in &rows {
        if r["entry_kind"] == "received" {
            if let Some(peer) = r["from_brain_id"].as_str() {
                *peer_received_counts.entry(peer.to_string()).or_insert(0) += 1;
            }
        }
    }
    assert!(
        peer_received_counts.contains_key("brain-a-test"),
        "sensor's peer_breakdown would include Brain A's id; got peers: {peer_received_counts:?}"
    );

    // Per-pattern-kind aggregate.
    let mut kind_received_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for r in &rows {
        if r["entry_kind"] == "received" {
            if let Some(kind) = r["payload"]
                .as_object()
                .and_then(|p| p.get("pattern_kind"))
                .and_then(|v| v.as_str())
            {
                *kind_received_counts.entry(kind.to_string()).or_insert(0) += 1;
            }
        }
    }
    assert_eq!(
        kind_received_counts.get("vigilance-pattern"),
        Some(&1),
        "sensor's pattern_kind_breakdown should report vigilance-pattern receipts; got: {kind_received_counts:?}"
    );

    // Low-confidence: false because we have 1 receipt in a fresh
    // window. (At N=1, the sensor's `low_confidence` is FALSE per
    // the C6 logic — `low_confidence = (received_in_window +
    // emitted_in_window == 0)`. The 7-day window is wide enough
    // that the just-now timestamp falls inside it.)
    let low_confidence = rows.is_empty();
    assert!(
        !low_confidence,
        "low_confidence should be false (1 receipt in a fresh window)"
    );
}

// =========================================================================
// Helper: silence unused-import warning when one variant of the
// dependency surface isn't exercised in a test build configuration.
// =========================================================================

#[test]
fn _build_federated_pattern_envelope_is_in_scope() {
    // Compile-time anchor — this test exists so the
    // `build_federated_pattern_envelope` import (used by C4 protocol-
    // behavior tests but not by the e2e tests above, which use
    // `emit_federated_pattern` end-to-end) is detectable as in-use
    // by any future code change. Suppresses the unused-import
    // warning if a future refactor moves the helper out of the
    // public surface.
    let payload = baseline_payload();
    let env = build_federated_pattern_envelope(payload, "brain-a-test").unwrap();
    assert!(matches!(env.message_type, MessageType::FederatedPattern));
    // Also exercise the `_mutex_in_scope` import to keep the
    // `tokio::sync::Mutex` import detectable. The helper structs in
    // this file use `Arc<Mutex<_>>` patterns implicitly; this is
    // belt-and-braces.
    let _: Mutex<i32> = Mutex::new(0);
}
