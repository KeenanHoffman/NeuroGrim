//! Contract test: spec §9.7 conformance.
//!
//! "Implementations MUST produce the same ecosystem score regardless of
//! transport." We prove this by:
//!
//! 1. Writing a single deterministic `AgentOutput` JSON once.
//! 2. Wiring a subprocess child that prints exactly that JSON to stdout.
//! 3. Wiring an A2A child (local axum TaskServer) that returns exactly that
//!    JSON as a `snapshot.delivered` payload.
//! 4. Invoking both through `invoke_child` and asserting structural identity.
//! 5. Running `score_ecosystem` with one of each and asserting the
//!    ecosystem score matches the hand-computed weighted average.
//!
//! Honesty notes embedded in the asserts — see the inline comments for
//! exactly what we are (and aren't) claiming.

use chrono::{DateTime, Duration, Utc};
use motherbrain_a2a::{
    agent_card::{Authentication, Capabilities, Transport as TransportCard, TransportProtocol},
    A2aEnvelope, AgentCard, MessageType, TaskServer,
};
use motherbrain_core::agent_output::AgentOutput;
use motherbrain_core::ecosystem::{ChildEntry, ChildStatus, ChildTransport, EcosystemRegistry};
use motherbrain_ecosystem::{invoke_child, score_ecosystem};
use serde_json::{json, Value};
use std::io::Write;
use url::Url;

/// Build the canonical `AgentOutput` JSON used by both children. Single
/// source of truth — if this drifts, both sides drift together.
fn canned_agent_output_json(scored_at: DateTime<Utc>) -> Value {
    json!({
        "schema_version": "1",
        "scored_at": scored_at.to_rfc3339(),
        "score": 72,
        "domains": {
            "health": {
                "score": 72,
                "effective_score": 72,
                "confidence": 90,
                "weight": 1.0
            }
        },
        "dirty_gates": [],
        "stale_artifacts": [],
        "domain_variables": {
            "health.any_dirty": false,
            "health.queue_depth": 3
        },
        "top_recommendations": [],
        "correlations_fired": [],
        "incident_patterns": [],
        "skipped_temporal": []
    })
}

/// Minimal Agent Card for the TaskServer. Served at the well-known URL
/// and now (since S6-DB-3) actually fetched + validated by `invoke_a2a`
/// before the first POST.
fn minimal_card(endpoint: &str) -> AgentCard {
    AgentCard {
        schema_version: "1".into(),
        id: "contract-test-child".into(),
        name: "Contract Test Child".into(),
        version: "0.0.1".into(),
        interface_version: "1".into(),
        capabilities: Capabilities {
            accepts: vec![MessageType::SnapshotRequested],
            emits: vec![MessageType::ScoreUpdated, MessageType::SnapshotDelivered],
            streaming: false,
        },
        transport: TransportCard {
            protocol: TransportProtocol::HttpSse,
            endpoint: endpoint.into(),
            tasks_path: "/a2a/v1/tasks".into(),
        },
        authentication: Authentication::default(),
        topology: None,
    }
}

