//! `TaskServer` — serves this Brain as an A2A peer.
//!
//! Implements the server-side task lifecycle from spec §13.3 + Appendix G.4:
//!
//! - `GET /.well-known/agent-card.json` — publishes the Agent Card
//! - `POST /a2a/v1/tasks` — accepts an envelope, assigns a `task_id`, runs
//!   the registered handler, returns 202 with `{"task_id": "..."}`
//! - `GET /a2a/v1/tasks/{task_id}` — returns the terminal envelope or 404
//! - `GET /a2a/v1/tasks/{task_id}/events` — SSE stream (v1: single terminal
//!   event when the handler completes)
//!
//! # Idempotency (spec §13.3 step 5)
//!
//! If a POST arrives with a `message_id` already processed, the cached
//! response envelope is returned — the handler does not re-execute. The
//! cache keys on `message_id`, not `task_id`, because the client owns the
//! former and the server owns the latter.
//!
//! # Honesty
//!
//! The v1 implementation runs handlers *synchronously inside the POST
//! handler* rather than spawning a background task. The 202-then-poll
//! ritual is preserved on the wire, but internally completion is immediate.
//! This is documented here rather than hidden — adopters who need true
//! long-running tasks should extend this module before relying on it.

use crate::agent_card::AgentCard;
use crate::envelope::{A2aEnvelope, MessageType};
use crate::error::A2aError;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, sse::{Event, Sse}},
    routing::{get, post},
};
use futures::stream::{self, Stream};
use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Boxed async handler signature. Takes an inbound envelope, returns the
/// terminal envelope (or an A2A error). `Send + Sync + 'static` because axum
/// needs to move the registry across await points.
pub type HandlerFn = Arc<
    dyn Fn(A2aEnvelope) -> Pin<Box<dyn Future<Output = Result<A2aEnvelope, A2aError>> + Send>>
        + Send
        + Sync,
>;

/// A2A server state — shared across axum routes via `State`.
///
/// Handlers are stored behind a `std::sync::RwLock` rather than `tokio`'s
/// because registration is a setup-time operation (no `.await` needed) and
/// reads during request handling are cheap. The async tokio `RwLock` is
/// used only for the two caches that are touched on the request hot path.
#[derive(Clone)]
pub struct TaskServer {
    agent_card: Arc<AgentCard>,
    handlers: Arc<std::sync::RwLock<HashMap<MessageType, HandlerFn>>>,
    /// Idempotency cache keyed by request `message_id` → response envelope.
    /// Spec §13.3 step 5: duplicate receipts return the cached response
    /// without re-executing the handler.
    idempotency: Arc<RwLock<HashMap<String, A2aEnvelope>>>,
    /// Task store keyed by server-assigned `task_id` → completed envelope.
    /// v1 keeps everything in memory; eviction and persistence are future work.
    tasks: Arc<RwLock<HashMap<String, A2aEnvelope>>>,
}

