//! End-to-end integration tests for A2A bearer authentication.
//!
//! These verify the full request path — middleware, token store, Agent Card
//! publication — through a real `tokio::net::TcpListener` + axum server on a
//! loopback port. The client side uses `reqwest` directly (for raw-response
//! assertions) and the `TaskClient` with `with_bearer_token` (for the
//! happy-path smoke test).
//!
//! Scenarios (from the Phase 1 plan in
//! `~/.claude/plans/nice-i-like-it-delightful-matsumoto.md`):
//!
//! - Agent Card advertises `scheme: bearer` and stays public (no auth).
//! - Protected routes reject: missing header, malformed header, unknown,
//!   revoked, and expired tokens. All return 401; body carries a generic
//!   detail — not a distinguishing reason (no info leak to attacker).
//! - Valid token passes: 202 Accepted + `task_id`.
//! - A `TaskClient::new_http_with_bearer` reaches the same peer successfully.

use neurogrim_a2a::agent_card::{
    AgentCard, AuthScheme, Authentication, Capabilities, Transport as TransportCard,
    TransportProtocol,
};
use neurogrim_a2a::envelope::MessageType;
use neurogrim_a2a::token_store::TokenStore;
use neurogrim_a2a::{A2aEnvelope, TaskClient, TaskServer};
use serde_json::json;
use tempfile::TempDir;
use url::Url;

/// Agent Card that advertises bearer auth — the scheme the middleware checks.
fn bearer_card() -> AgentCard {
    AgentCard {
        schema_version: "1".into(),
        id: "auth-test-peer".into(),
        name: "Auth Test Peer".into(),
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
        authentication: Authentication {
            scheme: AuthScheme::Bearer,
        },
        topology: None,
        queue_endpoints: None,
    }
}

/// Shared test rig: spins up a TaskServer with bearer auth, a fresh on-disk
/// TokenStore, and returns everything the caller needs to drive the peer.
struct Rig {
    addr: std::net::SocketAddr,
    _dir: TempDir,
    store_path: std::path::PathBuf,
    _handle: tokio::task::JoinHandle<()>,
}

async fn spawn_rig() -> Rig {
    let dir = tempfile::tempdir().expect("tempdir");
    let store_path = dir.path().join("tokens.sqlite");
    let store = TokenStore::open(&store_path).expect("open store");

    let mut server = TaskServer::new(bearer_card()).with_token_store(store);
    server.register_handler(MessageType::SnapshotRequested, |req| async move {
        let mut resp = A2aEnvelope::new(
            "auth-test-peer",
            MessageType::SnapshotDelivered,
            json!({"score": 88}),
        );
        resp.reply_to = Some(req.message_id);
        Ok(resp)
    });

    let router = server.router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    Rig {
        addr,
        _dir: dir,
        store_path,
        _handle: handle,
    }
}

fn issue_token(store_path: &std::path::Path, label: &str, expires_in: Option<i64>) -> String {
    // Reopen the store — the server holds its own handle, and sqlite WAL
    // lets two connections cooperate safely on the same file.
    let store = TokenStore::open(store_path).expect("reopen store");
    let (raw, _rec) = store.issue(label, "default", expires_in).expect("issue");
    raw
}

#[tokio::test]
async fn agent_card_advertises_bearer_scheme_and_stays_public() {
    // Positivity: peers can always discover the auth requirements without
    // presenting credentials — otherwise bootstrap is impossible.
    let rig = spawn_rig().await;
    let url = format!("http://{}/.well-known/agent-card.json", rig.addr);
    let resp = reqwest::get(&url).await.expect("get card");
    assert_eq!(resp.status(), 200, "Agent Card must be publicly readable");
    let card: AgentCard = resp.json().await.expect("parse card");
    assert_eq!(
        card.authentication.scheme,
        AuthScheme::Bearer,
        "card must advertise bearer scheme"
    );
}

#[tokio::test]
async fn missing_authorization_header_returns_401() {
    let rig = spawn_rig().await;
    let client = reqwest::Client::new();
    let env = A2aEnvelope::new("caller", MessageType::SnapshotRequested, json!({}));
    let url = format!("http://{}/a2a/v1/tasks", rig.addr);
    let resp = client.post(&url).json(&env).send().await.expect("post");
    assert_eq!(resp.status(), 401);
    let www_auth = resp
        .headers()
        .get("www-authenticate")
        .map(|h| h.to_str().unwrap_or("").to_string())
        .unwrap_or_default();
    assert!(
        www_auth.starts_with("Bearer"),
        "401 must carry WWW-Authenticate: Bearer (got {www_auth:?})"
    );
}