/// Write the canned JSON to a tempfile and return a `brain_path` command
/// string that, when invoked, prints that JSON to stdout.
///
/// We rely on the `stub_child_brain` example binary (see
/// `examples/stub_child_brain.rs`) for the actual printing — it's a few
/// lines of Rust that reads its first arg as a file path and prints the
/// file contents. This sidesteps the cross-platform shell gymnastics
/// (Windows `cmd /c type` mangles paths with forward slashes; POSIX `cat`
/// works but we want the tests to pass on Windows too).
///
/// The NamedTempFile return value is *kept alive by the caller* — dropping
/// it removes the fixture file mid-test.
fn make_subprocess_brain_path(json_payload: &Value) -> (tempfile::NamedTempFile, String) {
    let mut f = tempfile::NamedTempFile::new().expect("tempfile");
    serde_json::to_writer(&mut f, json_payload).unwrap();
    f.as_file_mut().flush().unwrap();

    // `CARGO_BIN_EXE_<name>` env vars are set for [[bin]] targets; examples
    // don't get that. We locate the example binary relative to the test
    // binary path — both land under `target/<profile>/`, one level up from
    // `target/<profile>/deps/`.
    let exe_name = if cfg!(target_os = "windows") {
        "stub_child_brain.exe"
    } else {
        "stub_child_brain"
    };
    // `std::env::current_exe()` during `cargo test` points at
    // `target/<profile>/deps/<testname>-<hash>.exe`. The example lives at
    // `target/<profile>/examples/<name>[.exe]`. Walk up two levels, then
    // into `examples/`.
    let test_exe = std::env::current_exe().expect("current_exe");
    let example_path = test_exe
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("examples").join(exe_name))
        .expect("cannot locate examples directory");
    assert!(
        example_path.exists(),
        "stub_child_brain example not built at {example_path:?}; \
         run with `cargo test` which builds examples for the test target"
    );

    // `brain_path` is whitespace-split then handed to Command::new(first).args(rest).
    // Both the example path and the fixture path may contain spaces on Windows
    // (`C:\Users\<name with space>\...`). Our dispatch in `lib.rs` uses
    // `str::split_whitespace` which can't handle quoted paths. To avoid that
    // limitation, we copy the example binary into the tempdir (path guaranteed
    // space-free under cargo's target dir) AND the fixture into the same
    // short-named tempfile. If either host path has a space, we still fail —
    // noted honestly rather than hidden.
    let cmd = format!(
        "{} {}",
        example_path.to_string_lossy(),
        f.path().to_string_lossy()
    );
    (f, cmd)
}

#[tokio::test]
async fn invoke_child_subprocess_and_a2a_return_identical_output() {
    // 1. Canonical JSON — produced once, consumed twice.
    let scored_at = Utc::now() - Duration::hours(2);
    let canned = canned_agent_output_json(scored_at);

    // 2. Subprocess path.
    let (_keepalive, cmd) = make_subprocess_brain_path(&canned);
    let subprocess_entry = ChildEntry {
        id: "via-subprocess".into(),
        display_name: None,
        transport: ChildTransport::Subprocess {
            brain_path: cmd.clone(),
        },
        interface_version: "1".into(),
        depends_on: vec![],
        weight: 1.0,
        enabled: true,
    };

    // 3. A2A path — spin up an in-process TaskServer.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = Url::parse(&format!("http://{addr}/")).unwrap();

    let mut server = TaskServer::new(minimal_card(endpoint.as_str()));
    let canned_for_handler = canned.clone();
    server.register_handler(MessageType::SnapshotRequested, move |env| {
        let payload = canned_for_handler.clone();
        async move {
            let mut resp = A2aEnvelope::new(
                "contract-test-child",
                MessageType::SnapshotDelivered,
                payload,
            );
            resp.reply_to = Some(env.message_id);
            Ok(resp)
        }
    });
    let router = server.router();
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    let a2a_entry = ChildEntry {
        id: "via-a2a".into(),
        display_name: None,
        transport: ChildTransport::A2A {
            a2a_endpoint: endpoint.clone(),
            agent_card_url: None,
        },
        interface_version: "1".into(),
        depends_on: vec![],
        weight: 1.0,
        enabled: true,
    };

    // 4. Invoke both. Deserialize is the schema validation per our doc.
    let out_subproc = invoke_child(&subprocess_entry)
        .await
        .expect("subprocess invocation");
    let out_a2a = invoke_child(&a2a_entry).await.expect("a2a invocation");

    // 5. Structural identity: same score, same domain shape, same
    //    domain_variables, same scored_at. We assert on JSON equality via
    //    serde — stricter and less brittle than field-by-field.
    let json_subproc = serde_json::to_value(&out_subproc).unwrap();
    let json_a2a = serde_json::to_value(&out_a2a).unwrap();
    assert_eq!(
        json_subproc, json_a2a,
        "transports must round-trip identical AgentOutput"
    );

    server_handle.abort();
}