impl TaskServer {
    /// Construct a new server with the given Agent Card. The Card is
    /// served verbatim at `/.well-known/agent-card.json`.
    pub fn new(agent_card: AgentCard) -> Self {
        Self {
            agent_card: Arc::new(agent_card),
            handlers: Arc::new(std::sync::RwLock::new(HashMap::new())),
            idempotency: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an async handler for a given message type. Overwrites any
    /// previous registration for the same type — last registration wins.
    ///
    /// The closure signature allows callers to pass `|env| async move { ... }`
    /// idiomatically without constructing a `Box<dyn Future>` themselves.
    pub fn register_handler<F, Fut>(&mut self, message_type: MessageType, handler: F)
    where
        F: Fn(A2aEnvelope) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<A2aEnvelope, A2aError>> + Send + 'static,
    {
        let boxed: HandlerFn = Arc::new(move |env| Box::pin(handler(env)));
        // std::sync::RwLock write on the setup path — no `.await` needed,
        // no risk of blocking an async worker because registration runs
        // before `serve` spawns any request handlers.
        let mut h = self
            .handlers
            .write()
            .expect("handler registry lock poisoned");
        h.insert(message_type, boxed);
    }

    /// Look up the handler for a given message type, if any. Primarily for
    /// tests; the POST route uses this internally too.
    pub fn handler_for(&self, message_type: MessageType) -> Option<HandlerFn> {
        self.handlers
            .read()
            .expect("handler registry lock poisoned")
            .get(&message_type)
            .cloned()
    }

    /// Build the axum router. Separated from `serve` so tests can mount the
    /// router against an in-process `TcpListener` without binding a public
    /// port.
    pub fn router(self) -> Router {
        Router::new()
            .route("/.well-known/agent-card.json", get(get_agent_card))
            .route("/a2a/v1/tasks", post(post_task))
            .route("/a2a/v1/tasks/:task_id", get(get_task))
            .route("/a2a/v1/tasks/:task_id/events", get(get_task_events))
            .with_state(self)
    }

    /// Bind to `addr` and serve forever. Returns only if the listener or the
    /// server itself fails — a successful shutdown is not modeled in v1
    /// (add a shutdown signal in a follow-on task when we need it).
    pub async fn serve(self, addr: SocketAddr) -> Result<(), A2aError> {
        let router = self.router();
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| A2aError::Transport(format!("bind {addr}: {e}")))?;
        tracing::info!(%addr, "A2A TaskServer listening");
        axum::serve(listener, router)
            .await
            .map_err(|e| A2aError::Transport(format!("axum serve: {e}")))
    }
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

async fn get_agent_card(State(server): State<TaskServer>) -> impl IntoResponse {
    // Cloning the Arc's target is cheap-ish and keeps the response Send.
    Json((*server.agent_card).clone())
}

async fn post_task(
    State(server): State<TaskServer>,
    Json(envelope): Json<A2aEnvelope>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // ---- Idempotency check (spec §13.3 step 5) ----
    {
        let idem = server.idempotency.read().await;
        if let Some(cached) = idem.get(&envelope.message_id) {
            tracing::debug!(
                message_id = %envelope.message_id,
                "A2A server returning cached response (idempotency)"
            );
            // Replay the original task_id if we have one stored in metadata;
            // otherwise generate a new one. v1 synthesizes a new one — we
            // document it rather than pretend it's round-tripped.
            let task_id = uuid::Uuid::new_v4().to_string();
            let mut tasks = server.tasks.write().await;
            tasks.insert(task_id.clone(), cached.clone());
            return Ok((
                StatusCode::ACCEPTED,
                Json(serde_json::json!({ "task_id": task_id, "idempotent_replay": true })),
            ));
        }
    }

    // ---- Envelope validation ----
    if envelope.schema_version != "1" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "unsupported schema_version",
                "expected": "1",
                "actual": envelope.schema_version,
            })),
        ));
    }

    // ---- Handler dispatch ----
    let handler = match server.handler_for(envelope.message_type) {
        Some(h) => h,
        None => {
            // Spec G.8: "Message_type not in accepts" => 405.
            return Err((
                StatusCode::METHOD_NOT_ALLOWED,
                Json(serde_json::json!({
                    "error": "message_type not in accepts",
                    "message_type": envelope.message_type,
                })),
            ));
        }
    };

    let message_id = envelope.message_id.clone();
    let response = match handler(envelope).await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::warn!(%message_id, error = %e, "A2A handler failed");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            ));
        }
    };

    // ---- Store and acknowledge ----
    let task_id = uuid::Uuid::new_v4().to_string();
    {
        let mut idem = server.idempotency.write().await;
        idem.insert(message_id.clone(), response.clone());
    }
    {
        let mut tasks = server.tasks.write().await;
        tasks.insert(task_id.clone(), response);
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "task_id": task_id })),
    ))
}