#[tokio::test]
async fn malformed_authorization_header_returns_401() {
    let rig = spawn_rig().await;
    let client = reqwest::Client::new();
    let env = A2aEnvelope::new("caller", MessageType::SnapshotRequested, json!({}));
    let url = format!("http://{}/a2a/v1/tasks", rig.addr);
    // "Basic foo" is not Bearer; must be rejected.
    let resp = client
        .post(&url)
        .header("Authorization", "Basic foo")
        .json(&env)
        .send()
        .await
        .expect("post");
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn unknown_token_returns_401() {
    let rig = spawn_rig().await;
    let client = reqwest::Client::new();
    let env = A2aEnvelope::new("caller", MessageType::SnapshotRequested, json!({}));
    let url = format!("http://{}/a2a/v1/tasks", rig.addr);
    let resp = client
        .post(&url)
        .bearer_auth("nb_sat_definitelynotreal0000000000000")
        .json(&env)
        .send()
        .await
        .expect("post");
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn revoked_token_returns_401() {
    let rig = spawn_rig().await;
    let raw = issue_token(&rig.store_path, "revoke-test", None);
    // Revoke via a second handle on the same store file.
    let store = TokenStore::open(&rig.store_path).expect("reopen");
    let records = store.list_all().expect("list");
    let token_id = records
        .iter()
        .find(|r| r.label == "revoke-test")
        .expect("row")
        .token_id
        .clone();
    assert!(store.revoke(&token_id).expect("revoke"));

    let client = reqwest::Client::new();
    let env = A2aEnvelope::new("caller", MessageType::SnapshotRequested, json!({}));
    let url = format!("http://{}/a2a/v1/tasks", rig.addr);
    let resp = client
        .post(&url)
        .bearer_auth(&raw)
        .json(&env)
        .send()
        .await
        .expect("post");
    assert_eq!(
        resp.status(),
        401,
        "revoked token must not grant access"
    );
}

#[tokio::test]
async fn expired_token_returns_401() {
    let rig = spawn_rig().await;
    // `expires_in = -1` → already expired.
    let raw = issue_token(&rig.store_path, "expired-test", Some(-1));

    let client = reqwest::Client::new();
    let env = A2aEnvelope::new("caller", MessageType::SnapshotRequested, json!({}));
    let url = format!("http://{}/a2a/v1/tasks", rig.addr);
    let resp = client
        .post(&url)
        .bearer_auth(&raw)
        .json(&env)
        .send()
        .await
        .expect("post");
    assert_eq!(
        resp.status(),
        401,
        "expired token must not grant access"
    );
}

#[tokio::test]
async fn valid_token_returns_202_accepted() {
    let rig = spawn_rig().await;
    let raw = issue_token(&rig.store_path, "valid-test", None);

    let client = reqwest::Client::new();
    let env = A2aEnvelope::new("caller", MessageType::SnapshotRequested, json!({}));
    let url = format!("http://{}/a2a/v1/tasks", rig.addr);
    let resp = client
        .post(&url)
        .bearer_auth(&raw)
        .json(&env)
        .send()
        .await
        .expect("post");
    assert_eq!(
        resp.status(),
        202,
        "valid token must accept the task"
    );
    let body: serde_json::Value = resp.json().await.expect("json");
    assert!(
        body.get("task_id").is_some(),
        "202 body must carry task_id; got {body}"
    );
}

#[tokio::test]
async fn task_client_with_bearer_completes_round_trip() {
    // Validates the client-side wiring: `new_http_with_bearer` plus the
    // downstream `bearer_auth` on poll_task both carry the token all the
    // way through to the terminal envelope.
    let rig = spawn_rig().await;
    let raw = issue_token(&rig.store_path, "client-test", None);

    let client = TaskClient::new_http_with_bearer(raw);
    let url = Url::parse(&format!("http://{}/", rig.addr)).unwrap();
    let request = A2aEnvelope::new("caller", MessageType::SnapshotRequested, json!({}));
    let response = client
        .invoke(&url, request.clone())
        .await
        .expect("client invoke");
    assert_eq!(response.message_type, MessageType::SnapshotDelivered);
    assert_eq!(response.reply_to, Some(request.message_id));
}

#[tokio::test]
async fn unauthorized_body_does_not_distinguish_revoked_from_expired() {
    // Integrity: an attacker probing tokens must not be able to tell
    // "unknown" from "revoked" from "expired". Our server emits a single
    // generic detail string; this test pins that posture.
    let rig = spawn_rig().await;
    let raw_revoked = issue_token(&rig.store_path, "probe-revoked", None);
    let store = TokenStore::open(&rig.store_path).unwrap();
    let records = store.list_all().unwrap();
    let rev_id = records
        .iter()
        .find(|r| r.label == "probe-revoked")
        .unwrap()
        .token_id
        .clone();
    store.revoke(&rev_id).unwrap();

    let raw_expired = issue_token(&rig.store_path, "probe-expired", Some(-1));

    let client = reqwest::Client::new();
    let env = A2aEnvelope::new("c", MessageType::SnapshotRequested, json!({}));
    let url = format!("http://{}/a2a/v1/tasks", rig.addr);

    let mut bodies = Vec::new();
    for tok in [raw_revoked, raw_expired, "nb_sat_unknowntoken".into()].into_iter() {
        let resp = client
            .post(&url)
            .bearer_auth(&tok)
            .json(&env)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 401);
        bodies.push(resp.text().await.unwrap());
    }
    let first = &bodies[0];
    for other in &bodies[1..] {
        assert_eq!(
            first, other,
            "401 bodies must be identical across reasons; got {first:?} vs {other:?}"
        );
    }
}