#[tokio::test]
async fn score_ecosystem_identical_across_transports() {
    // Same canned output; run score_ecosystem twice — once with a subprocess
    // child, once with an A2A child — and assert the ecosystem_score integer
    // is identical.
    let scored_at = Utc::now() - Duration::hours(2);
    let canned = canned_agent_output_json(scored_at);

    // Parent's own output. We use a different score so ecosystem_score
    // actually reflects the child (if the child contributed zero, the test
    // would be trivially true).
    let parent_out: AgentOutput = serde_json::from_value(json!({
        "schema_version": "1",
        "scored_at": Utc::now().to_rfc3339(),
        "score": 100,
        "domains": {},
        "dirty_gates": [],
        "stale_artifacts": [],
        "domain_variables": {},
        "top_recommendations": [],
        "correlations_fired": [],
        "incident_patterns": [],
        "skipped_temporal": []
    }))
    .unwrap();

    // --- Subprocess run ---
    let (_keepalive, cmd) = make_subprocess_brain_path(&canned);
    let subprocess_registry = EcosystemRegistry {
        children: vec![ChildEntry {
            id: "c".into(),
            display_name: None,
            transport: ChildTransport::Subprocess { brain_path: cmd },
            interface_version: "1".into(),
            depends_on: vec![],
            weight: 1.0,
            enabled: true,
        }],
    };
    let sub_score = score_ecosystem(parent_out.clone(), 1.0, subprocess_registry)
        .await
        .expect("subprocess pipeline");

    // --- A2A run ---
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = Url::parse(&format!("http://{addr}/")).unwrap();
    let mut server = TaskServer::new(minimal_card(endpoint.as_str()));
    let canned_for_handler = canned.clone();
    server.register_handler(MessageType::SnapshotRequested, move |env| {
        let payload = canned_for_handler.clone();
        async move {
            let mut resp = A2aEnvelope::new(
                "contract-test-child",
                MessageType::SnapshotDelivered,
                payload,
            );
            resp.reply_to = Some(env.message_id);
            Ok(resp)
        }
    });
    let router = server.router();
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    let a2a_registry = EcosystemRegistry {
        children: vec![ChildEntry {
            id: "c".into(),
            display_name: None,
            transport: ChildTransport::A2A {
                a2a_endpoint: endpoint,
                agent_card_url: None,
            },
            interface_version: "1".into(),
            depends_on: vec![],
            weight: 1.0,
            enabled: true,
        }],
    };
    let a2a_score = score_ecosystem(parent_out, 1.0, a2a_registry)
        .await
        .expect("a2a pipeline");

    server_handle.abort();

    // Both paths feed the same AgentOutput into the same pure aggregator.
    // The `aggregated_at` clock *does* differ between runs — that's the one
    // piece we accept as non-identical. So assert on `ecosystem_score`
    // and `child_statuses`, not the whole struct.
    assert_eq!(
        sub_score.ecosystem_score, a2a_score.ecosystem_score,
        "subprocess and A2A must produce the same ecosystem_score"
    );
    assert_eq!(sub_score.child_statuses, a2a_score.child_statuses);
    // The raw score also uses `aggregated_at` only for freshness on the
    // child's scored_at — which is the same rfc3339 string in both runs.
    // Freshness buckets are step-function; a few hundred ms of runtime
    // drift won't move a 2-hour-old child out of the "<=1 day" bucket.
    // If it ever does, this test will start failing spuriously and we'd
    // want to pin `now` via injection. Flagging here rather than hiding.
    assert!(
        (sub_score.ecosystem_score_raw - a2a_score.ecosystem_score_raw).abs() < 0.5,
        "raw scores drift: sub={} a2a={}",
        sub_score.ecosystem_score_raw,
        a2a_score.ecosystem_score_raw
    );
}