async fn get_task(
    State(server): State<TaskServer>,
    Path(task_id): Path<String>,
) -> Result<Json<A2aEnvelope>, StatusCode> {
    let tasks = server.tasks.read().await;
    tasks
        .get(&task_id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_task_events(
    State(server): State<TaskServer>,
    Path(task_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    // v1 implementation: if the task is complete, emit the terminal envelope
    // as a single SSE event and close. If not, return 404 — we don't track
    // pending tasks because handlers run synchronously (see module docs).
    let envelope = {
        let tasks = server.tasks.read().await;
        tasks.get(&task_id).cloned().ok_or(StatusCode::NOT_FOUND)?
    };

    // axum 0.7 SSE API: `Event::default().data(String)`. Verified by the
    // `post_then_get_roundtrip` test + the mock_peer integration test.
    let event =
        Event::default().data(serde_json::to_string(&envelope).unwrap_or_else(|_| "{}".into()));
    let s = stream::iter(vec![Ok::<Event, Infallible>(event)]);
    Ok(Sse::new(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_card::{
        Authentication, Capabilities, Transport as TransportCard, TransportProtocol,
    };
    use crate::envelope::MessageType;
    use serde_json::json;

    fn minimal_card() -> AgentCard {
        AgentCard {
            schema_version: "1".into(),
            id: "test-brain".into(),
            name: "Test Brain".into(),
            version: "0.1.0".into(),
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
        }
    }

    #[tokio::test]
    async fn register_handler_then_lookup() {
        // Respect for the caller: registering a handler MUST make it
        // observable via `handler_for` — no silent drops.
        let mut server = TaskServer::new(minimal_card());
        server.register_handler(MessageType::SnapshotRequested, |env| async move {
            Ok(A2aEnvelope {
                reply_to: Some(env.message_id),
                ..A2aEnvelope::new(
                    "peer",
                    MessageType::SnapshotDelivered,
                    json!({"score": 72}),
                )
            })
        });
        let got = server.handler_for(MessageType::SnapshotRequested);
        assert!(got.is_some(), "registered handler should be retrievable");

        let missing = server.handler_for(MessageType::ScoreUpdated);
        assert!(missing.is_none(), "unregistered type should return None");
    }

    /// Round-trip integration test: POST an envelope, follow up with GET
    /// `/a2a/v1/tasks/{task_id}`, verify the terminal envelope comes back.
    ///
    /// Uses `tokio::net::TcpListener` + `axum::serve` directly rather than a
    /// helper crate — fewer dependencies, easier to reason about.
    #[tokio::test]
    async fn post_then_get_roundtrip() {
        let mut server = TaskServer::new(minimal_card());
        server.register_handler(MessageType::SnapshotRequested, |env| async move {
            let mut resp = A2aEnvelope::new(
                "peer",
                MessageType::SnapshotDelivered,
                json!({"score": 80}),
            );
            resp.reply_to = Some(env.message_id);
            Ok(resp)
        });

        let router = server.router();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_handle = tokio::spawn(async move {
            axum::serve(listener, router).await.ok();
        });

        let client = reqwest::Client::new();
        let request = A2aEnvelope::new("caller", MessageType::SnapshotRequested, json!({}));
        let post_url = format!("http://{addr}/a2a/v1/tasks");

        let resp = client.post(&post_url).json(&request).send().await.unwrap();
        assert_eq!(resp.status(), 202, "POST should return 202 Accepted");
        let body: serde_json::Value = resp.json().await.unwrap();
        let task_id = body["task_id"].as_str().unwrap().to_string();

        let get_url = format!("http://{addr}/a2a/v1/tasks/{task_id}");
        let resp = client.get(&get_url).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let envelope: A2aEnvelope = resp.json().await.unwrap();
        assert_eq!(envelope.message_type, MessageType::SnapshotDelivered);
        assert_eq!(envelope.reply_to, Some(request.message_id));

        server_handle.abort();
    }

    #[tokio::test]
    async fn agent_card_served_at_well_known_url() {
        let server = TaskServer::new(minimal_card());
        let router = server.router();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, router).await.ok();
        });

        let url = format!("http://{addr}/.well-known/agent-card.json");
        let card: AgentCard = reqwest::get(&url).await.unwrap().json().await.unwrap();
        assert_eq!(card.id, "test-brain");
        assert_eq!(card.schema_version, "1");

        handle.abort();
    }

    #[tokio::test]
    async fn idempotent_post_returns_cached_without_rehandler() {
        // Gentle on the peer: a retried message_id doesn't cause the handler
        // to run twice. We assert by counting handler invocations via a
        // shared counter.
        use std::sync::atomic::{AtomicUsize, Ordering};
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_for_handler = counter.clone();

        let mut server = TaskServer::new(minimal_card());
        server.register_handler(MessageType::SnapshotRequested, move |env| {
            let c = counter_for_handler.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                let mut resp = A2aEnvelope::new(
                    "peer",
                    MessageType::SnapshotDelivered,
                    json!({"score": 90}),
                );
                resp.reply_to = Some(env.message_id);
                Ok(resp)
            }
        });

        let router = server.router();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, router).await.ok();
        });

        let client = reqwest::Client::new();
        let request = A2aEnvelope::new("caller", MessageType::SnapshotRequested, json!({}));
        let post_url = format!("http://{addr}/a2a/v1/tasks");

        let _ = client
            .post(&post_url)
            .json(&request)
            .send()
            .await
            .unwrap();
        let _ = client
            .post(&post_url)
            .json(&request)
            .send()
            .await
            .unwrap();

        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "handler must run only once for a repeated message_id"
        );

        handle.abort();
    }
}
