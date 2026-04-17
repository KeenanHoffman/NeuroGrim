//! End-to-end integration test for the `a2a-*` CLI subcommands.
//!
//! We don't shell out to the `motherbrain` binary here (doing so on Windows
//! doubles build times and pulls in path-quoting pain). Instead we exercise
//! the exact code the CLI entry points call: `TaskServer` served on a random
//! loopback port, then a `TaskClient` round-trip against the Agent Card and
//! the `snapshot.requested` handler.
//!
//! This is the CLI wiring's proof-of-life: if this passes, the CLI commands
//! can be trusted to behave the same way against a live peer — the binary
//! wrappers are thin.

use motherbrain_a2a::agent_card::{
    Authentication, Capabilities, Transport as TransportCard, TransportProtocol,
};
use motherbrain_a2a::envelope::MessageType;
use motherbrain_a2a::{A2aEnvelope, AgentCard, TaskClient, TaskServer};
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
    let envelope = A2aEnvelope::new("motherbrain-cli", MessageType::SnapshotRequested, json!({}));
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