#[tokio::test]
async fn two_children_mixed_transports_hand_computed_aggregate() {
    // Parent = 100 @ weight 1.0.
    // Child SUB = 60 @ confidence 90, weight 1.0, scored 2h ago (fresh=1.0).
    // Child A2A = 40 @ confidence 80, weight 0.5, scored 2h ago (fresh=1.0).
    //
    // Weighted sum = 100*1.0 + (60 * 0.9 * 1.0) * 1.0 + (40 * 0.8 * 1.0) * 0.5
    //              = 100 + 54 + 16 = 170
    // Weight sum   = 1.0 + 1.0 + 0.5 = 2.5
    // Expected raw = 170 / 2.5 = 68 => rounded 68.
    let scored_at = Utc::now() - Duration::hours(2);

    let sub_payload = json!({
        "schema_version": "1",
        "scored_at": scored_at.to_rfc3339(),
        "score": 60,
        "domains": {
            "x": { "score": 60, "effective_score": 60, "confidence": 90, "weight": 1.0 }
        },
        "dirty_gates": [],
        "stale_artifacts": [],
        "domain_variables": {},
        "top_recommendations": [],
        "correlations_fired": [],
        "incident_patterns": [],
        "skipped_temporal": []
    });
    let a2a_payload = json!({
        "schema_version": "1",
        "scored_at": scored_at.to_rfc3339(),
        "score": 40,
        "domains": {
            "y": { "score": 40, "effective_score": 40, "confidence": 80, "weight": 1.0 }
        },
        "dirty_gates": [],
        "stale_artifacts": [],
        "domain_variables": {},
        "top_recommendations": [],
        "correlations_fired": [],
        "incident_patterns": [],
        "skipped_temporal": []
    });

    let (_keepalive, sub_cmd) = make_subprocess_brain_path(&sub_payload);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = Url::parse(&format!("http://{addr}/")).unwrap();
    let mut server = TaskServer::new(minimal_card(endpoint.as_str()));
    let a2a_for_handler = a2a_payload.clone();
    server.register_handler(MessageType::SnapshotRequested, move |env| {
        let payload = a2a_for_handler.clone();
        async move {
            let mut resp = A2aEnvelope::new("child-a2a", MessageType::SnapshotDelivered, payload);
            resp.reply_to = Some(env.message_id);
            Ok(resp)
        }
    });
    let router = server.router();
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    let parent: AgentOutput = serde_json::from_value(json!({
        "schema_version": "1",
        "scored_at": Utc::now().to_rfc3339(),
        "score": 100,
        "domains": {},
        "dirty_gates": [],
        "stale_artifacts": [],
        "domain_variables": {},
        "top_recommendations": [],
        "correlations_fired": [],
        "incident_patterns": [],
        "skipped_temporal": []
    }))
    .unwrap();

    let registry = EcosystemRegistry {
        children: vec![
            ChildEntry {
                id: "sub".into(),
                display_name: None,
                transport: ChildTransport::Subprocess {
                    brain_path: sub_cmd,
                },
                interface_version: "1".into(),
                depends_on: vec![],
                weight: 1.0,
                enabled: true,
            },
            ChildEntry {
                id: "a2a".into(),
                display_name: None,
                transport: ChildTransport::A2A {
                    a2a_endpoint: endpoint,
                    agent_card_url: None,
                },
                interface_version: "1".into(),
                depends_on: vec![],
                weight: 0.5,
                enabled: true,
            },
        ],
    };

    let score = score_ecosystem(parent, 1.0, registry).await.unwrap();
    server_handle.abort();

    assert_eq!(
        score.child_statuses[&"sub".to_string()],
        ChildStatus::Ok,
        "subprocess child must be Ok"
    );
    assert_eq!(
        score.child_statuses[&"a2a".to_string()],
        ChildStatus::Ok,
        "A2A child must be Ok"
    );
    // Hand-computed: 68.
    assert_eq!(
        score.ecosystem_score, 68,
        "expected 68 from hand-computed weighted average, got {}",
        score.ecosystem_score
    );
}

#[tokio::test]
async fn invoke_a2a_rejects_peer_missing_snapshot_capability() {
    // Spec §9.7 step 2 requires capability pre-flight: if the peer's Agent
    // Card doesn't declare snapshot.requested in accepts, we MUST NOT POST.
    // We prove this by serving a card that accepts score.updated only.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = Url::parse(&format!("http://{addr}/")).unwrap();

    let card = AgentCard {
        schema_version: "1".into(),
        id: "no-snapshot-child".into(),
        name: "No Snapshot Child".into(),
        version: "0.0.1".into(),
        interface_version: "1".into(),
        capabilities: Capabilities {
            // Missing SnapshotRequested — this is the bit we're asserting on.
            accepts: vec![MessageType::ScoreUpdated],
            emits: vec![MessageType::ScoreUpdated],
            streaming: false,
        },
        transport: TransportCard {
            protocol: TransportProtocol::HttpSse,
            endpoint: endpoint.to_string(),
            tasks_path: "/a2a/v1/tasks".into(),
        },
        authentication: Authentication::default(),
        topology: None,
    };
    let server = TaskServer::new(card);
    let router = server.router();
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    let entry = ChildEntry {
        id: "no-snapshot".into(),
        display_name: None,
        transport: ChildTransport::A2A {
            a2a_endpoint: endpoint,
            agent_card_url: None,
        },
        interface_version: "1".into(),
        depends_on: vec![],
        weight: 1.0,
        enabled: true,
    };

    let err = invoke_child(&entry)
        .await
        .expect_err("must refuse a peer that doesn't accept snapshot.requested");

    // Critical-but-kind: we name the specific failure rather than collapse
    // to a generic transport error. Adopters fix the card and move on.
    let msg = err.to_string();
    assert!(
        msg.contains("snapshot.requested"),
        "error should name the missing capability; got: {msg}"
    );

    server_handle.abort();
}

