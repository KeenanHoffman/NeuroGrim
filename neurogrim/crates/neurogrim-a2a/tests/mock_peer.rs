//! End-to-end integration test: TaskClient <-> TaskServer via loopback.
//!
//! Spins up a minimal peer Brain that responds to `snapshot.requested` with
//! `snapshot.delivered`, then drives it with a real `TaskClient` over a
//! loopback TCP socket. This is the closest we can get to proving the crate
//! works end-to-end without a second process.
//!
//! Honesty: this test exercises the happy path only. Failure modes (timeouts,
//! 5xx from the peer, schema mismatch) are unit-tested in the individual
//! modules. A production test suite would add fault injection here.

use neurogrim_a2a::agent_card::{
    Authentication, Capabilities, Transport as TransportCard, TransportProtocol,
};
use neurogrim_a2a::envelope::MessageType;
use neurogrim_a2a::{A2aEnvelope, AgentCard, TaskClient, TaskServer};
use serde_json::json;
use url::Url;

fn mock_card() -> AgentCard {
    AgentCard {
        schema_version: "1".into(),
        id: "mock-peer".into(),
        name: "Mock Peer Brain".into(),
        version: "0.0.1".into(),
        interface_version: "1".into(),
        capabilities: Capabilities {
            accepts: vec![MessageType::SnapshotRequested],
            emits: vec![MessageType::SnapshotDelivered],
            streaming: false,
        },
        transport: TransportCard {
            protocol: TransportProtocol::HttpSse,
            endpoint: "http://127.0.0.1/".into(),
            tasks_path: "/a2a/v1/tasks".into(),
        },
        authentication: Authentication::default(),
        topology: None,
        queue_endpoints: None,
    }
}

#[tokio::test]
async fn client_invokes_mock_peer_end_to_end() {
    // ---- Arrange: mock peer that answers snapshot.requested ----
    let mut server = TaskServer::new(mock_card());
    server.register_handler(MessageType::SnapshotRequested, |req| async move {
        let mut resp = A2aEnvelope::new(
            "mock-peer",
            MessageType::SnapshotDelivered,
            json!({"score": 77}),
        );
        resp.reply_to = Some(req.message_id);
        Ok(resp)
    });
    let router = server.router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    let endpoint = Url::parse(&format!("http://{addr}/")).unwrap();

    // ---- Act: discover then invoke ----
    let client = TaskClient::new_http();
    let card = client.discover(&endpoint).await.expect("discover succeeds");
    assert_eq!(card.id, "mock-peer");

    let request = A2aEnvelope::new("caller", MessageType::SnapshotRequested, json!({}));
    let original_id = request.message_id.clone();
    let response = client
        .invoke(&endpoint, request)
        .await
        .expect("invoke succeeds");

    // ---- Assert ----
    assert_eq!(response.message_type, MessageType::SnapshotDelivered);
    assert_eq!(response.reply_to, Some(original_id));
    assert_eq!(response.payload["score"], 77);

    server_handle.abort();
}
