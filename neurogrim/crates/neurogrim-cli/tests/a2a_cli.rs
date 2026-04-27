//! End-to-end integration test for the `a2a-*` CLI subcommands.
//!
//! We don't shell out to the `neurogrim` binary here (doing so on Windows
//! doubles build times and pulls in path-quoting pain). Instead we exercise
//! the exact code the CLI entry points call: `TaskServer` served on a random
//! loopback port, then a `TaskClient` round-trip against the Agent Card and
//! the `snapshot.requested` handler.
//!
//! This is the CLI wiring's proof-of-life: if this passes, the CLI commands
//! can be trusted to behave the same way against a live peer — the binary
//! wrappers are thin.

use neurogrim_a2a::agent_card::{
    Authentication, Capabilities, Transport as TransportCard, TransportProtocol,
};
use neurogrim_a2a::envelope::MessageType;
use neurogrim_a2a::{A2aEnvelope, AgentCard, TaskClient, TaskServer};
use serde_json::json;
use url::Url;

fn card(endpoint: &str) -> AgentCard {
    AgentCard {
        schema_version: "1".into(),
        id: "cli-test-brain".into(),
        name: "CLI Test Brain".into(),
        version: "0.0.1".into(),
        interface_version: "1".into(),
        capabilities: Capabilities {
            accepts: vec![MessageType::SnapshotRequested, MessageType::ScoreUpdated],
            emits: vec![MessageType::SnapshotDelivered],
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

#[tokio::test]
async fn cli_client_path_discovers_and_invokes_local_server() {
    // ---- Serve a peer on a random port ----
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("http://{addr}/");

    let mut server = TaskServer::new(card(&endpoint));
    server.register_handler(MessageType::SnapshotRequested, |req| async move {
        // Shape echoes the placeholder output from `a2a_serve.rs`.
        let payload = json!({
            "schema_version": "1",
            "brain_id": "cli-test-brain",
            "scored_at": "2026-04-17T00:00:00Z",
            "score": 0,
            "domains": {},
            "dirty_gates": [],
            "stale_artifacts": [],
            "domain_variables": {},
            "top_recommendations": [],
            "correlations_fired": [],
            "incident_patterns": [],
            "skipped_temporal": []
        });
        let mut resp = A2aEnvelope::new("cli-test-brain", MessageType::SnapshotDelivered, payload);
        resp.reply_to = Some(req.message_id);
        Ok(resp)
    });

    let router = server.router();
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    // ---- Discovery path (what `a2a-discover` calls) ----
    let url = Url::parse(&endpoint).unwrap();
    let client = TaskClient::new_http();
    let card = client
        .discover(&url)
        .await
        .expect("discover must succeed against our own server");
    assert_eq!(card.id, "cli-test-brain");
    assert!(card
        .capabilities
        .accepts
        .contains(&MessageType::SnapshotRequested));

    // ---- Invoke path (what `a2a-invoke` calls) ----
    let envelope = A2aEnvelope::new("neurogrim-cli", MessageType::SnapshotRequested, json!({}));
    let original_id = envelope.message_id.clone();
    let reply = client
        .invoke(&url, envelope)
        .await
        .expect("invoke must succeed");
    assert_eq!(reply.message_type, MessageType::SnapshotDelivered);
    assert_eq!(reply.reply_to, Some(original_id));
    // Payload is the AgentOutput shape — proves the CLI would get something
    // serde_json can pretty-print.
    assert_eq!(reply.payload["schema_version"], "1");
    assert_eq!(reply.payload["score"], 0);

    handle.abort();
}

#[tokio::test]
async fn cli_discovery_rejects_unreachable_peer() {
    // Fail loudly and fast when there's no one home — the CLI prints this
    // verbatim to the user, so the error must read sensibly.
    let url = Url::parse("http://127.0.0.1:1/").unwrap(); // reserved, no server here
    let client = TaskClient::new_http();
    let err = client.discover(&url).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unreachable") || msg.contains("agent card") || msg.contains("connection"),
        "error should name the discovery failure; got: {msg}"
    );
}

#[tokio::test]
async fn supply_chain_signal_e2e_over_loopback() {
    // 2026-04-26 PRE-RELEASE Round 2 R2-5 (D2-G3) regression guard.
    //
    // E-SC-7 + E-SC-10 shipped MessageType::SupplyChainSignal +
    // SupplyChainSignalPayload + default_handle_received without a
    // full HTTP-loopback E2E test. Unit tests in
    // `crates/neurogrim-a2a/src/supply_chain_signal.rs` cover the
    // helpers individually; this test exercises the full sender →
    // network → receiver → received-signals.jsonl flow.
    //
    // Validates three contracts simultaneously:
    //   1. bidirectional_opt_in_satisfied returns true when both
    //      peers' Agent Cards declare `supply-chain-signal` correctly.
    //   2. Sender can serialize a SupplyChainSignalPayload + wrap
    //      it in an A2aEnvelope + send via TaskClient.
    //   3. Receiver runs default_handle_received, persists the
    //      signal to the log file, and returns an ack envelope
    //      whose payload references the original message id.
    use neurogrim_a2a::supply_chain_signal::{
        bidirectional_opt_in_satisfied, default_handle_received, DiscoverySource, PackageRef,
        SeverityClass, SupplyChainSignalPayload,
    };
    use std::path::PathBuf;
    use std::sync::Arc;

    // ---- Set up a temp log file unique to this test run ----
    let log_path: PathBuf = std::env::temp_dir().join(format!(
        "neurogrim-r2-r5-supply-chain-signal-log-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&log_path); // start clean

    // ---- Build a receiver card that ACCEPTS supply-chain-signal ----
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("http://{addr}/");

    let mut receiver_card = card(&endpoint);
    receiver_card.id = "receiver-brain".into();
    receiver_card
        .capabilities
        .accepts
        .push(MessageType::SupplyChainSignal);

    // ---- Build a sender card that EMITS supply-chain-signal ----
    let mut sender_card = card("http://localhost:0/");
    sender_card.id = "sender-brain".into();
    sender_card
        .capabilities
        .emits
        .push(MessageType::SupplyChainSignal);

    // Contract 1: bidirectional opt-in satisfied.
    assert!(
        bidirectional_opt_in_satisfied(&sender_card, &receiver_card),
        "sender emits + receiver accepts → opt-in must be satisfied"
    );
    // Negative control: swap roles → receiver-as-sender does NOT
    // emit, so opt-in fails.
    assert!(
        !bidirectional_opt_in_satisfied(&receiver_card, &sender_card),
        "receiver doesn't emit → opt-in must NOT be satisfied"
    );

    // ---- Wire up the receiver server with default_handle_received ----
    let log_path_for_handler = Arc::new(log_path.clone());
    let mut server = TaskServer::new(receiver_card.clone());
    server.register_handler(MessageType::SupplyChainSignal, move |req| {
        let log_path = Arc::clone(&log_path_for_handler);
        async move {
            default_handle_received(&req, &log_path, "receiver-brain")
                .map_err(neurogrim_a2a::error::A2aError::InvalidEnvelope)
        }
    });

    let router = server.router();
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    // ---- Sender constructs a signal + sends over HTTP loopback ----
    let payload = SupplyChainSignalPayload {
        schema_version: "1".to_string(),
        advisory_id: Some("RUSTSEC-NG-TEST-R2".to_string()),
        package: PackageRef {
            name: "fakepkg".to_string(),
            ecosystem: "crates.io".to_string(),
            version: "0.1.0".to_string(),
            version_range: None,
        },
        severity_class: SeverityClass::Medium,
        discovery_source: DiscoverySource::Vigilance,
        peer_brain_id: "sender-brain".to_string(),
        cross_brain_count: None,
        discovered_at: None,
        advisory_uri: None,
        summary: Some("R2-5 E2E test signal".to_string()),
        legal_disclaimer: Some(
            "This signal is shared under non-attributive language discipline per spec section 16.4."
                .to_string(),
        ),
        recommended_action: None,
        metadata: serde_json::Map::new(),
    };
    let payload_value = serde_json::to_value(&payload).unwrap();
    let envelope = A2aEnvelope::new("sender-brain", MessageType::SupplyChainSignal, payload_value);
    let original_id = envelope.message_id.clone();

    let url = Url::parse(&endpoint).unwrap();
    let client = TaskClient::new_http();
    let reply = client
        .invoke(&url, envelope)
        .await
        .expect("invoke must succeed");

    // Contract 2 + 3: sender side worked, receiver replied with ack.
    // The default handler returns a ScoreUpdated-shaped ack envelope
    // whose payload references the original message_id.
    assert_eq!(
        reply.message_type,
        MessageType::ScoreUpdated,
        "default_handle_received returns a ScoreUpdated-shaped ack"
    );
    assert_eq!(
        reply.payload["ack"].as_str(),
        Some("supply-chain-signal-received"),
        "ack payload must declare receipt"
    );
    assert_eq!(
        reply.payload["envelope_message_id"].as_str(),
        Some(original_id.as_str()),
        "ack must reference the original message_id"
    );

    // Contract 3 (cont.): receiver appended to received-signals.jsonl.
    let log_contents = std::fs::read_to_string(&log_path)
        .expect("default_handle_received should have written the log file");
    let lines: Vec<&str> = log_contents.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1, "exactly one signal expected in log");

    let parsed: serde_json::Value =
        serde_json::from_str(lines[0]).expect("logged signal must be valid JSON");
    assert_eq!(parsed["from_brain_id"].as_str(), Some("sender-brain"));
    assert_eq!(parsed["envelope_message_id"].as_str(), Some(original_id.as_str()));
    assert_eq!(parsed["payload"]["advisory_id"].as_str(), Some("RUSTSEC-NG-TEST-R2"));
    assert_eq!(parsed["payload"]["package"]["name"].as_str(), Some("fakepkg"));

    // ---- Cleanup ----
    let _ = std::fs::remove_file(&log_path);
    handle.abort();
}