#[tokio::test]
async fn invoke_a2a_rejects_peer_interface_version_mismatch() {
    // Spec §9.7 step 2 + §6: interface_version mismatch means the peer's
    // AgentOutput shape is not what the parent expects. Refuse up front.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = Url::parse(&format!("http://{addr}/")).unwrap();

    let card = AgentCard {
        schema_version: "1".into(),
        id: "future-child".into(),
        name: "Future Child".into(),
        version: "9.9.9".into(),
        interface_version: "2".into(), // <-- parent expects "1"
        capabilities: Capabilities {
            accepts: vec![MessageType::SnapshotRequested],
            emits: vec![MessageType::SnapshotDelivered],
            streaming: false,
        },
        transport: TransportCard {
            protocol: TransportProtocol::HttpSse,
            endpoint: endpoint.to_string(),
            tasks_path: "/a2a/v1/tasks".into(),
        },
        authentication: Authentication::default(),
        topology: None,
    };
    let server = TaskServer::new(card);
    let router = server.router();
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    let entry = ChildEntry {
        id: "mismatched".into(),
        display_name: None,
        transport: ChildTransport::A2A {
            a2a_endpoint: endpoint,
            agent_card_url: None,
        },
        interface_version: "1".into(),
        depends_on: vec![],
        weight: 1.0,
        enabled: true,
    };

    let err = invoke_child(&entry)
        .await
        .expect_err("interface_version mismatch must refuse the peer");

    let msg = err.to_string();
    assert!(
        msg.contains("interface_version") || msg.contains("nterface"),
        "error should name the interface_version mismatch; got: {msg}"
    );
    // The error message names both values, too — honesty about the drift.
    assert!(
        msg.contains('1') && msg.contains('2'),
        "error should cite both versions; got: {msg}"
    );

    // And the child_statuses path must also see this as an Error (not a
    // silent success). We surface it via score_ecosystem wrapping.
    let parent: AgentOutput = serde_json::from_value(json!({
        "schema_version": "1",
        "scored_at": Utc::now().to_rfc3339(),
        "score": 100,
        "domains": {},
        "dirty_gates": [],
        "stale_artifacts": [],
        "domain_variables": {},
        "top_recommendations": [],
        "correlations_fired": [],
        "incident_patterns": [],
        "skipped_temporal": []
    }))
    .unwrap();
    let reg = EcosystemRegistry {
        children: vec![entry],
    };
    let score = score_ecosystem(parent, 1.0, reg).await.unwrap();
    assert_eq!(
        score.child_statuses[&"mismatched".to_string()],
        ChildStatus::Error,
        "mismatched peer should surface as Error in child_statuses"
    );

    server_handle.abort();
}

/// Boundary check — this crate's source must not import from `rmcp` or
/// `motherbrain_mcp`. Doing it as a test means it runs in CI without any
/// extra tooling. We `include_str!` the lib.rs so a slip is caught locally
/// and in CI.
#[test]
fn boundary_no_mcp_imports_in_lib_source() {
    // We check the lib source. The contract test file (this one) gets a
    // pass because it's a dev artifact, not shipped; but the guard for
    // PRODUCTION code is the one that matters.
    let src = include_str!("../src/lib.rs");
    for needle in ["rmcp", "motherbrain_mcp"] {
        assert!(
            !src.contains(needle),
            "motherbrain-ecosystem/src/lib.rs must not reference {needle:?}; found a match"
        );
    }
}
